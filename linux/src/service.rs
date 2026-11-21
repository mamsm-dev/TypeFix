//! Background workers shared by the command-line app and the settings UI:
//! the conversion shortcut and the optional typing-suggestion loop.
//!
//! Each worker runs on its own thread with its own X11 connection and can be
//! stopped and restarted at any time (for example when the user picks a new
//! shortcut in the UI). Selected and typed text stays in memory only.

use std::{
    env,
    ffi::OsString,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use arboard::{Clipboard, GetExtLinux, LinuxClipboardKind, SetExtLinux};

use crate::{
    compose::ComposeProcessor,
    convert::convert_selection,
    hotkey::HotkeyListener,
    session::{ComposedTypingSession, SessionAction},
    shortcut::Shortcut,
    x11::X11SessionConnection,
    Dictionaries, Language, LayoutMap, Suggestion,
};

/// Builds the layout conversion table from the active XKB keymap: every key is
/// translated in the source layout group and in the target layout group, with
/// and without Shift.
pub fn build_layout_map(
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

fn layout_maps() -> Result<(LayoutMap, LayoutMap), String> {
    let connection = X11SessionConnection::connect()?;
    Ok((
        build_layout_map(&connection, Language::English)?,
        build_layout_map(&connection, Language::Persian)?,
    ))
}

/// Shows a desktop notification with `notify-send`, falling back to stderr.
pub fn notify(title: &str, body: &str) {
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

/// Finds the directory holding `en_50k.txt` and `fa_50k.txt`.
pub fn find_data_dir(explicit: Option<PathBuf>) -> Option<PathBuf> {
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

struct Worker {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl Worker {
    fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.handle.join();
    }
}

/// Owns the shortcut worker and the optional suggestion worker.
#[derive(Default)]
pub struct Service {
    hotkey: Option<Worker>,
    suggest: Option<Worker>,
}

impl Service {
    pub fn new() -> Self {
        Self::default()
    }

    /// (Re)starts the conversion shortcut. The previous shortcut is released
    /// first. On error nothing is grabbed (for example when another
    /// application already uses the combination).
    pub fn set_shortcut(&mut self, shortcut: &Shortcut) -> Result<(), String> {
        if let Some(worker) = self.hotkey.take() {
            worker.stop();
        }
        let (en_to_fa, fa_to_en) = layout_maps()?;
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let thread_stop = Arc::clone(&stop);
        let shortcut = shortcut.clone();

        let handle = thread::spawn(move || {
            let listener = match HotkeyListener::new(&shortcut) {
                Ok(listener) => listener,
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };
            let mut clipboard = match Clipboard::new() {
                Ok(clipboard) => clipboard,
                Err(error) => {
                    let _ = ready_tx.send(Err(format!("cannot access the clipboard: {error}")));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));
            let result = listener.run_until(&thread_stop, || {
                if let Err(error) =
                    convert_current_selection(&listener, &mut clipboard, &en_to_fa, &fa_to_en)
                {
                    notify("TypeFix", &format!("Conversion failed: {error}"));
                }
            });
            if let Err(error) = result {
                notify("TypeFix", &format!("Shortcut stopped: {error}"));
            }
        });

        match ready_rx.recv() {
            Ok(Ok(())) => {
                self.hotkey = Some(Worker { stop, handle });
                Ok(())
            }
            Ok(Err(error)) => {
                let _ = handle.join();
                Err(error)
            }
            Err(_) => {
                let _ = handle.join();
                Err("the shortcut worker stopped unexpectedly".to_owned())
            }
        }
    }

    pub fn stop_shortcut(&mut self) {
        if let Some(worker) = self.hotkey.take() {
            worker.stop();
        }
    }

    /// Turns the typing-suggestion mode on or off. When on, key events on this
    /// X server are observed and suggestions are shown as notifications.
    pub fn set_suggest(
        &mut self,
        enabled: bool,
        source: Language,
        data_dir: Option<PathBuf>,
    ) -> Result<(), String> {
        if let Some(worker) = self.suggest.take() {
            worker.stop();
        }
        if !enabled {
            return Ok(());
        }

        let dir = find_data_dir(data_dir)
            .ok_or("cannot find en_50k.txt and fa_50k.txt; set TYPEFIX_DATA_DIR")?;
        let dicts = Dictionaries::load(dir.join("en_50k.txt"), dir.join("fa_50k.txt"))
            .map_err(|e| format!("cannot load dictionaries from {}: {e}", dir.display()))?;
        let map = {
            let connection = X11SessionConnection::connect()?;
            build_layout_map(&connection, source)?
        };

        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let thread_stop = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            let setup = || -> Result<(X11SessionConnection, ComposedTypingSession), String> {
                let connection = X11SessionConnection::connect()?;
                let compose = ComposeProcessor::new(&locale())
                    .or_else(|_| ComposeProcessor::new(std::ffi::OsStr::new("en_US.UTF-8")))
                    .map_err(|e| format!("cannot initialise the Compose table: {e}"))?;
                Ok((connection, ComposedTypingSession::new(compose)))
            };
            let (mut connection, mut session) = match setup() {
                Ok(parts) => parts,
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));
            let result = connection.run_session_loop(
                &mut session,
                &dicts,
                source,
                &map,
                |action| {
                    if let SessionAction::Suggestion(suggestion) = action {
                        notify_suggestion(&suggestion);
                    }
                },
                || thread_stop.load(Ordering::Relaxed),
            );
            if let Err(error) = result {
                notify("TypeFix", &format!("Suggestions stopped: {error}"));
            }
        });

        match ready_rx.recv() {
            Ok(Ok(())) => {
                self.suggest = Some(Worker { stop, handle });
                Ok(())
            }
            Ok(Err(error)) => {
                let _ = handle.join();
                Err(error)
            }
            Err(_) => {
                let _ = handle.join();
                Err("the suggestion worker stopped unexpectedly".to_owned())
            }
        }
    }

    pub fn stop(&mut self) {
        self.stop_shortcut();
        if let Some(worker) = self.suggest.take() {
            worker.stop();
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.stop();
    }
}
