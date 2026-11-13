# Linux implementation

The Linux port is a standalone implementation. The former macOS and Windows application source trees are not part of this Linux-focused repository.

## Desktop support

Targets: KDE Plasma, GNOME, and XFCE. Desktop detection is informational only; the application must not depend on a particular shell or tray implementation.

## Input backends

### X11

The X11 layer provides an unprivileged X-server connection, XI2 preflight, XKB keymap compilation, and explicit opt-in key observation. The raw observer returns keycodes only. The focus-propagated XI2 device-event observer also returns the server-supplied effective modifier mask and active group at event time. Neither observer is enabled by the probe or automatically at startup; neither grabs or consumes input. Run `cargo run --manifest-path linux/Cargo.toml --bin typefix-x11-probe` inside an X11 session to check prerequisites; the probe does not capture key events.

The device-event path is the candidate for future key translation. Its delivery of key press/release events and effective Shift/group snapshots has been verified under Xvfb. Real desktop validation, key repeats, compose sequences, keymap changes, and focus behavior still need testing before text decoding is enabled. X11 observation is server-wide and has weak per-application isolation. It does not provide reliable password-field detection. The backend must remain a per-user process and must not require a root daemon or raw evdev access.

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

## Implementation status

The shared English/Persian detector, backend-neutral typing-session state machine, X11 connection/keymap preflight, and explicit opt-in XI2 raw and device-event observers are implemented. The device-event observer exposes server-supplied effective modifier/group metadata, but does not decode text or inject replacements. The Xvfb integration test now verifies XI2 key press/release delivery and effective Shift/group snapshots. Compose/dead-key processing, focus resets, keymap-change handling, popup/tray UI, safe replacement, and real desktop-session smoke tests remain pending. See [SESSION.md](SESSION.md) for the adapter contract and exact boundary.

## Shared behavior and data

The Linux implementation uses the shared English and Persian word-frequency files in `../Resources/`. Layout conversion must use the active XKB keymap, not a hard-coded US mapping, so Persian and non-US layouts work correctly.

## Release artifacts

Linux packages are not available yet. Enable AppImage, Debian, RPM, and Arch packaging only after the Linux application and runtime dependencies are buildable and tested in CI; never publish placeholder or empty packages.

## Milestones

1. Shared detection core and backend-neutral typing-session state machine.
2. X11 connection/XInput2 capability preflight and XKB layout-group translation (implemented).
3. Explicit opt-in XI2 raw key observation (implemented; keycodes only).
4. Explicit opt-in XI2 device key events with effective modifier/group metadata (Xvfb event delivery verified; real desktop validation required before text decoding).
5. Compose/dead-key handling, focus resets, safe replacement, and desktop-session smoke tests.
6. IBus Wayland integration; validate GNOME, KDE Plasma, and XFCE configurations.
7. Fcitx5 compatibility and documented limitations.
8. Build and test Linux release formats, then add them to a tag release workflow.
