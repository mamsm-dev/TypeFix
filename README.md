# TypeFix for Linux

TypeFix detects likely English ⇄ Persian keyboard-layout mistakes on Linux and suggests the correctly converted text.

> **Development status:** TypeFix has a settings window (`typefix-gui`, GTK 3, light/dark theme with the #18B0B0 accent, English/Persian interface) and a command-line app (`typefix`). Select text and press a configurable shortcut to convert it between the English and Persian keyboard layouts. It is **not yet a complete desktop application** and has had only limited testing on a real desktop. Wayland support (KDE Plasma, shortcut conversion only) is new and untested on real hardware.

## Implemented

- Shared English/Persian word detection and conversion core.
- Backend-neutral typing-session state machine and coordinated Compose/dead-key lifecycle.
- X11/XInput2 event observation; XI2 device events carry event-time modifier/group snapshots, while the foreground raw-event path queries the event's master keyboard and documents its timing limitation.
- XKB key translation, Compose handling, focus/navigation reset handling, keymap-change refresh, and rebuilding the English/Persian conversion map when layouts change.
- Explicit foreground session loop that delivers non-empty session actions to a caller-provided callback.
- `typefix-gui` settings window: record a new shortcut, switch typing suggestions on/off, choose theme (system/light/dark) and interface language; closing it keeps TypeFix running in the background.
- `typefix` foreground app: a configurable global shortcut converts the selected text between the English and Persian layouts (the layout map is built from your active XKB layouts); optional `--suggest` mode shows generic, content-free mismatch notifications.
- Release packages (AppImage, `.deb`, `.rpm`, Arch) built by CI and published on GitHub Releases.
- Automated Xvfb integration coverage, including an end-to-end English-to-Persian suggestion test.

## Current status

| Area | Status |
|---|---|
| English/Persian detection core | Implemented and unit-tested |
| Typing-session and Compose lifecycle | Implemented and tested |
| X11/XInput2 session event loop | Implemented; Xvfb integration-tested |
| XKB translation and layout-group handling | Implemented; real desktop/layout validation remains |
| Key repeat and real KDE session behavior | Further validation required |
| Suggestions as desktop notifications (`notify-send`) | Implemented; not validated on a real desktop |
| Settings window (shortcut, suggestions, theme, language) | Implemented (GTK 3); layout checked with screenshots under Xvfb |
| Convert selected text with a configurable shortcut | Implemented; paste via Shift+Insert has only limited testing across applications |
| Tray icon (open window / quit) | Implemented; not yet tested on a real desktop |
| Wayland (KDE Plasma, via XDG portals) | Experimental: conversion shortcut only |
| IBus / Fcitx5 | Planned |
| Fcitx5 | Planned |
| Linux packages (AppImage, deb, rpm, Arch) | Built and published by CI from version tags; the AppImage with the settings window is untested on real systems |

See [linux/README.md](linux/README.md), [linux/SESSION.md](linux/SESSION.md), and [linux/SECURITY.md](linux/SECURITY.md) for technical details and security boundaries.

## Desktop and backend direction

The initial desktop target is KDE Plasma on X11, followed by GNOME and XFCE. The Wayland conversion shortcut uses the XDG desktop portals; an input-method integration (IBus, then Fcitx5) remains planned for typing suggestions.

X11 event observation is server-wide: it does not provide strong per-application isolation or reliable password-field detection. The session loop is foreground-only and must be started explicitly by the user. No privileged service, raw `/dev/input` access, uinput, or global input grab is used.

## Install

Download a package from the [latest release](https://github.com/mamsm-dev/TypeFix/releases/latest) (x86_64 only) and check it against `SHA256SUMS`:

```bash
# Debian / Ubuntu
sudo apt install ./typefix_0.4.0-1_amd64.deb
# Fedora / openSUSE
sudo dnf install ./typefix-0.4.0-1.x86_64.rpm
# Arch Linux
sudo pacman -U typefix-0.4.0-1-x86_64.pkg.tar.zst
# AppImage (any distribution)
chmod +x typefix-0.4.0-x86_64.AppImage && ./typefix-0.4.0-x86_64.AppImage
```

The release also contains a `typefix-debug` Arch package with debug symbols; you do not need it.

## Usage

### Settings window

Start **TypeFix** from your application menu (or run `typefix-gui`). The window shows the active conversion shortcut and lets you:

- **Change the shortcut:** click *Change shortcut*, then press the key combination you want (it needs at least one of Ctrl, Alt, Shift, Super; Esc cancels). If another application already uses the combination, the previous shortcut stays active and the window explains the problem.
- **Turn "Suggest while typing" on or off** and choose which layout the typed text comes from.
- **Pick the theme** (follow system, light or dark) and the **interface language** (English or فارسی, including right-to-left layout).

Settings are saved in `~/.config/typefix/config`, shared with the command-line `typefix`. Closing the window hides it and TypeFix keeps converting in the background; use the tray icon (or start TypeFix again) to show the window, or press *Quit* to stop it. A tray icon (StatusNotifierItem) with the TypeFix icon is shown too: click it or choose *Open TypeFix* to show the window, *Quit* to stop. GNOME needs the AppIndicator/KStatusNotifierItem extension to display tray icons (Zorin OS and Ubuntu include it). There is no autostart yet: add `typefix-gui` to your desktop's startup applications to run it at login.

### Command line

Requirements: an X11 session (needs the `setxkbmap` tool) or a Wayland session on KDE Plasma (shortcut conversion only; KDE asks permission for the shortcut and for keyboard input the first time, and you can change the shortcut in System Settings → Shortcuts). On X11 you also need and both a US/English layout and a Persian (`ir`) layout configured in your keyboard settings. `notify-send` (`libnotify`) is recommended; without it suggestions are printed to the terminal.

```bash
typefix                                # shortcut mode, default Ctrl+Alt+Space
typefix --shortcut "Super+Shift+F9"    # use another shortcut
typefix --suggest                      # also suggest conversions while typing
```

`typefix` runs in the foreground until you press Ctrl+C. **Select text in any application and press the shortcut**: text typed with the English layout (e.g. `sghl`) is replaced by the Persian text for the same keys (`سلام`), and Persian text is converted back to the English layout. The direction is detected from the selected text.

**Custom shortcut.** Use `--shortcut` or create `~/.config/typefix/config`:

```
# ~/.config/typefix/config
shortcut = Ctrl+Alt+Space
suggest = false
```

A shortcut is one or more modifiers (`Ctrl`, `Alt`, `Shift`, `Super`) plus one key name such as `Space`, `F9`, `c` or `Return`, joined with `+`. Command-line options override the config file. If another application already uses the combination, `typefix` reports that it could not grab it.

How it works: the selected text is read from the X selection, converted in memory, placed on the selection and clipboard, and pasted with a synthetic Shift+Insert (which works in GTK/Qt apps, browsers and terminals). Your previous clipboard text is restored afterwards. Applications that ignore Shift+Insert will not be replaced.

`--suggest` mode additionally watches typing: type a word with the wrong layout and press Space; the converted text appears as a notification (nothing is replaced). It also accepts `--source en|fa` and `--data-dir DIR` (or `TYPEFIX_DATA_DIR`) for the dictionary files. Other options: `--help`, `--version`.

## Build and test

The settings window is behind the `gui` cargo feature and needs the GTK 3 development files (`libgtk-3-dev` on Debian/Ubuntu):

```bash
cargo build --manifest-path linux/Cargo.toml --features gui --bin typefix-gui
```

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

The integration tests use XTest/xdotool only to synthesize input inside an isolated virtual X server. Production code injects keystrokes only for the shortcut's single Shift+Insert paste.

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
- Shortcut mode grabs only the one configured key combination and does not observe other keystrokes. Typing observation (`--suggest`) is opt-in, foreground-only and off by default.
- The selected text is read only when you press the shortcut, is kept in memory only, and replaces your clipboard for a moment (the previous text is restored). Other programs that watch the clipboard could see it.
- One synthetic Shift+Insert is sent through XTest to paste. No root service, raw evdev access, or uinput is used.
- Session state is reset on focus changes, navigation, shortcuts, cancellation, and relevant lifecycle events.
- X11 cannot reliably identify password fields or isolate one application from others on the same server. Disable X11 observation if that boundary is unacceptable.
- Suggestion notifications contain only a generic mismatch warning. The original and converted words are never passed to `notify-send` or written to notification history; typed text remains in the in-memory session.
- Wayland: only the conversion shortcut works (no typing suggestions), and it has not been tested on real KDE hardware yet.

## Project layout

- `linux/`: Rust core, X11 backend, the `typefix` and `typefix-gui` apps, packaging files, session documentation, and security model.
- `CHANGELOG.md`: changes of each version; the section of a version becomes its GitHub release notes.
- `.github/workflows/`: `linux-core.yml` (tests) and `linux-release.yml` (builds packages and publishes a GitHub Release when a `v*` tag is pushed).
- `Resources/en_50k.txt` and `Resources/fa_50k.txt`: shared English and Persian word-frequency data.
- `assets/`: project artwork and demo media.
- `icon/`: the application icon (`linux/packaging/icons/` holds the sizes used by the packages).

## License

Project code is MIT-licensed. Third-party data and assets retain their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
