//! X11 session preflight, XKB keymap translation, and opt-in raw key observation.
//!
//! Event observation is separate from text translation: XI2 raw key events
//! contain keycodes, not the active modifier/group snapshot. The explicit
//! translator can decode one device event in memory; observation itself never
//! logs, persists, or injects text.

use std::process::Command;

use crate::compose::{ComposeOutcome, ComposeProcessor};
use x11rb::{
    connection::Connection,
    protocol::{
        xinput::{
            ConnectionExt as XInputConnectionExt, EventMask, XIEventMask,
        },
        xkb as x11_xkb,
        xkb::ConnectionExt as XkbConnectionExt,
        Event,
    },
    rust_connection::RustConnection,
};
use xkbcommon::xkb;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XkbConfig {
    pub rules: String,
    pub model: String,
    pub layouts: Vec<String>,
    pub variants: Vec<String>,
    pub options: String,
}

impl XkbConfig {
    pub fn from_query_output(output: &str) -> Result<Self, String> {
        let mut fields = std::collections::HashMap::new();
        for line in output.lines() {
            if let Some((key, value)) = line.split_once(':') {
                fields.insert(key.trim().to_owned(), value.trim().to_owned());
            }
        }

        let layouts: Vec<String> = fields
            .get("layout")
            .map(|s| s.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect())
            .unwrap_or_default();
        if layouts.is_empty() {
            return Err("XKB query did not report an active layout".into());
        }

        let variants = fields
            .get("variant")
            .map(|s| s.split(',').map(str::trim).map(str::to_owned).collect())
            .unwrap_or_default();

        Ok(Self {
            rules: fields.get("rules").cloned().unwrap_or_default(),
            model: fields.get("model").cloned().unwrap_or_default(),
            layouts,
            variants,
            options: fields.get("options").cloned().unwrap_or_default(),
        })
    }

    pub fn from_current_session() -> Result<Self, String> {
        let output = Command::new("setxkbmap")
            .arg("-query")
            .output()
            .map_err(|e| format!("could not run setxkbmap -query: {e}"))?;
        if !output.status.success() {
            return Err("setxkbmap -query failed".into());
        }
        let text = String::from_utf8(output.stdout)
            .map_err(|_| "setxkbmap returned non-UTF-8 output".to_owned())?;
        Self::from_query_output(&text)
    }

    pub fn compile(&self) -> Result<XkbKeymap, String> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let layout = self.layouts.join(",");
        let variant = self.variants.join(",");
        let keymap = xkb::Keymap::new_from_names(
            &context,
            &self.rules,
            &self.model,
            &layout,
            &variant,
            Some(self.options.clone()),
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .ok_or_else(|| "libxkbcommon could not compile the active XKB layout".to_owned())?;
        Ok(XkbKeymap { keymap })
    }
}

pub struct XkbKeymap {
    keymap: xkb::Keymap,
}

impl XkbKeymap {
    /// Translate one XI2 device key event through XKB and Compose.
    ///
    /// The caller must invoke this only for key presses, pass the server's
    /// effective modifier/group snapshot, and reset Compose on focus changes,
    /// navigation, cancellation, disable, and keymap/locale replacement.
    /// This helper does not subscribe to events or retain text between calls.
    pub fn translate_event(
        &self,
        event: &X11KeyEvent,
        compose: &mut ComposeProcessor,
    ) -> X11TextOutcome {
        if !event.pressed {
            return X11TextOutcome::NoText;
        }

        let mut state = xkb::State::new(&self.keymap);
        state.update_mask(event.effective_modifiers, 0, 0, u32::from(event.effective_group), 0, 0);
        let keycode = xkb::Keycode::new(u32::from(event.keycode));
        let keysym = state.key_get_one_sym(keycode);

        match compose.feed(keysym) {
            ComposeOutcome::Composing => X11TextOutcome::Composing,
            ComposeOutcome::Committed(text) => X11TextOutcome::Text(text),
            ComposeOutcome::Cancelled => X11TextOutcome::Cancelled,
            ComposeOutcome::Passthrough => {
                let text = state.key_get_utf8(keycode);
                if text.is_empty() || text.chars().any(char::is_control) {
                    X11TextOutcome::NoText
                } else {
                    X11TextOutcome::Text(text)
                }
            }
        }
    }

    /// Convert a translated key press into a backend-neutral typing-session event.
    ///
    /// This classifies editing/navigation keys before Compose and routes
    /// Control/Alt/Super-modified input to the shortcut reset path. Modifier
    /// names are resolved through the active XKB keymap, so custom Alt/Super
    /// mappings are respected. AltGr/Mod5 is intentionally not treated as a
    /// shortcut so it remains available for printable symbols.
    pub fn session_event_for_key(
        &self,
        event: &X11KeyEvent,
        compose: &mut ComposeProcessor,
    ) -> Option<crate::session::SessionEvent> {
        if !event.pressed {
            return None;
        }

        let mut state = xkb::State::new(&self.keymap);
        state.update_mask(event.effective_modifiers, 0, 0, u32::from(event.effective_group), 0, 0);
        // Resolve semantic modifier names through this keymap rather than
        // assuming Alt/Super always occupy Mod1/Mod4 bit positions.
        const SHORTCUT_MODIFIERS: [&str; 5] = ["Control", "Alt", "Super", "Meta", "Hyper"];
        if SHORTCUT_MODIFIERS
            .iter()
            .any(|name| state.mod_name_is_active(name, xkb::STATE_MODS_EFFECTIVE))
        {
            return Some(crate::session::SessionEvent::Shortcut);
        }

        let keycode = xkb::Keycode::new(u32::from(event.keycode));
        let keysym = u32::from(state.key_get_one_sym(keycode));

        match keysym {
            0xff08 => Some(crate::session::SessionEvent::Backspace),
            0xff09 if event.effective_modifiers & 1 != 0 => Some(crate::session::SessionEvent::Navigation),
            0xff09 => Some(crate::session::SessionEvent::Tab),
            0x0020 => Some(crate::session::SessionEvent::Space),
            0xfe20 | 0xff0d | 0xff1b | 0xffff | 0xff50..=0xff57 | 0xff63 | 0xffbe..=0xffe0 => {
                Some(crate::session::SessionEvent::Navigation)
            }
            _ => match self.translate_event(event, compose) {
                X11TextOutcome::Text(text) => Some(crate::session::SessionEvent::Text(text)),
                X11TextOutcome::Cancelled => Some(crate::session::SessionEvent::Reset),
                X11TextOutcome::NoText | X11TextOutcome::Composing => None,
            },
        }
    }

    /// Translate an X11 keycode using the requested XKB layout group and
    /// modifier mask. X11 keycodes are already in the XKB keycode namespace;
    /// do not add the evdev +8 offset a second time.
    ///
    /// The caller must supply a coherent modifier/group snapshot from XKB.
    /// XI2 raw events do not contain that snapshot, so do not derive it from
    /// the raw event's keycode or assume that the active group is unchanged.
    pub fn translate_keycode(
        &self,
        x11_keycode: u8,
        group: u32,
        modifiers: u32,
    ) -> Option<String> {
        let mut state = xkb::State::new(&self.keymap);
        state.update_mask(modifiers, 0, 0, group, 0, 0);
        let keycode = xkb::Keycode::new(u32::from(x11_keycode));
        let text = state.key_get_utf8(keycode);
        (!text.is_empty() && !text.chars().any(char::is_control)).then_some(text)
    }

    pub fn layout_count(&self) -> u32 {
        self.keymap.num_layouts()
    }
}

/// Result of translating one XI2 device key event. Text exists only in this
/// return value; callers must keep it in memory and must never log or persist it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X11TextOutcome {
    NoText,
    Composing,
    Text(String),
    Cancelled,
}

/// A key transition observed from XI2. It intentionally contains no decoded text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X11RawKeyEvent {
    pub keycode: u8,
    pub device_id: u16,
    pub timestamp: u32,
    pub pressed: bool,
}

/// An XI2 device key event with the modifier and group snapshot supplied by the server.
/// Unlike XI2 raw events, device events include the effective XKB state at event time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X11KeyEvent {
    pub keycode: u8,
    pub device_id: u16,
    pub source_id: u16,
    pub timestamp: u32,
    pub pressed: bool,
    pub effective_modifiers: u32,
    pub effective_group: u8,
}

/// An XI2 focus transition observed on the root window.
/// `event_window` is the window on which the event was selected; it is not
/// necessarily the focused client window. `child_window` is the reported
/// child (zero means no child was reported).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X11FocusEvent {
    pub device_id: u16,
    pub source_id: u16,
    pub timestamp: u32,
    pub focused: bool,
    pub event_window: u32,
    pub child_window: u32,
}

/// A single event from the opt-in X11 session event stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum X11SessionEvent {
    Key(X11KeyEvent),
    Focus(X11FocusEvent),
    KeymapChanged(X11KeymapChangeEvent),
}

/// An XKB map or core keyboard replacement notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X11KeymapChangeEvent {
    pub device_id: u16,
    pub timestamp: u32,
    pub changed_components: u16,
    pub new_keyboard: bool,
}

impl X11KeymapChangeEvent {
    /// A keymap change invalidates both translated key state and pending Compose.
    pub fn as_session_event(&self) -> crate::session::SessionEvent {
        crate::session::SessionEvent::Reset
    }
}

impl X11FocusEvent {
    /// Convert either direction of an XI2 focus transition into the
    /// backend-neutral lifecycle event that clears typing and Compose state.
    pub fn as_session_event(&self) -> crate::session::SessionEvent {
        crate::session::SessionEvent::FocusChanged
    }
}

pub struct X11SessionConnection {
    connection: RustConnection,
    pub screen_number: usize,
    pub xi2_version: (u16, u16),
    pub config: XkbConfig,
    pub keymap: XkbKeymap,
}

impl X11SessionConnection {
    /// Connects as the current desktop user. No root privileges or device nodes
    /// are accessed. Connecting alone does not subscribe to keyboard events.
    pub fn connect() -> Result<Self, String> {
        let (connection, screen_number) =
            x11rb::connect(None).map_err(|e| format!("cannot connect to X11 display: {e}"))?;
        let version = connection
            .xinput_xi_query_version(2, 0)
            .map_err(|e| format!("XInput2 is unavailable: {e}"))?
            .reply()
            .map_err(|e| format!("XInput2 version negotiation failed: {e}"))?;
        if version.major_version < 2 {
            return Err("XInput2 version 2.0 or newer is required".into());
        }

        let config = XkbConfig::from_current_session()?;
        let keymap = config.compile()?;
        Ok(Self {
            connection,
            screen_number,
            xi2_version: (version.major_version, version.minor_version),
            config,
            keymap,
        })
    }

    /// Explicitly enables global raw keycode observation for all master devices.
    ///
    /// This is opt-in and must not be called by the probe or silently at startup.
    /// X11 raw events are server-wide and do not provide strong per-application
    /// isolation. This method does not grab, consume, or alter application input.
    pub fn subscribe_raw_key_events(&self) -> Result<(), String> {
        let root = self.connection.setup().roots[self.screen_number].root;
        let mask = XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_KEY_RELEASE;
        self.connection
            .xinput_xi_select_events(
                root,
                &[EventMask {
                    // XIAllMasterDevices is device ID 1 in the XI2 protocol.
                    deviceid: 1,
                    mask: vec![mask],
                }],
            )
            .map_err(|e| format!("could not select XI2 raw key events: {e}"))?
            .check()
            .map_err(|e| format!("X server rejected XI2 raw key event selection: {e}"))?;
        self.connection
            .flush()
            .map_err(|e| format!("could not flush XI2 event selection: {e}"))
    }

    /// Explicitly subscribes to XI2 device key events on the root window.
    ///
    /// These focus-propagated device events include effective modifier and group
    /// snapshots at event time. They are still visible across applications on
    /// the same X server; this method is opt-in and does not grab or consume input.
    pub fn subscribe_key_events(&self) -> Result<(), String> {
        let root = self.connection.setup().roots[self.screen_number].root;
        let mask = XIEventMask::KEY_PRESS | XIEventMask::KEY_RELEASE;
        self.connection
            .xinput_xi_select_events(
                root,
                &[EventMask {
                    deviceid: 1, // XIAllMasterDevices
                    mask: vec![mask],
                }],
            )
            .map_err(|e| format!("could not select XI2 device key events: {e}"))?
            .check()
            .map_err(|e| format!("X server rejected XI2 device key event selection: {e}"))?;
        self.connection
            .flush()
            .map_err(|e| format!("could not flush XI2 device event selection: {e}"))
    }

    /// Explicitly subscribes to XI2 focus transitions propagated through the root window.
    ///
    /// Focus events are metadata only and must be forwarded to the session as a
    /// reset signal. They do not identify password fields or provide strong
    /// application isolation. This subscription is opt-in.
    pub fn subscribe_focus_events(&self) -> Result<(), String> {
        let root = self.connection.setup().roots[self.screen_number].root;
        let mask = XIEventMask::FOCUS_IN | XIEventMask::FOCUS_OUT;
        self.connection
            .xinput_xi_select_events(
                root,
                &[EventMask {
                    deviceid: 1, // XIAllMasterDevices
                    mask: vec![mask],
                }],
            )
            .map_err(|e| format!("could not select XI2 focus events: {e}"))?
            .check()
            .map_err(|e| format!("X server rejected XI2 focus event selection: {e}"))?;
        self.connection
            .flush()
            .map_err(|e| format!("could not flush XI2 focus event selection: {e}"))
    }

    /// Selects key press/release and focus-in/out events in one XI2 request.
    ///
    /// Use this when consuming `poll_for_session_event()` or
    /// `wait_for_session_event()`. XI2 event selections are per device/window;
    /// issuing separate key-only and focus-only selections can replace the
    /// previous mask, so this combined method is the safe option for a session.
    pub fn subscribe_session_events(&self) -> Result<(), String> {
        let root = self.connection.setup().roots[self.screen_number].root;
        let mask = XIEventMask::KEY_PRESS
            | XIEventMask::KEY_RELEASE
            | XIEventMask::FOCUS_IN
            | XIEventMask::FOCUS_OUT;
        self.connection
            .xinput_xi_select_events(
                root,
                &[EventMask {
                    deviceid: 1, // XIAllMasterDevices
                    mask: vec![mask],
                }],
            )
            .map_err(|e| format!("could not select XI2 session events: {e}"))?
            .check()
            .map_err(|e| format!("X server rejected XI2 session event selection: {e}"))?;
        self.connection
            .flush()
            .map_err(|e| format!("could not flush XI2 session event selection: {e}"))
    }

    /// Selects XKB map and core-keyboard replacement notifications.
    ///
    /// The caller should refresh the compiled XKB keymap after receiving
    /// `X11SessionEvent::KeymapChanged`. This subscription is explicit.
    pub fn subscribe_xkb_map_events(&self) -> Result<(), String> {
        let version = self.connection
            .xkb_use_extension(1, 0)
            .map_err(|e| format!("XKB extension request failed: {e}"))?
            .reply()
            .map_err(|e| format!("XKB extension negotiation failed: {e}"))?;
        if !version.supported {
            return Err("XKB extension 1.0 or newer is required for map notifications".into());
        }

        let map_parts = x11_xkb::MapPart::KEY_TYPES
            | x11_xkb::MapPart::KEY_SYMS
            | x11_xkb::MapPart::MODIFIER_MAP
            | x11_xkb::MapPart::EXPLICIT_COMPONENTS
            | x11_xkb::MapPart::KEY_ACTIONS
            | x11_xkb::MapPart::KEY_BEHAVIORS
            | x11_xkb::MapPart::VIRTUAL_MODS
            | x11_xkb::MapPart::VIRTUAL_MOD_MAP;
        let details = x11_xkb::SelectEventsAux {
            new_keyboard_notify: None,
            state_notify: None,
            controls_notify: None,
            indicator_state_notify: None,
            indicator_map_notify: None,
            names_notify: None,
            compat_map_notify: None,
            bell_notify: None,
            action_message: None,
            access_x_notify: None,
            extension_device_notify: None,
        };
        let events = x11_xkb::EventType::MAP_NOTIFY
            | x11_xkb::EventType::NEW_KEYBOARD_NOTIFY;
        self.connection
            .xkb_select_events(
                x11_xkb::ID::USE_CORE_KBD.into(),
                x11_xkb::EventType::from(0u16),
                events,
                map_parts,
                map_parts,
                &details,
            )
            .map_err(|e| format!("could not select XKB map notifications: {e}"))?
            .check()
            .map_err(|e| format!("X server rejected XKB map notification selection: {e}"))?;
        self.connection
            .flush()
            .map_err(|e| format!("could not flush XKB map notification selection: {e}"))
    }

    /// Rebuilds the XKB translator from the current session's RMLVO settings.
    ///
    /// Call after receiving `X11SessionEvent::KeymapChanged`; pending Compose
    /// state should also be reset by handling the event's session reset mapping.
    pub fn refresh_keymap(&mut self) -> Result<(), String> {
        let config = XkbConfig::from_current_session()?;
        let keymap = config.compile()?;
        self.config = config;
        self.keymap = keymap;
        Ok(())
    }

    /// Dispatch one observed X11 session event into the composed typing state.
    ///
    /// Focus and keymap changes clear both typing and Compose state. Key presses
    /// are translated using the event-time modifier/group snapshot. Keymap
    /// notifications also rebuild the XKB translator before returning.
    ///
    /// This method processes one event only; the caller owns the event loop,
    /// subscription lifecycle, UI, and any eventual text replacement policy.
    pub fn handle_session_event(
        &mut self,
        session: &mut crate::session::ComposedTypingSession,
        event: X11SessionEvent,
        dicts: &crate::Dictionaries,
        source: crate::Language,
        map: &crate::LayoutMap,
    ) -> Result<crate::session::SessionAction, String> {
        match event {
            X11SessionEvent::Key(key) => {
                if let Some(session_event) = self.keymap.session_event_for_key(&key, session.compose_mut()) {
                    Ok(session.handle(session_event, dicts, source, map))
                } else {
                    Ok(crate::session::SessionAction::None)
                }
            }
            X11SessionEvent::Focus(focus) => {
                Ok(session.handle(focus.as_session_event(), dicts, source, map))
            }
            X11SessionEvent::KeymapChanged(change) => {
                let action = session.handle(change.as_session_event(), dicts, source, map);
                self.refresh_keymap()?;
                Ok(action)
            }
        }
    }

    /// Run an explicitly invoked foreground session loop.
    ///
    /// This enables the XI2 key/focus and XKB map subscriptions, dispatches
    /// events into the composed typing session, and reports non-empty actions
    /// to the caller. The caller supplies a stop predicate (for example, an
    /// application shutdown flag) and owns all UI/presentation decisions.
    /// No text is logged or injected by this loop.
    pub fn run_session_loop<OnAction, ShouldStop>(
        &mut self,
        session: &mut crate::session::ComposedTypingSession,
        dicts: &crate::Dictionaries,
        source: crate::Language,
        map: &crate::LayoutMap,
        mut on_action: OnAction,
        mut should_stop: ShouldStop,
    ) -> Result<(), String>
    where
        OnAction: FnMut(crate::session::SessionAction),
        ShouldStop: FnMut() -> bool,
    {
        self.subscribe_session_events()?;
        self.subscribe_xkb_map_events()?;

        while !should_stop() {
            if let Some(event) = self.poll_for_session_event()? {
                let action = self.handle_session_event(session, event, dicts, source, map)?;
                if !matches!(action, crate::session::SessionAction::None) {
                    on_action(action);
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        Ok(())
    }

    /// Polls one event from the opt-in X11 session stream without blocking.
    ///
    /// Returns `None` when no subscribed key/focus event is currently queued.
    pub fn poll_for_session_event(&self) -> Result<Option<X11SessionEvent>, String> {
        loop {
            let Some(event) = self.connection.poll_for_event()
                .map_err(|e| format!("X11 event poll failed: {e}"))? else {
                return Ok(None);
            };
            let decoded = match event {
                Event::XinputKeyPress(event) => {
                    let keycode = u8::try_from(event.detail)
                        .map_err(|_| "XI2 keycode is outside the XKB keycode range".to_owned())?;
                    Some(X11SessionEvent::Key(X11KeyEvent {
                        keycode,
                        device_id: event.deviceid,
                        source_id: event.sourceid,
                        timestamp: event.time,
                        pressed: true,
                        effective_modifiers: event.mods.effective,
                        effective_group: event.group.effective,
                    }))
                }
                Event::XinputKeyRelease(event) => {
                    let keycode = u8::try_from(event.detail)
                        .map_err(|_| "XI2 keycode is outside the XKB keycode range".to_owned())?;
                    Some(X11SessionEvent::Key(X11KeyEvent {
                        keycode,
                        device_id: event.deviceid,
                        source_id: event.sourceid,
                        timestamp: event.time,
                        pressed: false,
                        effective_modifiers: event.mods.effective,
                        effective_group: event.group.effective,
                    }))
                }
                Event::XinputFocusIn(event) => Some(X11SessionEvent::Focus(X11FocusEvent {
                    device_id: event.deviceid,
                    source_id: event.sourceid,
                    timestamp: event.time,
                    focused: true,
                    event_window: event.event,
                    child_window: event.child,
                })),
                Event::XinputFocusOut(event) => Some(X11SessionEvent::Focus(X11FocusEvent {
                    device_id: event.deviceid,
                    source_id: event.sourceid,
                    timestamp: event.time,
                    focused: false,
                    event_window: event.event,
                    child_window: event.child,
                })),
                Event::XkbMapNotify(event) => Some(X11SessionEvent::KeymapChanged(X11KeymapChangeEvent {
                    device_id: u16::from(event.device_id),
                    timestamp: event.time,
                    changed_components: u16::from(event.changed),
                    new_keyboard: false,
                })),
                Event::XkbNewKeyboardNotify(event) => Some(X11SessionEvent::KeymapChanged(X11KeymapChangeEvent {
                    device_id: u16::from(event.device_id),
                    timestamp: event.time,
                    changed_components: u16::from(event.changed),
                    new_keyboard: true,
                })),
                _ => None,
            };
            if decoded.is_some() {
                return Ok(decoded);
            }
        }
    }

    /// Waits for the next subscribed key or focus event.
    ///
    /// Use one event loop with this method after enabling the corresponding
    /// subscriptions; separate blocking wait methods on the same connection
    /// would consume and discard each other's event types.
    pub fn wait_for_session_event(&self) -> Result<X11SessionEvent, String> {
        loop {
            if let Some(event) = self.poll_for_session_event()? {
                return Ok(event);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// Waits for a focus-propagated XI2 key event and returns the server's
    /// effective modifier/group snapshot. No text is decoded or emitted.
    pub fn wait_for_key_event(&self) -> Result<X11KeyEvent, String> {
        loop {
            let event = self.connection.wait_for_event()
                .map_err(|e| format!("X11 event wait failed: {e}"))?;
            let (detail, device_id, source_id, timestamp, pressed, modifiers, group) = match event {
                Event::XinputKeyPress(event) =>
                    (event.detail, event.deviceid, event.sourceid, event.time, true,
                     event.mods.effective, event.group.effective),
                Event::XinputKeyRelease(event) =>
                    (event.detail, event.deviceid, event.sourceid, event.time, false,
                     event.mods.effective, event.group.effective),
                _ => continue,
            };
            let keycode = u8::try_from(detail)
                .map_err(|_| "XI2 keycode is outside the XKB keycode range".to_owned())?;
            return Ok(X11KeyEvent {
                keycode, device_id, source_id, timestamp, pressed,
                effective_modifiers: modifiers, effective_group: group,
            });
        }
    }

    /// Waits for the next raw key transition, ignoring unrelated X11 events.
    /// Call only after explicit opt-in via subscribe_raw_key_events().
    /// No key text is decoded or emitted.
    pub fn wait_for_raw_key_event(&self) -> Result<X11RawKeyEvent, String> {
        loop {
            let event = self.connection.wait_for_event()
                .map_err(|e| format!("X11 event wait failed: {e}"))?;
            let (keycode, device_id, timestamp, pressed) = match event {
                Event::XinputRawKeyPress(event) => (event.detail, event.deviceid, event.time, true),
                Event::XinputRawKeyRelease(event) => (event.detail, event.deviceid, event.time, false),
                _ => continue,
            };
            let keycode = u8::try_from(keycode)
                .map_err(|_| "XI2 raw keycode is outside the XKB keycode range".to_owned())?;
            return Ok(X11RawKeyEvent { keycode, device_id, timestamp, pressed });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_event_text_with_server_modifier_snapshot() {
        let config = XkbConfig {
            rules: "evdev".into(),
            model: "pc105".into(),
            layouts: vec!["us".into(), "ir".into()],
            variants: vec!["".into(), "".into()],
            options: String::new(),
        };
        let keymap = config.compile().expect("xkb-data and libxkbcommon installed");
        let mut compose = ComposeProcessor::from_buffer("", "en_US.UTF-8")
            .expect("empty deterministic Compose table compiles");
        let event = X11KeyEvent {
            keycode: 38,
            device_id: 2,
            source_id: 2,
            timestamp: 1,
            pressed: true,
            effective_modifiers: 1,
            effective_group: 0,
        };
        assert_eq!(keymap.translate_event(&event, &mut compose), X11TextOutcome::Text("A".into()));

        let release = X11KeyEvent { pressed: false, ..event };
        assert_eq!(keymap.translate_event(&release, &mut compose), X11TextOutcome::NoText);
    }

    #[test]
    fn repeated_key_presses_are_preserved_as_repeated_text() {
        let config = XkbConfig {
            rules: "evdev".into(),
            model: "pc105".into(),
            layouts: vec!["us".into()],
            variants: vec!["".into()],
            options: String::new(),
        };
        let keymap = config.compile().expect("US XKB keymap compiles");
        let mut compose = ComposeProcessor::from_buffer("", "en_US.UTF-8")
            .expect("empty deterministic Compose table compiles");
        let press = X11KeyEvent {
            keycode: 38,
            device_id: 2,
            source_id: 2,
            timestamp: 10,
            pressed: true,
            effective_modifiers: 0,
            effective_group: 0,
        };

        // XI2 reports each key-repeat as another press transition. The
        // translator must preserve it as text rather than deduplicate it.
        assert_eq!(
            keymap.session_event_for_key(&press, &mut compose),
            Some(crate::session::SessionEvent::Text("a".into()))
        );
        let repeated = X11KeyEvent { timestamp: 45, ..press };
        assert_eq!(
            keymap.session_event_for_key(&repeated, &mut compose),
            Some(crate::session::SessionEvent::Text("a".into()))
        );
        let release = X11KeyEvent { pressed: false, ..repeated };
        assert_eq!(keymap.session_event_for_key(&release, &mut compose), None);
    }

    #[test]
    fn classifies_special_and_shortcut_keys_for_typing_session() {
        let config = XkbConfig {
            rules: "evdev".into(),
            model: "pc105".into(),
            layouts: vec!["us".into(), "ir".into()],
            variants: vec!["".into(), "".into()],
            options: String::new(),
        };
        let keymap = config.compile().expect("US/Persian keymap compiles");
        let mut compose = ComposeProcessor::from_buffer("", "en_US.UTF-8")
            .expect("empty deterministic Compose table compiles");
        let event = |keycode, modifiers| X11KeyEvent {
            keycode,
            device_id: 2,
            source_id: 2,
            timestamp: 1,
            pressed: true,
            effective_modifiers: modifiers,
            effective_group: 0,
        };

        assert_eq!(
            keymap.session_event_for_key(&event(22, 0), &mut compose),
            Some(crate::session::SessionEvent::Backspace)
        );
        assert_eq!(
            keymap.session_event_for_key(&event(23, 0), &mut compose),
            Some(crate::session::SessionEvent::Tab)
        );
        assert_eq!(
            keymap.session_event_for_key(&event(23, 1), &mut compose),
            Some(crate::session::SessionEvent::Navigation)
        );
        assert_eq!(
            keymap.session_event_for_key(&event(65, 0), &mut compose),
            Some(crate::session::SessionEvent::Space)
        );
        assert_eq!(
            keymap.session_event_for_key(&event(113, 0), &mut compose),
            Some(crate::session::SessionEvent::Navigation)
        );
        assert_eq!(
            keymap.session_event_for_key(&event(38, 1), &mut compose),
            Some(crate::session::SessionEvent::Text("A".into()))
        );
        for modifier in [1 << 2, 1 << 3, 1 << 6] {
            assert_eq!(
                keymap.session_event_for_key(&event(38, modifier), &mut compose),
                Some(crate::session::SessionEvent::Shortcut)
            );
        }
        assert_ne!(
            keymap.session_event_for_key(&event(38, 1 << 7), &mut compose),
            Some(crate::session::SessionEvent::Shortcut),
            "AltGr/Mod5 must remain available for text input"
        );
        let release = X11KeyEvent { pressed: false, ..event(38, 0) };
        assert_eq!(keymap.session_event_for_key(&release, &mut compose), None);
    }

    #[test]
    fn translates_dead_key_sequence_before_plain_text() {
        let config = XkbConfig {
            rules: "evdev".into(),
            model: "pc105".into(),
            layouts: vec!["us".into()],
            variants: vec!["intl".into()],
            options: String::new(),
        };
        let keymap = config.compile().expect("us intl keymap compiles");
        let mut compose = ComposeProcessor::from_buffer(
            "<dead_acute> <e> : \"é\" eacute\n",
            "en_US.UTF-8",
        )
        .expect("deterministic Compose table compiles");

        // X11 keycode 48 is the apostrophe/dead-acute key in us(intl);
        // keycode 26 is the physical E key.
        let dead_acute = X11KeyEvent {
            keycode: 48, device_id: 2, source_id: 2, timestamp: 1,
            pressed: true, effective_modifiers: 0, effective_group: 0,
        };
        let e = X11KeyEvent { keycode: 26, timestamp: 2, ..dead_acute };
        assert_eq!(keymap.translate_event(&dead_acute, &mut compose), X11TextOutcome::Composing);
        assert_eq!(keymap.translate_event(&e, &mut compose), X11TextOutcome::Text("é".into()));
    }

    #[test]
    fn both_xi2_focus_directions_map_to_session_reset() {
        let focus_in = X11FocusEvent {
            device_id: 3,
            source_id: 5,
            timestamp: 10,
            focused: true,
            event_window: 1,
            child_window: 42,
        };
        let focus_out = X11FocusEvent {
            focused: false,
            timestamp: 11,
            ..focus_in
        };

        assert_eq!(focus_in.as_session_event(), crate::session::SessionEvent::FocusChanged);
        assert_eq!(focus_out.as_session_event(), crate::session::SessionEvent::FocusChanged);
    }

    #[test]
    fn parses_multiple_layouts_and_options() {
        let config = XkbConfig::from_query_output(
            "rules: evdev\nmodel: pc105\nlayout: us,ir\nvariant: ,\noptions: grp:alt_shift_toggle\n",
        )
        .unwrap();
        assert_eq!(config.layouts, vec!["us", "ir"]);
        assert_eq!(config.options, "grp:alt_shift_toggle");
        assert_eq!(config.variants, vec!["", ""]);
    }

    #[test]
    fn rejects_missing_layout_instead_of_guessing_us() {
        assert!(XkbConfig::from_query_output("rules: evdev\nmodel: pc105\n").is_err());
    }

    #[test]
    fn compiles_and_translates_us_and_persian_groups() {
        let config = XkbConfig {
            rules: "evdev".into(),
            model: "pc105".into(),
            layouts: vec!["us".into(), "ir".into()],
            variants: vec!["".into(), "".into()],
            options: String::new(),
        };
        let keymap = config.compile().expect("xkb-data and libxkbcommon installed");
        assert_eq!(keymap.translate_keycode(38, 0, 0).as_deref(), Some("a"));
        assert_eq!(keymap.translate_keycode(38, 1, 0).as_deref(), Some("ش"));
    }

    #[test]
    fn translates_us_shift_modifier_from_effective_mask() {
        let config = XkbConfig {
            rules: "evdev".into(),
            model: "pc105".into(),
            layouts: vec!["us".into(), "ir".into()],
            variants: vec!["".into(), "".into()],
            options: String::new(),
        };
        let keymap = config.compile().expect("xkb-data and libxkbcommon installed");

        // XKB's standard Shift modifier occupies bit zero. The XI2 event
        // adapter supplies the effective modifier mask at key-event time.
        assert_eq!(keymap.translate_keycode(38, 0, 1).as_deref(), Some("A"));
        assert_eq!(keymap.translate_keycode(10, 0, 1).as_deref(), Some("!"));
        assert_eq!(keymap.translate_keycode(38, 0, 0).as_deref(), Some("a"));
    }

    #[test]
    fn translates_persian_group_without_losing_group_selection() {
        let config = XkbConfig {
            rules: "evdev".into(),
            model: "pc105".into(),
            layouts: vec!["us".into(), "ir".into()],
            variants: vec!["".into(), "".into()],
            options: String::new(),
        };
        let keymap = config.compile().expect("xkb-data and libxkbcommon installed");

        // The active Persian group remains selected while the effective
        // modifier mask is passed through the same translation path.
        assert_eq!(keymap.translate_keycode(38, 1, 0).as_deref(), Some("ش"));
    }
}

    #[test]
    fn translates_qwertz_physical_keys_in_german_layout_group() {
        let config = XkbConfig {
            rules: "evdev".into(),
            model: "pc105".into(),
            layouts: vec!["us".into(), "de".into()],
            variants: vec!["".into(), "".into()],
            options: String::new(),
        };
        let keymap = config.compile().expect("US/German XKB keymap compiles");

        // The physical Y and Z positions swap in the German QWERTZ group.
        // This guards against assuming a US layout when translating keycodes.
        assert_eq!(keymap.translate_keycode(29, 0, 0).as_deref(), Some("y"));
        assert_eq!(keymap.translate_keycode(52, 0, 0).as_deref(), Some("z"));
        assert_eq!(keymap.translate_keycode(29, 1, 0).as_deref(), Some("z"));
        assert_eq!(keymap.translate_keycode(52, 1, 0).as_deref(), Some("y"));
        assert_eq!(keymap.translate_keycode(29, 1, 1).as_deref(), Some("Z"));
    }
}
