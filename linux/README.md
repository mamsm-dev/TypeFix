# Linux port

The Linux port is being developed as a separate implementation. Existing macOS and Windows applications remain unchanged.

## Desktop support

Targets: KDE Plasma, GNOME, and XFCE. Desktop detection is informational only; the application must not depend on a particular shell or tray implementation.

## Input backends

### X11

The X11 layer provides an unprivileged X-server connection, XI2 preflight, XKB keymap compilation, and an explicit opt-in raw keycode observer. The observer must be enabled by calling `subscribe_raw_key_events()`; it is not enabled by the probe or automatically at startup. It observes raw key press/release metadata for master devices without grabbing or consuming application input, decoding text, or injecting replacements. Run `cargo run --manifest-path linux/Cargo.toml --bin typefix-x11-probe` inside an X11 session to check prerequisites; the probe does not capture key events.

Raw XI2 events do not contain a coherent XKB modifier/group snapshot, so text translation must remain disabled until a separate XKB state-notification path is implemented. X11 observation is server-wide and has weak per-application isolation. It does not provide reliable password-field detection. The backend must remain a per-user process and must not require a root daemon or raw evdev access.

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

The shared detector, backend-neutral typing-session state machine, X11 connection/keymap preflight, and explicit opt-in XI2 raw keycode observer are implemented. The observer emits keycodes and event metadata only; it does not decode text or inject replacements. Live XKB modifier/group tracking, compose sequences, focus resets, popup/tray UI, safe replacement, and desktop-session smoke tests remain pending. See [SESSION.md](SESSION.md) for the adapter contract and exact boundary.

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

1. Shared detection core and backend-neutral typing-session state machine.
2. X11 connection/XInput2 capability preflight and XKB layout-group translation (implemented).
3. Explicit opt-in XI2 raw key observation (implemented; keycodes only, no text decoding).
4. XKB state/modifier/group notifications, compose handling, focus resets, safe replacement, and desktop-session smoke tests.
5. IBus Wayland integration; validate GNOME, KDE Plasma, and XFCE configurations.
6. Fcitx5 compatibility and documented limitations.
7. Build and test all four Linux release formats, then add them to the tag release workflow.
