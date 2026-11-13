# Linux typing session state

The Linux core includes a backend-neutral `TypingSession` state machine. It mirrors relevant Windows behavior without depending on Win32 APIs:

- Collect text as produced by the active layout and evaluate a word at a space boundary.
- Preserve a pending wrong-layout run so subsequent words can be included.
- Permit a short preceding word to join a longer wrong-layout run.
- Offer converted text for acceptance with Tab.
- Clear pending text and suggestions on focus changes, navigation, pointer activity, shortcuts, cancellation, and disable.

## X11 event adapter boundary

The X11 connection provides two explicitly enabled XI2 observation paths:

- `subscribe_raw_key_events()` / `wait_for_raw_key_event()`: raw keycode and event metadata only. Raw events do not contain the effective modifier/group snapshot.
- `subscribe_key_events()` / `wait_for_key_event()`: focus-propagated device key events containing the server-supplied effective modifier mask and active group at event time, along with master/source device IDs and timestamp.

Neither path is enabled by the probe or automatically at startup. Neither grabs or consumes application input. The enriched device-event path is the candidate for future key translation; the raw path remains diagnostic only. Both are visible across applications on the same X server and therefore require explicit user opt-in.

## Adapter contract

A complete desktop adapter must:

1. Use the per-event modifier/group snapshot from XI2 device events with the active keymap; do not infer layout from raw keycodes.
2. Handle key repeat, compose/dead-key output, and keymap changes before forwarding committed printable text to `TypingSession`.
3. Forward focus changes, navigation, pointer clicks, shortcuts, and cancellation as reset events.
4. Keep typed text only in memory; do not log, persist, or transmit it.
5. Show suggestions only in supported contexts and fail closed when a secure-input state is exposed.
6. On Tab acceptance, perform replacement in the focused application and handle Unicode grapheme boundaries correctly. The core's Unicode-scalar deletion count is not yet a complete injection contract.
7. Keep capture and replacement inside the unprivileged user's graphical session. Never access raw evdev devices, install a root service, or use uinput.

## Current boundary

The X11 layer now has an opt-in XI2 device-event path that receives modifier/group snapshots from the server. It still does not decode text, process compose/dead-key sequences, detect focus changes reliably, display a tray icon or popup, or inject replacement text. The XKB compiler rebuilds from RMLVO configuration returned by `setxkbmap -query`; custom server-side keymap modifications may not be represented exactly. Device-event propagation and modifier/group correctness still need Xvfb and real desktop smoke tests before text decoding can be enabled.

X11 clients with access to the same X server do not have strong per-application input isolation. Reliable password-field detection must not be claimed. Users who require stronger isolation should disable X11 event observation; Wayland remains planned through IBus input-method integration.
