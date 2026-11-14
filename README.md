# TypeFix for Linux

TypeFix is an English ⇄ Persian keyboard-layout mistake detector for Linux. The Linux port is under active development and is **not yet a complete desktop application**.

It currently contains the shared detection core, a backend-neutral typing-session state machine with coordinated Compose resets, X11/XInput2 connection and event-observation primitives, XKB keymap compilation/refresh, and an in-memory key-to-session dispatcher. The dispatcher handles one event at a time; a continuously running desktop loop, suggestion UI, and text replacement in applications are not implemented.

## Current status

| Area | Status |
|---|---|
| English/Persian detection core | Implemented with Rust tests |
| Typing-session state machine | Implemented with lifecycle/reset tests |
| X11 connection and XI2 capability preflight | Implemented |
| XKB keymap compilation, layout groups, and map-change refresh | Implemented; refresh is explicit after XKB notifications; server-side custom keymap parity still needs validation |
| Opt-in XI2 raw key observer | Implemented; keycodes and metadata only |
| Opt-in XI2 device-event observer | Implemented; effective modifier/group snapshots verified under Xvfb; real desktop validation pending |
| XKB key-event translation and session-event classification | Implemented as explicit in-memory primitives; not wired into the live session |
| Opt-in XI2 focus event delivery | Implemented as event metadata; client/session lifecycle wiring and secure-input handling remain incomplete |
| Suggestion popup, tray/menu UI, and safe replacement | Not implemented |
| Wayland / IBus | Planned |
| Fcitx5 | Planned |
| Linux packages | Not available |

See [linux/README.md](linux/README.md), [linux/SESSION.md](linux/SESSION.md), and [linux/SECURITY.md](linux/SECURITY.md) for implementation details and security boundaries.

## Supported desktop direction

The initial target is X11 on KDE Plasma, GNOME, and XFCE. Wayland support is planned through an input-method integration, IBus first and Fcitx5 later. There is no privileged service, raw `/dev/input` access, uinput, or global keyboard grab in the current implementation.

X11 event observation is server-wide and does not provide strong per-application isolation or reliable password-field detection. It remains explicit opt-in and must not be treated as a secure-input-aware monitor.

## Build and test

Requirements: Rust stable, a C toolchain, `libxkbcommon` development headers, X11/XInput2 development libraries, XKB data, Xvfb, and xdotool for the integration test.

On Debian/Ubuntu:

```bash
sudo apt-get install build-essential pkg-config libxkbcommon-dev libx11-dev libxi-dev xkb-data x11-xkb-utils xvfb xdotool
```

Run the core and unit tests:

```bash
cargo test --manifest-path linux/Cargo.toml --lib --bins
```

Run the XI2 event-delivery integration test in an isolated virtual X server:

```bash
xvfb-run -a cargo test --manifest-path linux/Cargo.toml --test x11_xvfb -- --nocapture
```

The Xvfb test uses xdotool/XTest only to synthesize test keystrokes in the isolated CI/test display. The production code does not inject input.

Run the X11 preflight probe from an X11 graphical session:

```bash
cargo run --manifest-path linux/Cargo.toml --bin typefix-x11-probe
```

The probe checks the X11 connection, XInput2, and XKB configuration. It does **not** subscribe to or capture key or focus events.

## Privacy and security

- No typed text is logged, persisted, or transmitted by the current X11 observer.
- The event-observation layer returns key/event metadata only. A separate explicit translator can decode a supplied XI2 device-event snapshot in memory, but it is not started automatically or connected to a live session.
- No root service, raw evdev access, uinput, or input grab is used.
- Any text buffer must remain in memory only and be cleared on focus changes, navigation, shortcuts, cancellation, and pause/quit. The opt-in XI2 focus event API is a metadata source for a future session adapter; it is not yet wired to the typing buffer.
- X11 does not offer reliable per-application isolation or reliable password-field detection. Users requiring strict isolation should keep X11 event observation disabled.

## Project layout

- `linux/`: Rust core, X11 backend primitives, security notes, and session adapter contract.
- `Resources/en_50k.txt` and `Resources/fa_50k.txt`: shared English and Persian word-frequency data.
- `assets/`: project artwork and demo media.

## License

The project code is MIT-licensed. Third-party data and assets retain their respective licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
