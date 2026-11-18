//! TypeFix for Linux: explicit, foreground X11 frontend.
//!
//! Observes key events on the current X server (opt-in, runs only while this
//! process is alive), detects English/Persian keyboard-layout mistakes with the
//! shared core and shows the converted text as a desktop notification.
//!
//! It never injects or replaces text, never logs or persists typed content and
//! uses no network or privileged device access. See linux/SECURITY.md.

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use typefix_core::{
    compose::ComposeProcessor,
    session::{ComposedTypingSession, SessionAction},
    x11::X11SessionConnection,
    Dictionaries, Language, LayoutMap, Suggestion,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn usage() {
    eprintln!("TypeFix {VERSION} - keyboard layout mistake detector (English/Persian, X11)");
    eprintln!();
    eprintln!("Usage: typefix [--source en|fa] [--data-dir DIR]");
    eprintln!();
    eprintln!("  --source en   text typed with the English layout, suggest Persian (default)");
    eprintln!("  --source fa   text typed with the Persian layout, suggest English");
    eprintln!("  --data-dir    directory containing en_50k.txt and fa_50k.txt");
    eprintln!();
    eprintln!("Runs in the foreground and observes key events on this X server until");
    eprintln!("you stop it with Ctrl+C. Requires an X11 session with a US/English and an");
    eprintln!("Iranian (ir) layout configured, and the setxkbmap tool. Suggestions are");
    eprintln!("shown with notify-send when available. Text is never replaced or stored.");
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

fn notify(suggestion: &Suggestion) {
    let body = format!("{}  \u{2192}  {}", suggestion.original, suggestion.converted);
    let spawned = Command::new("notify-send")
        .args(["--app-name=TypeFix", "--expire-time=5000", "TypeFix", &body])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match spawned {
        Ok(mut child) => {
            // Reap the short-lived notify-send process.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        // Fallback UI only: shown on this terminal, never written to a file.
        Err(_) => eprintln!("TypeFix suggestion: {body}"),
    }
}

fn run() -> Result<(), String> {
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

    eprintln!("TypeFix {VERSION} is running in the foreground.");
    eprintln!("It observes key events on this X server (X11 has no per-application isolation).");
    eprintln!("Typed text is kept in memory only and is never stored. Stop with Ctrl+C.");

    connection.run_session_loop(
        &mut session,
        &dicts,
        source,
        &map,
        |action| {
            if let SessionAction::Suggestion(suggestion) = action {
                notify(&suggestion);
            }
        },
        || false,
    )
}

fn main() {
    if let Err(error) = run() {
        eprintln!("typefix: {error}");
        std::process::exit(1);
    }
}
