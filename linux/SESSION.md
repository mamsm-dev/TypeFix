# Linux typing session state

The Linux core includes a backend-neutral `TypingSession` state machine:

- Collect text as produced by the active layout and evaluate a word at a space boundary.
- Preserve a pending wrong-layout run so subsequent words can be included.
- Permit a short preceding word to join a longer wrong-layout run.
- Offer converted text for acceptance with Tab.
- Clear pending text and suggestions on focus changes, navigation, pointer activity, shortcuts, cancellation, and disable.

## X11 event adapter boundary

The X11 connection provides three explicitly enabled XI2 observation paths:

- `subscribe_raw_key_events()` / `wait_for_raw_key_event()`: raw keycode and event metadata only. Raw events do not contain the effective modifier/group snapshot.
- `subscribe_key_events()` / `wait_for_key_event()`: focus-propagated device key events containing the server-supplied effective modifier mask and active group at event time, along with master/source device IDs and timestamp.\n- `subscribe_focus_events()` / `poll_for_session_event()` or `wait_for_session_event()`: focus-in/focus-out metadata from XI2. The combined session event stream returns key and focus events without one blocking wait path discarding the other.

None of these subscriptions is enabled by the probe or automatically at startup. None grabs or consumes application input. The enriched device-event path is the candidate for future key translation; the raw path remains diagnostic only. Focus notifications can be mapped to `SessionEvent::FocusChanged` by a future adapter, but the current code does not yet coordinate focus notifications with a live typing buffer. All three observation paths are visible across applications on the same X server and therefore require explicit user opt-in. The `typefix-x11-events` foreground diagnostic uses the enriched path and prints key metadata only; run it only during an intentional desktop smoke test and stop it with Ctrl+C.

## Compose/dead-key state

The `compose` module wraps `libxkbcommon`'s locale-specific Compose state machine:

- A key symbol is fed to Compose before ordinary key-to-text translation.
- Incomplete sequences produce no text; completed sequences return one committed UTF-8 string.
- Cancelled sequences are discarded, and callers can explicitly reset pending composition.
- The `ComposedTypingSession` wrapper couples Compose and typing-buffer lifecycle: focus changes, navigation, pointer activity, shortcuts, Backspace, Tab, explicit reset, and disable clear pending Compose state as well as the typing buffer. The platform adapter must still call these events when the corresponding desktop lifecycle event occurs.

The Compose processor remains an in-memory component. `ComposedTypingSession` now coordinates its reset lifecycle with `TypingSession`. The XKB layer exposes an explicit per-event translator that takes an XI2 device-event modifier/group snapshot, obtains the keysym, feeds Compose first, and returns only committed text. Neither component subscribes to events, retains a typed history, logs text, or injects text. The translator is not yet connected to the live event loop or `TypingSession`. Compose tests use a tiny deterministic table and do not depend on the host's locale files.

## Adapter contract

A complete desktop adapter must:

1. Use the per-event modifier/group snapshot from XI2 device events with the active keymap; do not infer layout from raw keycodes.
2. Handle key repeat, Compose/dead-key output, and keymap changes before forwarding committed printable text to `TypingSession`.
3. Forward focus changes, navigation, pointer clicks, shortcuts, and cancellation as reset events.
4. Keep typed text only in memory; do not log, persist, or transmit it.
5. Show suggestions only in supported contexts and fail closed when a secure-input state is exposed.
6. On Tab acceptance, perform replacement in the focused application and handle Unicode grapheme boundaries correctly. The core's Unicode-scalar deletion count is not yet a complete replacement contract.
7. Keep capture and replacement inside the unprivileged user's graphical session. Never access raw evdev devices, install a root service, or use uinput.

## Current boundary

The X11 layer now has an opt-in XI2 device-event path that receives modifier/group snapshots from the server. The isolated Xvfb integration test verifies delivery of key press/release events and checks the effective Shift modifier and group 0 snapshot. It does not validate every keyboard layout or a real desktop compositor/session. The implementation has an explicit per-event XKB/Compose translation primitive, but does not yet connect it to the live event loop or `TypingSession`; it also does not detect focus changes reliably, display a tray icon or popup, or replace text. The XKB compiler rebuilds from RMLVO configuration returned by `setxkbmap -query`; custom server-side keymap modifications may not be represented exactly.

X11 clients with access to the same X server do not have strong per-application input isolation. Reliable password-field detection must not be claimed. Users who require stronger isolation should disable X11 event observation; Wayland remains planned through IBus input-method integration.
