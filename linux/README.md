# Linux port

The Linux port is being developed as a separate implementation. Existing macOS and Windows applications remain unchanged.

## Desktop support

Targets: KDE Plasma, GNOME, and XFCE. Desktop detection is informational only; the application must not depend on a particular shell or tray implementation.

## Input backends

### X11

The X11 backend will run as a per-user process in the graphical session. It will use the X server for key events and text injection, and must not require a root daemon or read raw evdev devices. It must detect unavailable permissions or unsupported sessions and fail closed.

### Wayland

Wayland does not provide ordinary desktop applications with a safe, compositor-independent global keyboard hook. The Wayland backend will therefore be an input-method integration (IBus first, with Fcitx5 compatibility as a follow-up), rather than an evdev/uinput keylogger.

This means Wayland support depends on the active input method and applications accepting its preedit/commit path. It cannot promise identical behavior in every application.

## Privacy and security requirements

- No privileged global keyboard capture service.
- No raw /dev/input/event* access and no uinput injection.
- No network access, telemetry, or persistent typing history.
- Keep only the short in-memory text buffer needed for detection; clear it on focus changes, navigation, shortcuts, and explicit cancellation.
- Do not claim arbitrary password fields can be detected reliably. The input-method backend must avoid suggestions in secure input contexts where the platform exposes that state; unsupported contexts must fail closed.
- Document each backend's actual visibility and limitations before enabling it.

## Shared behavior

Reuse the bundled English/Persian word lists and detection rules from the Windows implementation. Layout conversion must use the active XKB keymap, not a hard-coded US mapping, so Persian and non-US layouts work correctly.

## Release artifacts

Each tagged release should eventually attach:

- TypeFix-Linux-x86_64.AppImage
- typefix_<version>_amd64.deb
- typefix_<version>_x86_64.rpm
- typefix-<version>-1-x86_64.pkg.tar.zst

All packages must contain the same tested Linux build. Enable AppImage, Debian, RPM, and Arch packaging only after the Linux application and runtime dependencies are buildable in CI; never publish placeholder or empty packages.

## Milestones

1. Shared detection core and XKB layout conversion, with automated tests.
2. X11 session backend and desktop-session smoke tests.
3. IBus Wayland integration; validate GNOME, KDE Plasma, and XFCE configurations.
4. Fcitx5 compatibility and documented limitations.
5. Build and test all four Linux release formats, then add them to the tag release workflow.
