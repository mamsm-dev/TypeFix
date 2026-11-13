# Linux input privacy model

TypeFix needs access to text as it is entered to recognize a wrong keyboard layout. The Linux implementation deliberately uses different mechanisms on X11 and Wayland.

## X11

The X11 connection and keymap probe do not subscribe to key events. A separate `subscribe_raw_key_events()` method provides explicit opt-in to XI2 raw key press/release observation for master devices. It returns keycodes and event metadata only; it does not decode text, grab or consume events, inject text, or write typed content to logs. The probe never enables this subscription.

Raw XI2 events are server-wide, not scoped to the focused application, and X11 does not provide strong per-application input isolation. The observer must not be enabled silently. Any future text buffer must remain in memory only, avoid logs/persistence/network transmission, and clear on focus changes, navigation, shortcuts, cancellation, and pause/quit. Reliable password-field detection is not available; users who require strict isolation should disable X11 event observation.

Raw key events do not include a coherent active XKB modifier/group snapshot. Do not translate raw keycodes into text until a separate XKB state-notification path is implemented and tested.

## Wayland

The Wayland implementation is based on an input method (IBus/Fcitx5), not raw device monitoring. Input methods receive text as part of text entry and may be bypassed by applications or toolkit configurations. This is a compatibility trade-off, not a universal guarantee that every sensitive field is automatically recognized.

## Explicit non-goals

The project will not install a privileged daemon that reads /dev/input, will not silently enable uinput injection, and will not promise reliable password-field detection across arbitrary apps.
