# Linux implementation

This directory contains TypeFix's Linux Rust implementation. The former macOS and Windows application source trees are not part of this Linux-focused repository.

## Desktop support

The first target is KDE Plasma on X11, followed by GNOME and XFCE. Desktop detection is informational; the core must not depend on a specific shell, tray, or widget toolkit.

## Input backends

### X11

The X11 backend provides an unprivileged connection, XI2 session-event subscriptions, XKB keymap translation, Compose/dead-key processing, focus lifecycle handling, and explicit keymap refresh.

The `X11SessionConnection::run_session_loop()` API is an explicitly invoked foreground loop. It subscribes to key/focus and XKB map-change events, dispatches them into `ComposedTypingSession`, and passes non-empty session actions to a callback supplied by the caller. It does not create a daemon, UI, tray icon, or automatic startup entry, and it does not inject or replace text.

Automated Xvfb tests cover event delivery, layout-group/modifier snapshots, keymap refresh, lifecycle resets, and an end-to-end English-to-Persian suggestion path. These tests do not replace validation in a real KDE/X11 desktop session. Key-repeat behavior, custom server keymaps, and broader hardware/layout combinations still require validation.

X11 event visibility is server-wide. It offers neither strong per-application isolation nor reliable password-field detection. The session loop must remain an explicit user action and must not be described as secure-input-aware.

### Wayland

Wayland does not provide ordinary desktop applications with a safe, compositor-independent global keyboard hook. The planned backend is an input-method integration, starting with IBus and followed by Fcitx5 compatibility—not raw evdev monitoring or uinput injection.

Input-method support depends on the active input method and on applications accepting its preedit/commit path. It cannot guarantee identical behavior in every application.

## Privacy and security requirements

- No privileged global keyboard-capture service.
- No raw `/dev/input/event*` access and no uinput injection.
- No network access, telemetry, or persistent typing history.
- Typed content is kept only in the short-lived in-memory session state required for detection.
- Clear session and Compose state on focus changes, navigation, shortcuts, cancellation, disable, and keymap changes.
- Do not claim arbitrary password fields can be detected reliably.
- Document each backend's actual visibility and limitations before enabling it.

## Implementation status

Implemented: shared English/Persian detection; backend-neutral typing and Compose state; X11/XInput2 event delivery; XKB translation using server-supplied modifier/group snapshots; key classification; focus and keymap-change handling; and a callback-driven foreground session loop.

Not implemented: desktop suggestion popup, tray/menu UI, safe text replacement, Wayland/IBus, Fcitx5, and Linux release packages.

See [SESSION.md](SESSION.md) for the event/session contract and [SECURITY.md](SECURITY.md) for the privacy boundary.

## Shared behavior and data

The Linux implementation uses the shared English and Persian word-frequency files in `../Resources/`. Layout conversion uses the active XKB keymap rather than a hard-coded US mapping.

## Release artifacts

Linux packages are not available yet. Add AppImage, Debian, RPM, and Arch packaging only after the desktop application and runtime dependencies are buildable and tested in CI. Do not publish placeholder packages.

## Milestones

1. Shared detection core and backend-neutral typing-session state machine — implemented.
2. X11 connection, XI2 observation, XKB translation, Compose, focus/keymap lifecycle, and foreground session loop — implemented; Xvfb-tested.
3. Real KDE/X11 smoke testing, key-repeat semantics, and broader keymap/device validation — pending.
4. Desktop suggestion UI with explicit lifecycle and privacy behavior — pending design/implementation.
5. Wayland IBus integration, then Fcitx5 compatibility — planned.
6. Build and test Linux release formats, then add them to a tag release workflow — planned.
