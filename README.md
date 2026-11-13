# TypeFix for Linux

TypeFix is an English ⇄ Persian keyboard-layout mistake detector for Linux. The Linux port is under active development and is **not yet a complete desktop application**.

It currently contains the shared detection core, a backend-neutral typing-session state machine, X11/XInput2 connection and event-observation primitives, and XKB keymap compilation. It does not yet decode live text, show suggestions, or replace text in applications.

## Current status

| Area | Status |
|---|---|
| English/Persian detection core | Implemented with Rust tests |
| Typing-session state machine | Implemented with lifecycle/reset tests |
| X11 connection and XI2 capability preflight | Implemented |
| XKB keymap compilation and layout groups | Implemented; server-side custom keymap parity still needs validation |
| Opt-in XI2 raw key observer | Implemented; keycodes and metadata only |
| Opt-in XI2 device-event observer | Implemented; includes effective modifier/group snapshots; Xvfb and desktop validation pending |
| Text decoding and compose/dead keys | Not implemented |
| Focus tracking and secure-input handling | Not implemented |
| Suggestion popup, tray/menu UI, and safe replacement | Not implemented |
| Wayland / IBus | Planned |
| Fcitx5 | Planned |
| Linux packages | Not available |

See [linux/README.md](linux/README.md), [linux/SESSION.md](linux/SESSION.md), and [linux/SECURITY.md](linux/SECURITY.md) for implementation details and security boundaries.

## Supported desktop direction

The initial target is X11 on KDE Plasma, GNOME, and XFCE. Wayland support is planned through an input-method integration, IBus first and Fcitx5 later. There is no privileged service, raw `/dev/input` access, uinput, or global keyboard grab in the current implementation.

X11 event observation is server-wide and does not provide strong per-application isolation or reliable password-field detection. It remains explicit opt-in and must not be treated as a secure-input-aware monitor.

## Build and test

Requirements: Rust stable, a C toolchain, `libxkbcommon` development headers, X11/XInput2 development libraries, and XKB data.

On Debian/Ubuntu:

```bash
sudo apt-get install build-essential pkg-config libxkbcommon-dev libx11-dev libxi-dev xkb-data x11-xkb-utils
```

Run the Rust tests:

```bash
cargo test --manifest-path linux/Cargo.toml
```

Run the X11 preflight probe from an X11 graphical session:

```bash
cargo run --manifest-path linux/Cargo.toml --bin typefix-x11-probe
```

The probe checks the X11 connection, XInput2, and XKB configuration. It does **not** subscribe to or capture key events.

## Privacy and security

- No typed text is logged, persisted, or transmitted by the current X11 observer.
- The current event layer returns key/event metadata only; text decoding is not enabled.
- No root service, raw evdev access, uinput, or input grab is used.
- Any future text buffer must remain in memory only and be cleared on focus changes, navigation, shortcuts, cancellation, and pause/quit.
- X11 does not offer reliable per-application isolation or reliable password-field detection. Users requiring strict isolation should keep X11 event observation disabled.

## Project layout

- `linux/`: Rust core, X11 backend primitives, security notes, and session adapter contract.
- `Resources/en_50k.txt` and `Resources/fa_50k.txt`: shared English and Persian word-frequency data.
- `assets/`: project artwork and demo media.

## License

The project code is MIT-licensed. Third-party data and assets retain their respective licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
