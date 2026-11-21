# Linux input privacy model

TypeFix needs access to committed text during typing to recognize likely keyboard-layout mistakes. The Linux implementation uses different integration models for X11 and the planned Wayland backend.

## X11

X11 event observation is explicit opt-in and runs in the foreground. The session loop subscribes to XI2 key/focus events and XKB map notifications, translates key presses in memory, and sends session actions to a caller-provided callback.

This is not isolated to one application: events may be visible across applications on the same X server. X11 does not provide reliable password-field detection or strong per-application isolation. Users who require strict isolation should not enable the X11 session loop.

The `typefix` app passes each suggestion (the typed word and its conversion) to `notify-send`, i.e. to another local process and your desktop's notification service. That service may display it in a history or log it; TypeFix does not control this. If that is unacceptable, run the core without the notification frontend or disable notification history.

Shortcut mode (the default of the `typefix` app) passively grabs only the one configured key combination; it does not observe other keystrokes. When the shortcut is pressed it reads the selected text from the X selection, converts it in memory, temporarily replaces the clipboard/selection with the result, and sends a single synthetic Shift+Insert through XTest to paste it, then restores the previous clipboard text. Other programs that monitor the clipboard can observe the converted text during that moment. No uinput or raw device access is used.

The session loop (`--suggest`) does not inject or replace text. The current implementation does not create a daemon, install a privileged service, or access raw input devices. Typed content used by the session core is transient in-memory state; it must not be logged, persisted, or transmitted. Session and Compose state must be cleared on focus changes, navigation, shortcuts, cancellation, disable, and keymap changes.

## Wayland

The planned Wayland implementation is an input-method integration (IBus first, Fcitx5 later), not raw device monitoring. Input methods receive text as part of text entry, but applications may bypass or incompletely support the input-method path. Secure-input behavior must follow the capabilities exposed by the active input method; unsupported contexts must fail closed rather than claim universal password detection.

## Explicit non-goals

- No privileged daemon reading `/dev/input`.
- No silent global keyboard capture.
- No uinput-based injection (the shortcut paste uses a single XTest Shift+Insert).
- No claim of reliable password-field detection across arbitrary applications.
- No persistent typing history, telemetry, or network transmission of typed content.
