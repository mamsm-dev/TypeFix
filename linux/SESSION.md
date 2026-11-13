# Linux typing session state

The Linux core now includes a backend-neutral `TypingSession` state machine. It mirrors the relevant Windows behavior without depending on Win32 APIs:

- Collect text as produced by the active layout and evaluate a word at a space boundary.
- Preserve a pending wrong-layout run so subsequent words can be included.
- Permit a short preceding word to join a longer wrong-layout run.
- Offer the converted text for acceptance with Tab.
- Clear all pending text and suggestions on focus changes, navigation, pointer activity, shortcuts, cancellation, and disable.

## Adapter contract

A concrete desktop adapter must:

1. Obtain text from the active XKB keymap, including the active group, modifier state, and compose/dead-key output. Do not hard-code US key positions.
2. Forward only committed printable text to `TypingSession`; do not log, persist, or transmit it.
3. Forward focus changes, navigation, pointer clicks, shortcuts, and cancellation as reset events.
4. Show suggestions only in supported contexts and fail closed when a secure-input state is exposed.
5. On Tab acceptance, perform the replacement in the focused application and handle Unicode grapheme boundaries correctly. The core currently reports a Unicode-scalar deletion count as a low-level hint; this is not yet a complete injection contract.
6. Keep capture and replacement inside the unprivileged user's graphical session. Never access raw evdev devices, install a root service, or use uinput.

## Current boundary

This state machine is not itself an X11 listener. It does not subscribe to XInput2 events, query an XKB keymap, display a tray icon or popup, or inject replacement text. Those operations require a separately reviewed X11 adapter and desktop integration tests. Do not describe the Linux app as globally correcting text until that adapter exists and has been tested.

X11 clients with access to the same X server do not have strong per-application input isolation. This implementation must not claim reliable password-field detection. Users who require stronger isolation should disable X11 support; Wayland remains planned through IBus input-method integration.
