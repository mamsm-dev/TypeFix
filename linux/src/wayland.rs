//! Wayland backend (tested target: KDE Plasma). Wayland has no global key
//! grabs and no synthetic input for ordinary clients, so this uses the XDG
//! desktop portals instead:
//!
//! * `GlobalShortcuts` registers the conversion shortcut. The desktop shows
//!   the binding in its own settings and the user can change it there.
//! * `RemoteDesktop` (keyboard only) injects Shift+Insert to paste the result.
//!   The desktop asks for permission once; the grant is remembered with a
//!   restore token stored next to the config file.
//!
//! The selection is read from PRIMARY and the result is placed on the
//! CLIPBOARD through the data-control protocol (see `arboard`). Text stays in
//! memory only.

use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc,
    },
    thread,
    time::Duration,
};

use arboard::Clipboard;
use ashpd::desktop::{
    global_shortcuts::{GlobalShortcuts, NewShortcut},
    remote_desktop::{DeviceType, KeyState, RemoteDesktop},
    PersistMode,
};
use futures_lite::{future, Stream, StreamExt};

use crate::{
    config::config_path,
    service::{finish_conversion, notify, prepare_conversion},
    shortcut::Shortcut,
    LayoutMap,
};

const SHORTCUT_ID: &str = "convert-selection";
/// Linux evdev key codes (the RemoteDesktop portal takes evdev codes).
const KEY_LEFTSHIFT: i32 = 42;
const KEY_INSERT: i32 = 110;

fn token_path() -> Option<PathBuf> {
    config_path().map(|path| path.with_file_name("remote-desktop-token"))
}

fn read_token() -> Option<String> {
    let text = fs::read_to_string(token_path()?).ok()?;
    let text = text.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

fn save_token(token: &str) {
    if let Some(path) = token_path() {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, token);
    }
}

/// Waits for the next item; `None` is a timeout, `Some(None)` a closed stream.
async fn next_within<S: Stream + Unpin>(stream: &mut S, wait: Duration) -> Option<Option<S::Item>> {
    future::or(async { Some(stream.next().await) }, async {
        async_io::Timer::after(wait).await;
        None
    })
    .await
}

fn portal_error(what: &str, error: impl std::fmt::Display) -> String {
    format!("{what}: {error}")
}

async fn worker(
    shortcut: &Shortcut,
    stop: &AtomicBool,
    ready: &mut Option<Sender<Result<(), String>>>,
    en_to_fa: &LayoutMap,
    fa_to_en: &LayoutMap,
) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| {
        format!("cannot access the Wayland clipboard (the compositor must support the data-control protocol): {e}")
    })?;

    let remote = RemoteDesktop::new()
        .await
        .map_err(|e| portal_error("the RemoteDesktop portal is not available", e))?;
    let remote_session = remote
        .create_session()
        .await
        .map_err(|e| portal_error("cannot create a RemoteDesktop session", e))?;
    let token = read_token();
    remote
        .select_devices(&remote_session, DeviceType::Keyboard.into(), token.as_deref(), PersistMode::ExplicitlyRevoked)
        .await
        .map_err(|e| portal_error("cannot select the keyboard device", e))?
        .response()
        .map_err(|e| portal_error("keyboard access was not granted", e))?;
    let started = remote
        .start(&remote_session, None)
        .await
        .map_err(|e| portal_error("cannot start the RemoteDesktop session", e))?
        .response()
        .map_err(|e| portal_error("keyboard access was denied (needed to paste the converted text)", e))?;
    eprintln!("typefix: RemoteDesktop session started, devices: {:?}", started.devices());
    if let Some(token) = started.restore_token() {
        save_token(token);
    }

    let shortcuts = GlobalShortcuts::new()
        .await
        .map_err(|e| portal_error("the GlobalShortcuts portal is not available", e))?;
    let shortcut_session = shortcuts
        .create_session()
        .await
        .map_err(|e| portal_error("cannot create a GlobalShortcuts session", e))?;
    let trigger = shortcut.portal_trigger();
    let new_shortcut = NewShortcut::new(SHORTCUT_ID, "Convert the selected text between English and Persian layouts")
        .preferred_trigger(Some(trigger.as_str()));
    let bound = shortcuts
        .bind_shortcuts(&shortcut_session, &[new_shortcut], None)
        .await
        .map_err(|e| portal_error("cannot register the shortcut", e))?
        .response()
        .map_err(|e| portal_error("the shortcut was not accepted", e))?;
    eprintln!("typefix: portal accepted the shortcut (requested {trigger}): {bound:?}");

    let mut activated = Box::pin(
        shortcuts
            .receive_activated()
            .await
            .map_err(|e| portal_error("cannot listen for the shortcut", e))?,
    );
    let mut deactivated = Box::pin(
        shortcuts
            .receive_deactivated()
            .await
            .map_err(|e| portal_error("cannot listen for the shortcut", e))?,
    );

    if let Some(tx) = ready.take() {
        let _ = tx.send(Ok(()));
    }

    let mut outcome = Ok(());
    while !stop.load(Ordering::Relaxed) {
        let event = match next_within(&mut activated, Duration::from_millis(200)).await {
            None => continue,
            Some(None) => {
                outcome = Err("the GlobalShortcuts portal closed the session".to_owned());
                break;
            }
            Some(Some(event)) => event,
        };
        eprintln!("typefix: shortcut activated ({})", event.shortcut_id());
        if event.shortcut_id() != SHORTCUT_ID {
            continue;
        }
        // Let the user release the keys first, otherwise the injected
        // Shift+Insert would combine with the still-held shortcut modifiers.
        let _ = next_within(&mut deactivated, Duration::from_secs(2)).await;
        thread::sleep(Duration::from_millis(150));

        let result = match prepare_conversion(&mut clipboard, en_to_fa, fa_to_en) {
            Ok(Some(pending)) => {
                eprintln!("typefix: converted text placed on the clipboard; sending Shift+Insert");
                let mut pasted = Ok(());
                for (code, state) in [
                    (KEY_LEFTSHIFT, KeyState::Pressed),
                    (KEY_INSERT, KeyState::Pressed),
                    (KEY_INSERT, KeyState::Released),
                    (KEY_LEFTSHIFT, KeyState::Released),
                ] {
                    if let Err(error) = remote.notify_keyboard_keycode(&remote_session, code, state).await {
                        pasted = Err(portal_error("cannot send the paste keys", error));
                        break;
                    }
                }
                finish_conversion(&mut clipboard, pending);
                pasted
            }
            Ok(None) => {
                eprintln!("typefix: nothing selected or nothing to convert (PRIMARY selection empty?)");
                Ok(())
            }
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            notify("TypeFix", &format!("Conversion failed: {error}"));
        }
    }

    let _ = shortcut_session.close().await;
    let _ = remote_session.close().await;
    outcome
}

/// Runs the Wayland conversion-shortcut worker until `stop` is set. Reports
/// start-up success or failure through `ready` exactly once.
pub fn run_shortcut_worker(
    shortcut: Shortcut,
    stop: Arc<AtomicBool>,
    ready: Sender<Result<(), String>>,
    en_to_fa: LayoutMap,
    fa_to_en: LayoutMap,
) {
    let mut ready = Some(ready);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        async_io::block_on(worker(&shortcut, &stop, &mut ready, &en_to_fa, &fa_to_en))
    }))
    .unwrap_or_else(|_| Err("internal error in the Wayland portal worker (see the terminal output)".to_owned()));
    if let Err(error) = result {
        match ready.take() {
            Some(tx) => {
                let _ = tx.send(Err(error));
            }
            None => notify("TypeFix", &format!("Shortcut stopped: {error}")),
        }
    }
}
