# Linux input privacy model

TypeFix needs access to text as it is entered to recognize a wrong keyboard layout. The Linux implementation deliberately uses different mechanisms on X11 and Wayland.

## X11

The X11 connection and keymap probe do not subscribe to key events. Two separate XI2 methods provide explicit opt-in:

- Raw key events expose keycodes and metadata only; they do not contain effective XKB modifiers/group.
- Focus-propagated device key events include the server-supplied effective modifier/group snapshot at event time.

Neither subscription is enabled by the probe or silently at startup. Neither grabs or consumes application input. The observer returns key metadata only; this layer does not decode text, inject text, or write typed content to logs.

XI2 observation is server-wide, not strongly isolated to the focused application. X11 does not provide strong per-application input isolation or reliable password-field detection. Any future text buffer must remain in memory only, avoid logs/persistence/network transmission, and clear on focus changes, navigation, shortcuts, cancellation, and pause/quit. Users who require strict isolation should disable X11 event observation.

## Wayland

The Wayland implementation is based on an input method (IBus/Fcitx5), not raw device monitoring. Input methods receive text as part of text entry and may be bypassed by applications or toolkit configurations. This is a compatibility trade-off, not a universal guarantee that every sensitive field is automatically recognized.

## Explicit non-goals

The project will not install a privileged daemon that reads /dev/input, will not silently enable uinput injection, and will not promise reliable password-field detection across arbitrary apps.
