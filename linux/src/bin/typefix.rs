//! TypeFix for Linux: command-line, foreground X11 frontend.
//!
//! A configurable global shortcut converts the currently selected text between
//! the English and Persian keyboard layouts. Optional `--suggest` mode also
//! watches typing and shows layout-mistake suggestions as notifications.
//!
//! The settings window (`typefix-gui`) uses the same service and config file.
//! Typed or selected text is kept in memory only. See linux/SECURITY.md.

use std::env;
use std::path::PathBuf;
use std::thread;

use typefix_core::{
    config::{config_path, Config},
    service::Service,
    shortcut::Shortcut,
    Language,
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
        eprintln!("Settings are read from {} (also edited by the typefix-gui window).", path.display());
        eprintln!("Command-line options override the config file.");
    }
    eprintln!();
    eprintln!("Select text in any application and press the shortcut: English-layout text is");
    eprintln!("replaced by its Persian-layout equivalent and vice versa. The converted text is");
    eprintln!("pasted with Shift+Insert, so the clipboard is briefly used. Runs in the");
    eprintln!("foreground until Ctrl+C. Needs an X11 session, setxkbmap and both a US and an");
    eprintln!("Iranian (ir) layout. notify-send is used for messages when available.");
}

fn run() -> Result<(), String> {
    let config = Config::load()?;
    let mut shortcut_text = config.shortcut.unwrap_or_else(|| DEFAULT_SHORTCUT.to_owned());
    let mut suggest = config.suggest.unwrap_or(false);
    let mut source = match config.source.as_deref() {
        Some("fa") => Language::Persian,
        _ => Language::English,
    };
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
    let mut service = Service::new();
    service.set_shortcut(&shortcut)?;

    eprintln!("TypeFix {VERSION} is running in the foreground. Stop with Ctrl+C.");
    eprintln!("Select text and press {shortcut} to convert it between English and Persian layouts.");

    if suggest {
        service.set_suggest(true, source, data_dir)?;
        eprintln!("Suggestion mode is on: key events on this X server are observed (X11 has no");
        eprintln!("per-application isolation). Typed text is kept in memory only and never stored.");
    }

    // The workers run on their own threads; keep the service alive until Ctrl+C.
    loop {
        thread::park();
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("typefix: {error}");
        std::process::exit(1);
    }
}
