use std::{process::Command, thread, time::{Duration, Instant}};
use x11rb::{
    connection::Connection,
    protocol::{xinput::{ConnectionExt as XInputConnectionExt, EventMask, XIEventMask}, Event},
};

#[test]
fn xvfb_delivers_xi2_device_key_events_with_modifier_snapshot() {
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

    // XTest is used only by this isolated CI test to synthesize a deterministic
    // Shift+A keystroke. The production application does not inject input.
    thread::sleep(Duration::from_millis(150));
    for args in [
        vec!["keydown", "Shift_L"],
        vec!["keydown", "a"],
        vec!["keyup", "a"],
        vec!["keyup", "Shift_L"],
    ] {
        let status = Command::new("xdotool").args(args).status().expect("xdotool installed");
        assert!(status.success(), "xdotool command failed");
    }

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

    assert!(saw_shifted_a_press, "XI2 did not deliver Shift+A press with effective Shift modifier and group 0");
    assert!(saw_a_release, "XI2 did not deliver A release with group snapshot");
}
