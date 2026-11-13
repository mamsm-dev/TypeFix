# Linux input privacy model

TypeFix needs access to text as it is entered to recognize a wrong keyboard layout. The Linux implementation deliberately uses different mechanisms on X11 and Wayland.

## X11

The current X11 layer only opens a connection as the desktop user, checks XI2 availability, and compiles configured XKB layout metadata. It does not yet subscribe to key events or inject text. Any future event listener will be a user process, not a root service. X11 does not provide strong per-application input isolation: a client with access to the X server may be able to observe keystrokes from other clients. A future listener must keep its buffer in memory only, avoid logging typed text, and provide an immediate pause/quit control. Reliable password-field detection is not available; users who require strict isolation should disable the X11 event backend.

## Wayland

The Wayland implementation is based on an input method (IBus/Fcitx5), not raw device monitoring. Input methods receive text as part of text entry and may be bypassed by applications or toolkit configurations. This is a compatibility trade-off, not a universal guarantee that every sensitive field is automatically recognized.

## Explicit non-goals

The project will not install a privileged daemon that reads /dev/input, will not silently enable uinput injection, and will not promise reliable password-field detection across arbitrary apps.
