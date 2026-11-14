# TypeFix for Linux

TypeFix detects likely English ⇄ Persian keyboard-layout mistakes on Linux and suggests the correctly converted text.

> **Development status:** the Rust detection/session core and an opt-in X11 foreground session loop are implemented and covered by automated tests. TypeFix is **not yet a complete desktop application**: there is no suggestion popup, tray UI, or automatic text replacement.

## Implemented

- Shared English/Persian word detection and conversion core.
- Backend-neutral typing-session state machine and coordinated Compose/dead-key lifecycle.
- X11/XInput2 event observation with effective modifier and active-layout-group snapshots.
- XKB key translation, Compose handling, focus/navigation reset handling, and keymap-change refresh.
- Explicit foreground session loop that delivers non-empty session actions to a caller-provided callback.
- Automated Xvfb integration coverage, including an end-to-end English-to-Persian suggestion test.

## Current status

| Area | Status |
|---|---|
| English/Persian detection core | Implemented and unit-tested |
| Typing-session and Compose lifecycle | Implemented and tested |
| X11/XInput2 session event loop | Implemented; Xvfb integration-tested |
| XKB translation and layout-group handling | Implemented; real desktop/layout validation remains |
| Key repeat and real KDE session behavior | Further validation required |
| Suggestion popup, tray/menu UI | Not implemented |
| Safe text replacement in applications | Not implemented |
| Wayland / IBus | Planned |
| Fcitx5 | Planned |
| Linux packages | Not available |

See [linux/README.md](linux/README.md), [linux/SESSION.md](linux/SESSION.md), and [linux/SECURITY.md](linux/SECURITY.md) for technical details and security boundaries.

## Desktop and backend direction

The initial desktop target is KDE Plasma on X11, followed by GNOME and XFCE. Wayland support is planned through an input-method integration (IBus first, then Fcitx5).

X11 event observation is server-wide: it does not provide strong per-application isolation or reliable password-field detection. The session loop is foreground-only and must be started explicitly by the user. No privileged service, raw `/dev/input` access, uinput, or global input grab is used.

## Build and test

Requirements: Rust stable, a C toolchain, `libxkbcommon` development headers, X11/XInput2 development libraries, XKB data, Xvfb, and xdotool.

On Debian/Ubuntu:

```bash
sudo apt-get install build-essential pkg-config libxkbcommon-dev libx11-dev libxi-dev xkb-data x11-xkb-utils xvfb xdotool
```

Run Rust tests:

```bash
cargo test --manifest-path linux/Cargo.toml --lib --bins
```

Run X11/Xvfb integration tests:

```bash
xvfb-run -a cargo test --manifest-path linux/Cargo.toml --test x11_xvfb -- --nocapture
```

The integration tests use XTest/xdotool only to synthesize input inside an isolated virtual X server. Production code does not inject keystrokes.

For X11 capability preflight:

```bash
cargo run --manifest-path linux/Cargo.toml --bin typefix-x11-probe
```

For a deliberate foreground metadata diagnostic:

```bash
cargo run --manifest-path linux/Cargo.toml --bin typefix-x11-events
```

The diagnostic prints key metadata, not typed text. Stop it with Ctrl+C. It observes events visible to the X server, so use it only when intentionally testing.

## Privacy and security

- Typed text used by the in-memory detector/session is not logged, persisted, or transmitted.
- The X11 event path is opt-in and foreground-only; it is not started automatically.
- No root service, raw evdev access, uinput, or input grab is used.
- Session state is reset on focus changes, navigation, shortcuts, cancellation, and relevant lifecycle events.
- X11 cannot reliably identify password fields or isolate one application from others on the same server. Disable X11 observation if that boundary is unacceptable.
- Automatic text replacement and desktop UI are not implemented.

## Project layout

- `linux/`: Rust core, X11 backend, session documentation, and security model.
- `Resources/en_50k.txt` and `Resources/fa_50k.txt`: shared English and Persian word-frequency data.
- `assets/`: project artwork and demo media.

## License

Project code is MIT-licensed. Third-party data and assets retain their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
