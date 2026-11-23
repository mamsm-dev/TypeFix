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
    x11::{X11SessionConnection, XkbConfig, XkbKeymap},
    Dictionaries, Language, LayoutMap, Suggestion,
};

/// Builds the layout conversion table from the active XKB keymap: every key is
/// translated in the source layout group and in the target layout group, with
/// and without Shift.
pub fn build_layout_map(
    connection: &X11SessionConnection,
    source: Language,
) -> Result<LayoutMap, String> {
    connection.layout_map(source)
}

/// Same as [`build_layout_map`] but from any compiled keymap, so it also works
/// without an X server (Wayland).
pub fn build_layout_map_from(
    keymap: &XkbKeymap,
    layouts: &[String],
    source: Language,
) -> Result<LayoutMap, String> {
    let (english, persian) = crate::x11::english_persian_groups(layouts)?;
    let (from_group, to_group) = match source {
        Language::English => (english, persian),
        Language::Persian => (persian, english),
    };
    keymap.layout_map(from_group, to_group)
}

/// Layout tables for a Wayland session. The compositor does not expose its
/// keymap to ordinary clients, so the standard `us` and `ir` layouts are
/// compiled locally; the conversion only needs the key positions.
fn wayland_layout_maps() -> Result<(LayoutMap, LayoutMap), String> {
    let layouts = vec!["us".to_owned(), "ir".to_owned()];
    let config = XkbConfig {
        rules: String::new(),
        model: String::new(),
        layouts: layouts.clone(),
        variants: vec![String::new(), String::new()],
        options: String::new(),
    };
    let keymap = config.compile()?;
    Ok((
        build_layout_map_from(&keymap, &layouts, Language::English)?,
        build_layout_map_from(&keymap, &layouts, Language::Persian)?,
    ))
}

/// True when the current desktop session is Wayland.
pub fn is_wayland_session() -> bool {
    match env::var("XDG_SESSION_TYPE").ok().as_deref() {
        Some("wayland") => true,
        Some("x11") => false,
        _ => env::var_os("WAYLAND_DISPLAY").is_some(),
    }
}

fn layout_maps() -> Result<(LayoutMap, LayoutMap), String> {
    if is_wayland_session() {
        return wayland_layout_maps();
    }
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

const SUGGESTION_NOTIFICATION_BODY: &str =
    "Possible English/Persian layout mismatch. Select the text and use your TypeFix shortcut to convert it.";

fn suggestion_notification_body(_suggestion: &Suggestion) -> &'static str {
    // Never pass the typed word or converted text to the desktop notification
    // service; notification history is outside TypeFix's control.
    SUGGESTION_NOTIFICATION_BODY
}

fn notify_suggestion(suggestion: &Suggestion) {
    notify("TypeFix", suggestion_notification_body(suggestion));
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

/// State kept between copying the converted text and restoring the clipboard.
pub(crate) struct PendingPaste {
    previous: Option<String>,
}

/// Reads the selection (PRIMARY), converts it in memory and puts the result on
/// the CLIPBOARD. Returns `None` (after notifying) when there is nothing to do.
///
/// Only the CLIPBOARD is written. Taking over PRIMARY would make the
/// application drop its own selection, and the paste would then insert the
/// converted text next to the original instead of replacing it.
pub(crate) fn prepare_conversion(
    clipboard: &mut Clipboard,
    en_to_fa: &LayoutMap,
    fa_to_en: &LayoutMap,
) -> Result<Option<PendingPaste>, String> {
    // The selection can briefly be unavailable right after the keys are
    // released, so retry a few times before giving up.
    let mut selected = String::new();
    for attempt in 0..4 {
        selected = clipboard
            .get()
            .clipboard(LinuxClipboardKind::Primary)
            .text()
            .unwrap_or_default();
        if !selected.is_empty() {
            break;
        }
        if attempt < 3 {
            thread::sleep(Duration::from_millis(80));
        }
    }
    if selected.is_empty() {
        notify("TypeFix", "No text is selected.");
        return Ok(None);
    }
    let Some(converted) = convert_selection(&selected, en_to_fa, fa_to_en) else {
        notify("TypeFix", "Nothing to convert in the selected text.");
        return Ok(None);
    };

    let previous = clipboard
        .get()
        .clipboard(LinuxClipboardKind::Clipboard)
        .text()
        .ok();
    clipboard
        .set()
        .clipboard(LinuxClipboardKind::Clipboard)
        .text(converted)
        .map_err(|e| format!("could not set the clipboard: {e}"))?;
    thread::sleep(Duration::from_millis(150));
    Ok(Some(PendingPaste { previous }))
}

/// Waits for the application to read the pasted text, then restores the
/// previous clipboard contents.
pub(crate) fn finish_conversion(clipboard: &mut Clipboard, pending: PendingPaste) {
    thread::sleep(Duration::from_millis(900));
    if let Some(previous) = pending.previous {
        let _ = clipboard
            .set()
            .clipboard(LinuxClipboardKind::Clipboard)
            .text(previous);
    }
}

/// Handles one press of the conversion shortcut on X11: the converted text is
/// pasted over the selection with Shift+Insert.
fn convert_current_selection(
    listener: &HotkeyListener,
    clipboard: &mut Clipboard,
    en_to_fa: &LayoutMap,
    fa_to_en: &LayoutMap,
) -> Result<(), String> {
    listener.wait_for_modifier_release();
    let Some(pending) = prepare_conversion(clipboard, en_to_fa, fa_to_en)? else {
        return Ok(());
    };
    listener.send_paste()?;
    finish_conversion(clipboard, pending);
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

        let wayland = is_wayland_session();
        let handle = thread::spawn(move || {
            if wayland {
                #[cfg(feature = "wayland")]
                crate::wayland::run_shortcut_worker(shortcut, thread_stop, ready_tx, en_to_fa, fa_to_en);
                #[cfg(not(feature = "wayland"))]
                {
                    let _ = (shortcut, thread_stop, en_to_fa, fa_to_en);
                    let _ = ready_tx.send(Err(
                        "this build has no Wayland support (rebuild with --features wayland)".to_owned(),
                    ));
                }
                return;
            }
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

        if is_wayland_session() {
            return Err("typing suggestions are not available on Wayland; only the conversion shortcut is".to_owned());
        }
        let dir = find_data_dir(data_dir)
            .ok_or("cannot find en_50k.txt and fa_50k.txt; set TYPEFIX_DATA_DIR")?;
        let dicts = Dictionaries::load(dir.join("en_50k.txt"), dir.join("fa_50k.txt"))
            .map_err(|e| format!("cannot load dictionaries from {}: {e}", dir.display()))?;
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let thread_stop = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            let setup = || -> Result<(X11SessionConnection, ComposedTypingSession), String> {
                let connection = X11SessionConnection::connect()?;
                // Validate the map on the same connection the worker will use.
                connection.layout_map(source)?;
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


#[cfg(test)]
mod privacy_tests {
    use super::*;

    #[test]
    fn suggestion_notification_never_contains_typed_or_converted_text() {
        let suggestion = Suggestion {
            original: "sghl".to_owned(),
            converted: "سلام".to_owned(),
            source: Language::English,
        };
        let body = suggestion_notification_body(&suggestion);
        assert_eq!(body, SUGGESTION_NOTIFICATION_BODY);
        assert!(!body.contains(&suggestion.original));
        assert!(!body.contains(&suggestion.converted));
    }
}
