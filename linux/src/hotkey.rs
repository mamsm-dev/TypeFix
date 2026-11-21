//! Global shortcut for converting selected text (X11).
//!
//! Only the single configured key combination is grabbed (a passive key grab
//! on the root window); no other keystrokes are observed by this module. When
//! the shortcut fires, the caller reads the selection and the module sends one
//! synthetic Shift+Insert through XTest to paste the converted text. No uinput
//! or raw device access is used.

use std::{thread, time::{Duration, Instant}};

use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{ConnectionExt as XprotoConnectionExt, GrabMode, KEY_PRESS_EVENT, KEY_RELEASE_EVENT},
        xtest::ConnectionExt as XTestConnectionExt,
        Event,
    },
    rust_connection::RustConnection,
    NONE,
};
use xkbcommon::xkb;

use crate::shortcut::{Shortcut, ALT_MASK, CONTROL_MASK, SHIFT_MASK, SUPER_MASK};

const LOCK_MASK: u16 = 1 << 1;
const NUM_LOCK_MASK: u16 = 1 << 4;
const KEYSYM_SHIFT_L: u32 = 0xffe1;
const KEYSYM_INSERT: u32 = 0xff63;

pub struct HotkeyListener {
    connection: RustConnection,
    root: u32,
    keycode: u8,
}

fn resolve_keysym(name: &str) -> Result<u32, String> {
    let exact = u32::from(xkb::keysym_from_name(name, xkb::KEYSYM_NO_FLAGS));
    if exact != 0 {
        return Ok(exact);
    }
    let loose = u32::from(xkb::keysym_from_name(name, xkb::KEYSYM_CASE_INSENSITIVE));
    if loose != 0 {
        return Ok(loose);
    }
    Err(format!("unknown key name {name:?} (examples: Space, F9, c, Return)"))
}

fn find_keycode(connection: &RustConnection, keysym: u32) -> Result<Option<u8>, String> {
    let setup = connection.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let reply = connection
        .get_keyboard_mapping(min, max - min + 1)
        .map_err(|e| format!("could not request the keyboard mapping: {e}"))?
        .reply()
        .map_err(|e| format!("could not read the keyboard mapping: {e}"))?;
    let per_keycode = usize::from(reply.keysyms_per_keycode);
    if per_keycode == 0 {
        return Ok(None);
    }
    for (index, symbols) in reply.keysyms.chunks(per_keycode).enumerate() {
        if symbols.contains(&keysym) {
            return Ok(u8::try_from(usize::from(min) + index).ok());
        }
    }
    Ok(None)
}

impl HotkeyListener {
    /// Grabs the shortcut on the root window. Fails if another application
    /// already owns the same combination.
    pub fn new(shortcut: &Shortcut) -> Result<Self, String> {
        let (connection, screen) =
            x11rb::connect(None).map_err(|e| format!("cannot connect to X11 display: {e}"))?;
        let root = connection.setup().roots[screen].root;
        let keysym = resolve_keysym(&shortcut.key)?;
        let keycode = find_keycode(&connection, keysym)?
            .ok_or_else(|| format!("key {:?} does not exist on this keyboard", shortcut.key))?;

        // Grab with and without Caps Lock / Num Lock so the shortcut still works.
        let base = shortcut.modifier_mask();
        for extra in [0, LOCK_MASK, NUM_LOCK_MASK, LOCK_MASK | NUM_LOCK_MASK] {
            connection
                .grab_key(true, root, base | extra, keycode, GrabMode::ASYNC, GrabMode::ASYNC)
                .map_err(|e| format!("could not grab {shortcut}: {e}"))?
                .check()
                .map_err(|e| {
                    format!("could not grab {shortcut}: {e} (is it already used by another application?)")
                })?;
        }
        connection.flush().map_err(|e| format!("could not flush X11 requests: {e}"))?;
        Ok(Self { connection, root, keycode })
    }

    /// Blocks and calls `on_trigger` every time the shortcut is pressed.
    pub fn run<F: FnMut()>(&self, mut on_trigger: F) -> Result<(), String> {
        loop {
            let event = self
                .connection
                .wait_for_event()
                .map_err(|e| format!("X11 connection lost: {e}"))?;
            if let Event::KeyPress(press) = event {
                if press.detail == self.keycode {
                    on_trigger();
                }
            }
        }
    }

    /// Waits (up to ~1.5 s) until Ctrl/Alt/Shift/Super are physically released,
    /// so the synthetic paste is not combined with the shortcut's modifiers.
    pub fn wait_for_modifier_release(&self) {
        let watched = SHIFT_MASK | CONTROL_MASK | ALT_MASK | SUPER_MASK;
        let deadline = Instant::now() + Duration::from_millis(1500);
        while Instant::now() < deadline {
            let held = self
                .connection
                .query_pointer(self.root)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .map(|reply| u16::from(reply.mask) & watched);
            if held.unwrap_or(0) == 0 {
                return;
            }
            thread::sleep(Duration::from_millis(15));
        }
    }

    /// Sends Shift+Insert, which pastes in GTK, Qt, browsers and terminals and
    /// does not depend on the active keyboard layout group.
    pub fn send_paste(&self) -> Result<(), String> {
        let shift = find_keycode(&self.connection, KEYSYM_SHIFT_L)?
            .ok_or("no Shift key found on this keyboard")?;
        let insert = find_keycode(&self.connection, KEYSYM_INSERT)?
            .ok_or("no Insert key found on this keyboard")?;
        for (event_type, keycode) in [
            (KEY_PRESS_EVENT, shift),
            (KEY_PRESS_EVENT, insert),
            (KEY_RELEASE_EVENT, insert),
            (KEY_RELEASE_EVENT, shift),
        ] {
            self.connection
                .xtest_fake_input(event_type, keycode, 0, NONE, 0, 0, 0)
                .map_err(|e| format!("XTest paste failed: {e}"))?;
        }
        self.connection.flush().map_err(|e| format!("could not flush X11 requests: {e}"))
    }
}
