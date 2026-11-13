use std::{process::Command, thread, time::{Duration, Instant}};
use x11rb::{
    connection::Connection,
    protocol::{xinput::{ConnectionExt as XInputConnectionExt, EventMask, XIEventMask}, Event},
};

fn run_xdotool(args: &[&str]) {
    let status = Command::new("xdotool")
        .args(args)
        .status()
        .expect("xdotool installed");
    assert!(status.success(), "xdotool command failed: {args:?}");
}

fn wait_for_events(
    connection: &impl Connection,
    deadline: Instant,
    predicate: impl Fn(&Event) -> bool,
) -> bool {
    while Instant::now() < deadline {
        if let Some(event) = connection.poll_for_event().unwrap() {
            if predicate(&event) {
                return true;
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }
    false
}

#[test]
fn xvfb_delivers_xi2_device_key_events_and_tracks_layout_group_switch() {
    // Configure two real XKB groups and a deterministic group-toggle shortcut.
    let status = Command::new("setxkbmap")
        .args(["-layout", "us,ir", "-option", "grp:alt_shift_toggle"])
        .status()
        .expect("setxkbmap installed");
    assert!(status.success(), "could not configure us,ir XKB groups");

    let (connection, screen) = x11rb::connect(None).expect("connect to Xvfb");
    let version = connection.xinput_xi_query_version(2, 0).unwrap().reply().unwrap();
    assert!(version.major_version >= 2, "XInput2 2.0 is required");

    let root = connection.setup().roots[screen].root;
    let mask = XIEventMask::KEY_PRESS | XIEventMask::KEY_RELEASE;
    connection.xinput_xi_select_events(
        root,
        &[EventMask { deviceid: 1, mask: vec![mask] }],
    ).unwrap().check().unwrap();
    connection.flush().unwrap();

    // XTest is used only by this isolated CI test to synthesize keystrokes.
    // The production application does not inject input.
    thread::sleep(Duration::from_millis(150));
    run_xdotool(&["keydown", "Shift_L"]);
    run_xdotool(&["keydown", "a"]);
    run_xdotool(&["keyup", "a"]);
    run_xdotool(&["keyup", "Shift_L"]);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut saw_shifted_a_press = false;
    let mut saw_a_release = false;
    while Instant::now() < deadline && !(saw_shifted_a_press && saw_a_release) {
        if let Some(event) = connection.poll_for_event().unwrap() {
            match event {
                Event::XinputKeyPress(e) if e.detail == 38 => {
                    saw_shifted_a_press |= e.mods.effective & 1 != 0 && e.group.effective == 0;
                }
                Event::XinputKeyRelease(e) if e.detail == 38 => {
                    saw_a_release |= e.group.effective == 0;
                }
                _ => {}
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }
    assert!(saw_shifted_a_press, "XI2 did not deliver Shift+A with effective Shift modifier and group 0");
    assert!(saw_a_release, "XI2 did not deliver A release with group 0");

    // Toggle from US (group 0) to Persian (group 1), then send keycode 38
    // directly so the test does not depend on xdotool resolving a US keysym
    // while the Persian group is active.
    run_xdotool(&["keydown", "Alt_L"]);
    run_xdotool(&["keydown", "Shift_L"]);
    run_xdotool(&["keyup", "Shift_L"]);
    run_xdotool(&["keyup", "Alt_L"]);
    run_xdotool(&["keycode", "38"]);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut saw_group_one_press = false;
    let mut saw_group_one_release = false;
    while Instant::now() < deadline && !(saw_group_one_press && saw_group_one_release) {
        if let Some(event) = connection.poll_for_event().unwrap() {
            match event {
                Event::XinputKeyPress(e) if e.detail == 38 => {
                    saw_group_one_press |= e.group.effective == 1;
                }
                Event::XinputKeyRelease(e) if e.detail == 38 => {
                    saw_group_one_release |= e.group.effective == 1;
                }
                _ => {}
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }
    assert!(saw_group_one_press, "XI2 did not report active Persian group 1 on key press");
    assert!(saw_group_one_release, "XI2 did not report active Persian group 1 on key release");

    // Compile the active XKB configuration and verify the same physical key
    // translates differently in group 1. This is still metadata-level testing;
    // no typed text is captured or passed into the application.
    let config = typefix_core::x11::XkbConfig::from_current_session()
        .expect("read active XKB configuration");
    let keymap = config.compile().expect("compile active XKB keymap");
    assert_eq!(keymap.translate_keycode(38, 0, 0).as_deref(), Some("a"));
    assert_eq!(keymap.translate_keycode(38, 1, 0).as_deref(), Some("ش"));
}
