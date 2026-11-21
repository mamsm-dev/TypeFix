//! TypeFix for Linux: explicit, foreground X11 frontend.
//!
//! Default mode: a configurable global shortcut converts the currently
//! selected text between the English and Persian keyboard layouts. Only that
//! one key combination is grabbed; typing is not observed.
//!
//! Optional `--suggest` mode additionally watches key events on this X server
//! and shows layout-mistake suggestions as desktop notifications.
//!
//! Typed or selected text is kept in memory only. No network access, no
//! persistent history and no privileged device access. See linux/SECURITY.md.

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use arboard::{Clipboard, GetExtLinux, LinuxClipboardKind, SetExtLinux};
use typefix_core::{
    compose::ComposeProcessor,
    config::{config_path, Config},
    convert::convert_selection,
    hotkey::HotkeyListener,
    session::{ComposedTypingSession, SessionAction},
    shortcut::Shortcut,
    x11::X11SessionConnection,
    Dictionaries, Language, LayoutMap, Suggestion,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_SHORTCUT: &str = "Ctrl+Alt+Space";

fn usage() {
    eprintln!("TypeFix {VERSION} - English/Persian keyboard layout converter (X11)");
    eprintln!();
    eprintln!("Usage: typefix [--shortcut KEYS] [--suggest] [--source en|fa] [--data-dir DIR]");
    eprintln!();
    eprintln!("  --shortcut KEYS  shortcut that converts the selected text (default {DEFAULT_SHORTCUT})");
    eprintln!("                   examples: Ctrl+Alt+Space, Super+Shift+F9, Ctrl+Alt+c");
    eprintln!("  --suggest        also watch typing and suggest conversions as notifications");
    eprintln!("  --source en|fa   direction for --suggest (default en)");
    eprintln!("  --data-dir DIR   directory with en_50k.txt and fa_50k.txt (for --suggest)");
    eprintln!();
    if let Some(path) = config_path() {
        eprintln!("Settings can also be kept in {} (shortcut = ..., suggest = true).", path.display());
        eprintln!("Command-line options override the config file.");
    }
    eprintln!();
    eprintln!("Select text in any application and press the shortcut: English-layout text is");
    eprintln!("replaced by its Persian-layout equivalent and vice versa. The converted text is");
    eprintln!("pasted with Shift+Insert, so the clipboard is briefly used. Runs in the");
    eprintln!("foreground until Ctrl+C. Needs an X11 session, setxkbmap and both a US and an");
    eprintln!("Iranian (ir) layout. notify-send is used for messages when available.");
}

fn find_data_dir(explicit: Option<PathBuf>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = explicit {
        candidates.push(dir);
    }
    if let Some(dir) = env::var_os("TYPEFIX_DATA_DIR") {
        candidates.push(PathBuf::from(dir));
    }
    if let Some(bin_dir) = env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        // AppImage / relocatable layout: <prefix>/bin/typefix + <prefix>/share/typefix
        candidates.push(bin_dir.join("../share/typefix"));
    }
    candidates.push(PathBuf::from("/usr/share/typefix"));
    candidates.push(PathBuf::from("/usr/local/share/typefix"));
    // Development checkout.
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../Resources"));

    candidates
        .into_iter()
        .find(|dir| dir.join("en_50k.txt").is_file() && dir.join("fa_50k.txt").is_file())
}

fn locale() -> OsString {
    for key in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Some(value) = env::var_os(key) {
            if !value.is_empty() {
                return value;
            }
        }
    }
    OsString::from("en_US.UTF-8")
}

/// Builds the layout conversion table from the active XKB keymap: every key is
/// translated in the source layout group and in the target layout group, with
/// and without Shift.
fn build_layout_map(
    connection: &X11SessionConnection,
    source: Language,
) -> Result<LayoutMap, String> {
    let layouts = &connection.config.layouts;
    let persian = layouts
        .iter()
        .position(|name| name == "ir")
        .ok_or_else(|| "no Persian (ir) keyboard layout is configured; add one in your desktop keyboard settings".to_owned())?;
    let english = layouts
        .iter()
        .position(|name| name == "us")
        .or_else(|| layouts.iter().position(|name| name != "ir"))
        .ok_or_else(|| "no English keyboard layout is configured".to_owned())?;

    let (from_group, to_group) = match source {
        Language::English => (english as u32, persian as u32),
        Language::Persian => (persian as u32, english as u32),
    };

    let mut map = LayoutMap::default();
    let mut seen = std::collections::HashSet::new();
    for modifiers in [0u32, 1u32] {
        for keycode in 8u8..=255u8 {
            let from = connection.keymap.translate_keycode(keycode, from_group, modifiers);
            let to = connection.keymap.translate_keycode(keycode, to_group, modifiers);
            let (Some(from), Some(to)) = (from, to) else { continue };
            let mut chars = from.chars();
            let (Some(ch), None) = (chars.next(), chars.next()) else { continue };
            if seen.insert(ch) {
                map.insert(ch, to);
            }
        }
    }
    Ok(map)
}

fn notify(title: &str, body: &str) {
    let spawned = Command::new("notify-send")
        .args(["--app-name=TypeFix", "--expire-time=5000", title, body])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match spawned {
        Ok(mut child) => {
            // Reap the short-lived notify-send process.
            thread::spawn(move || {
                let _ = child.wait();
            });
        }
        // Fallback UI only: shown on this terminal, never written to a file.
        Err(_) => eprintln!("TypeFix: {title}: {body}"),
    }
}

fn notify_suggestion(suggestion: &Suggestion) {
    let body = format!("{}  \u{2192}  {}", suggestion.original, suggestion.converted);
    notify("TypeFix", &body);
}

/// Handles one press of the conversion shortcut. The selected text is read
/// from the PRIMARY selection, converted in memory, and pasted over the
/// selection with Shift+Insert. The previous clipboard text is restored.
fn convert_current_selection(
    listener: &HotkeyListener,
    clipboard: &mut Clipboard,
    en_to_fa: &LayoutMap,
    fa_to_en: &LayoutMap,
) -> Result<(), String> {
    listener.wait_for_modifier_release();

    let selected = clipboard
        .get()
        .clipboard(LinuxClipboardKind::Primary)
        .text()
        .unwrap_or_default();
    if selected.is_empty() {
        notify("TypeFix", "No text is selected.");
        return Ok(());
    }
    let Some(converted) = convert_selection(&selected, en_to_fa, fa_to_en) else {
        notify("TypeFix", "Nothing to convert in the selected text.");
        return Ok(());
    };

    let previous = clipboard
        .get()
        .clipboard(LinuxClipboardKind::Clipboard)
        .text()
        .ok();
    clipboard
        .set()
        .clipboard(LinuxClipboardKind::Primary)
        .text(converted.clone())
        .map_err(|e| format!("could not set the selection: {e}"))?;
    clipboard
        .set()
        .clipboard(LinuxClipboardKind::Clipboard)
        .text(converted)
        .map_err(|e| format!("could not set the clipboard: {e}"))?;

    thread::sleep(Duration::from_millis(60));
    listener.send_paste()?;
    thread::sleep(Duration::from_millis(400));

    if let Some(previous) = previous {
        let _ = clipboard
            .set()
            .clipboard(LinuxClipboardKind::Clipboard)
            .text(previous);
    }
    Ok(())
}

/// Optional typing-suggestion mode, run on its own thread with its own X connection.
fn run_suggest_mode(source: Language, data_dir: Option<PathBuf>) -> Result<(), String> {
    let dir = find_data_dir(data_dir)
        .ok_or("cannot find en_50k.txt and fa_50k.txt; use --data-dir or set TYPEFIX_DATA_DIR")?;
    let dicts = Dictionaries::load(dir.join("en_50k.txt"), dir.join("fa_50k.txt"))
        .map_err(|e| format!("cannot load dictionaries from {}: {e}", dir.display()))?;

    let mut connection = X11SessionConnection::connect()?;
    let map = build_layout_map(&connection, source)?;
    let compose = ComposeProcessor::new(&locale())
        .or_else(|_| ComposeProcessor::new(std::ffi::OsStr::new("en_US.UTF-8")))
        .map_err(|e| format!("cannot initialise the Compose table: {e}"))?;
    let mut session = ComposedTypingSession::new(compose);

    connection.run_session_loop(
        &mut session,
        &dicts,
        source,
        &map,
        |action| {
            if let SessionAction::Suggestion(suggestion) = action {
                notify_suggestion(&suggestion);
            }
        },
        || false,
    )
}

fn run() -> Result<(), String> {
    let config = Config::load()?;
    let mut shortcut_text = config.shortcut.unwrap_or_else(|| DEFAULT_SHORTCUT.to_owned());
    let mut suggest = config.suggest.unwrap_or(false);
    let mut source = Language::English;
    let mut data_dir: Option<PathBuf> = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                usage();
                return Ok(());
            }
            "-V" | "--version" => {
                println!("typefix {VERSION}");
                return Ok(());
            }
            "--shortcut" => {
                shortcut_text = args.next().ok_or("--shortcut needs a value, e.g. Ctrl+Alt+Space")?;
            }
            "--suggest" => suggest = true,
            "--source" => {
                let value = args.next().ok_or("--source needs a value (en or fa)")?;
                source = Language::parse(&value).ok_or("--source must be en or fa")?;
            }
            "--data-dir" => {
                data_dir = Some(PathBuf::from(args.next().ok_or("--data-dir needs a directory")?));
            }
            other => return Err(format!("unknown argument: {other} (see --help)")),
        }
    }

    if env::var_os("DISPLAY").is_none() {
        return Err("no X11 display found (DISPLAY is not set). Wayland-only sessions are not supported yet".into());
    }

    let shortcut = Shortcut::parse(&shortcut_text)?;

    let connection = X11SessionConnection::connect()?;
    let en_to_fa = build_layout_map(&connection, Language::English)?;
    let fa_to_en = build_layout_map(&connection, Language::Persian)?;
    drop(connection);

    let listener = HotkeyListener::new(&shortcut)?;
    let mut clipboard = Clipboard::new().map_err(|e| format!("cannot access the clipboard: {e}"))?;

    eprintln!("TypeFix {VERSION} is running in the foreground. Stop with Ctrl+C.");
    eprintln!("Select text and press {shortcut} to convert it between English and Persian layouts.");

    if suggest {
        eprintln!("Suggestion mode is on: key events on this X server are observed (X11 has no");
        eprintln!("per-application isolation). Typed text is kept in memory only and never stored.");
        thread::spawn(move || {
            if let Err(error) = run_suggest_mode(source, data_dir) {
                eprintln!("typefix: suggestion mode stopped: {error}");
                std::process::exit(1);
            }
        });
    }

    listener.run(|| {
        if let Err(error) = convert_current_selection(&listener, &mut clipboard, &en_to_fa, &fa_to_en) {
            notify("TypeFix", &format!("Conversion failed: {error}"));
        }
    })
}

fn main() {
    if let Err(error) = run() {
        eprintln!("typefix: {error}");
        std::process::exit(1);
    }
}
