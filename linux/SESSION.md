# Linux typing session state

The Linux core includes a backend-neutral `TypingSession` state machine. It mirrors relevant Windows behavior without depending on Win32 APIs:

- Collect text as produced by the active layout and evaluate a word at a space boundary.
- Preserve a pending wrong-layout run so subsequent words can be included.
- Permit a short preceding word to join a longer wrong-layout run.
- Offer converted text for acceptance with Tab.
- Clear pending text and suggestions on focus changes, navigation, pointer activity, shortcuts, cancellation, and disable.

## X11 event adapter boundary

The X11 connection now exposes an explicit `subscribe_raw_key_events()` opt-in and `wait_for_raw_key_event()` observer. It subscribes to XI2 raw key press/release events for master devices and returns only the X11 keycode, device ID, timestamp, and press/release direction. It does not grab or consume input, decode text, log text, or inject replacements. The probe does not enable this subscription.

**Important:** XI2 raw key events do not include the current XKB modifier/group snapshot. The raw event observer must not guess the layout from keycodes. A separate XKB state-notification integration is required before text translation is safe. X11 raw observation is server-wide and has weak per-application isolation.

## Adapter contract

A complete desktop adapter must:

1. Obtain a coherent active XKB keymap and modifier/group state, including compose/dead-key output. Do not hard-code US key positions or infer group state from raw keycodes.
2. Forward only committed printable text to `TypingSession`; do not log, persist, or transmit it.
3. Forward focus changes, navigation, pointer clicks, shortcuts, and cancellation as reset events.
4. Show suggestions only in supported contexts and fail closed when a secure-input state is exposed.
5. On Tab acceptance, perform replacement in the focused application and handle Unicode grapheme boundaries correctly. The core currently reports a Unicode-scalar deletion count as a low-level hint; this is not yet a complete injection contract.
6. Keep capture and replacement inside the unprivileged user's graphical session. Never access raw evdev devices, install a root service, or use uinput.

## Current boundary

The current X11 work provides an opt-in raw keycode observer, but it is not a text listener or a complete typing backend. It does not yet track XKB modifier/group state, process compose/dead-key sequences, detect focus changes reliably, display a tray icon or popup, or inject replacement text. The XKB compiler rebuilds from RMLVO configuration returned by `setxkbmap -query`; custom server-side keymap modifications may not be represented exactly. Do not describe the Linux app as globally correcting text until the full adapter and desktop tests exist.

X11 clients with access to the same X server do not have strong per-application input isolation. Reliable password-field detection must not be claimed. Users who require stronger isolation should disable X11 event observation; Wayland remains planned through IBus input-method integration.
