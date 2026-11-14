# Linux typing session state

The Linux core includes a backend-neutral `TypingSession` state machine and a `ComposedTypingSession` wrapper. The session collects committed text in memory, evaluates likely layout mistakes at word boundaries, and exposes suggestions as session actions. Tab acceptance is represented in the core; the desktop application must still decide how to present and apply it.

Session and Compose state are reset on focus changes, navigation, pointer activity, shortcuts, cancellation, disable, and explicit reset events.

## X11 event adapter

The X11 connection exposes explicit observation APIs:

- `subscribe_raw_key_events()` / `wait_for_raw_key_event()`: raw keycode and event metadata only.
- `subscribe_key_events()` / `wait_for_key_event()`: key events with server-supplied effective modifier and active-group snapshots, plus master/source device IDs and timestamps.
- `subscribe_focus_events()`: focus-in/focus-out metadata.
- `subscribe_session_events()` with `poll_for_session_event()` or `wait_for_session_event()`: combined key and focus stream. Prefer this combined subscription because XI2 selections are per device/window.
- `subscribe_xkb_map_events()`: XKB map/new-keyboard notifications. On change, session state is reset and `refresh_keymap()` rebuilds the keymap translator from the current XKB RMLVO settings.

`XkbKeymap::session_event_for_key()` translates a key press using its event-time modifier/group snapshot and maps the result to a session event (text/Compose, Space, Backspace, Tab, navigation, or shortcut). Key releases do not produce text.

`X11SessionConnection::handle_session_event()` dispatches one X11 event into `ComposedTypingSession`. `X11SessionConnection::run_session_loop()` provides the explicit foreground loop: it enables the required subscriptions, dispatches events, and invokes a caller-provided callback for non-empty `SessionAction` values until the caller's stop predicate requests termination.

No observer is enabled by the probe or silently at application startup. The foreground loop is opt-in. It does not grab or consume application input, log typed text, or inject replacements. The diagnostic binary `typefix-x11-events` reports metadata only and should be used only during an intentional test.

## Compose and dead keys

The Compose processor wraps `libxkbcommon`'s locale-specific Compose state machine:

- Incomplete sequences produce no committed text.
- Completed sequences return one UTF-8 string.
- Cancelled sequences are discarded.
- Explicit reset discards pending composition.

`ComposedTypingSession` coordinates Compose resets with typing-buffer lifecycle events. The XKB translator feeds Compose before ordinary key-to-text translation. Typed content and pending composition remain in memory; they are not persisted or transmitted by these components.

## Adapter and UI contract

A complete desktop application must:

1. Use the XI2 event-time modifier/group snapshot with the active keymap; never infer the active layout from raw keycodes alone.
2. Define and test key-repeat semantics so repeated press/release events cannot corrupt text or Compose state.
3. Forward focus changes, navigation, pointer activity, shortcuts, cancellation, disable, and keymap changes as reset events.
4. Keep typed content only in memory and never log, persist, or transmit it.
5. Show suggestions only in supported contexts and fail closed when a platform exposes secure-input state.
6. Treat Tab acceptance as a request to accept a suggestion, not as proof that text replacement is implemented. A future replacement adapter must handle Unicode grapheme boundaries and verify focus before acting.
7. Run only in the unprivileged user's graphical session. Never use raw evdev access, a root service, or uinput.

## Current limitations

X11 clients with access to the same server do not have strong per-application input isolation. Reliable password-field detection must not be claimed. Custom server-side keymap modifications may not be represented exactly by rebuilding from RMLVO settings. Automated Xvfb tests are not a substitute for real KDE/X11 validation.

The real desktop smoke test remains pending. The UI, safe replacement, Wayland/IBus, and Fcitx5 are not implemented.
