# Linux input privacy model

TypeFix needs access to text as it is entered to recognize a wrong keyboard layout. The Linux implementation deliberately uses different mechanisms on X11 and Wayland.

## X11

The X11 session backend is a user process, not a root service. X11 does not provide strong per-application input isolation: a client with access to the X server may be able to observe keystrokes from other clients. TypeFix must therefore keep its buffer in memory only, avoid logging typed text, and provide an immediate pause/quit control. Users who require strict isolation should disable the X11 backend.

## Wayland

The Wayland implementation is based on an input method (IBus/Fcitx5), not raw device monitoring. Input methods receive text as part of text entry and may be bypassed by applications or toolkit configurations. This is a compatibility trade-off, not a universal guarantee that every sensitive field is automatically recognized.

## Explicit non-goals

The project will not install a privileged daemon that reads /dev/input, will not silently enable uinput injection, and will not promise reliable password-field detection across arbitrary apps.
