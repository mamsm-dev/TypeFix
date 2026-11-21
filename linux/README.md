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

Not implemented: tray/menu UI, safe text replacement, Wayland/IBus, and Fcitx5. Suggestions are shown as desktop notifications by the foreground `typefix` binary.

See [SESSION.md](SESSION.md) for the event/session contract and [SECURITY.md](SECURITY.md) for the privacy boundary.

## Shared behavior and data

The Linux implementation uses the shared English and Persian word-frequency files in `../Resources/`. Layout conversion uses the active XKB keymap rather than a hard-coded US mapping.

## The `typefix` app

`src/bin/typefix.rs` is a foreground X11 frontend. It connects to the X server, builds the layout conversion map from the active XKB layout groups (`us` or the first non-`ir` layout, and `ir`), runs `X11SessionConnection::run_session_loop()`, and shows each suggestion with `notify-send` (falling back to the terminal). It does not replace text and is not started automatically. `--source en|fa` selects the direction; dictionaries are found via `--data-dir`, `TYPEFIX_DATA_DIR`, `../share/typefix` next to the binary (AppImage), `/usr/share/typefix`, or the repository's `Resources/`.

Suggestion text is handed to `notify-send` and thus to the desktop notification service, which may retain it; see [SECURITY.md](SECURITY.md).

## Release artifacts

Pushing a tag such as `v0.1.0` (it must match the version in `Cargo.toml`) runs `.github/workflows/linux-release.yml`, which builds and attaches to the GitHub Release:

- `.AppImage` (x86_64, via linuxdeploy)
- `.deb` (cargo-deb)
- `.rpm` (cargo-generate-rpm)
- Arch `.pkg.tar.zst` (makepkg in an Arch container, from `packaging/arch/PKGBUILD`; a `typefix-debug` package is produced as well)
- `SHA256SUMS`

The v0.1.0 release was built by this workflow and published successfully. The packages have not yet been installed and run on real systems, and AppImage/rpm/deb/Arch are x86_64 only.

## Milestones

1. Shared detection core and backend-neutral typing-session state machine — implemented.
2. X11 connection, XI2 observation, XKB translation, Compose, focus/keymap lifecycle, and foreground session loop — implemented; Xvfb-tested.
3. Real KDE/X11 smoke testing, key-repeat semantics, and broader keymap/device validation — pending.
4. Desktop suggestion UI with explicit lifecycle and privacy behavior — notification-based version implemented; tray/menu UI pending.
5. Wayland IBus integration, then Fcitx5 compatibility — planned.
6. Build and test Linux release formats, then add them to a tag release workflow — implemented (v0.1.0); install testing on real distributions pending.
