use std::{process::Command, thread, time::{Duration, Instant}};
use x11rb::{
    connection::Connection,
    protocol::{
        xinput::{ConnectionExt as XInputConnectionExt, EventMask, XIEventMask},
        xkb::{ConnectionExt as XkbConnectionExt, Group, ID},
        Event,
    },
};

fn run_xdotool(args: &[&str]) {
    let status = Command::new("xdotool")
        .args(args)
        .status()
        .expect("xdotool installed");
    assert!(status.success(), "xdotool command failed: {args:?}");
}

#[test]
fn xvfb_delivers_xi2_device_key_events_and_tracks_layout_group_switch() {
    // Configure two actual XKB groups on the isolated virtual X server.
    let configure = Command::new("setxkbmap")
        .args(["-layout", "us,ir", "-verbose", "10"])
        .output()
        .expect("setxkbmap installed");
    assert!(
        configure.status.success(),
        "could not configure us,ir XKB groups: {}",
        String::from_utf8_lossy(&configure.stderr)
    );

    let (connection, screen) = x11rb::connect(None).expect("connect to Xvfb");
    let version = connection.xinput_xi_query_version(2, 0).unwrap().reply().unwrap();
    assert!(version.major_version >= 2, "XInput2 2.0 is required");
    let xkb_version = connection.xkb_use_extension(1, 0).unwrap().reply().unwrap();
    assert!(xkb_version.supported, "XKB extension is required");

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

    // Lock the server's core keyboard into group 1 through the XKB protocol.
    // This is deterministic test setup, not a production layout-switch feature.
    connection.xkb_latch_lock_state(
        ID::USE_CORE_KBD,
        0,
        0,
        true,
        Group::from(1),
        0,
        false,
        0,
    ).unwrap().check().unwrap();
    connection.flush().unwrap();
    run_xdotool(&["key", "a"]);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut press_groups = Vec::new();
    let mut release_groups = Vec::new();
    while Instant::now() < deadline && !(press_groups.contains(&1) && release_groups.contains(&1)) {
        if let Some(event) = connection.poll_for_event().unwrap() {
            match event {
                Event::XinputKeyPress(e) if e.detail == 38 => press_groups.push(e.group.effective),
                Event::XinputKeyRelease(e) if e.detail == 38 => release_groups.push(e.group.effective),
                _ => {}
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }
    assert!(press_groups.contains(&1), "XI2 did not report Persian group 1 on key press; observed groups: {press_groups:?}");
    assert!(release_groups.contains(&1), "XI2 did not report Persian group 1 on key release; observed groups: {release_groups:?}");

    // Compile the same US/Persian RMLVO configuration and verify keycode 38
    // resolves differently in the two groups.
    let config = typefix_core::x11::XkbConfig {
        rules: "evdev".into(),
        model: "pc105".into(),
        layouts: vec!["us".into(), "ir".into()],
        variants: vec!["".into(), "".into()],
        options: String::new(),
    };
    let keymap = config.compile().expect("compile US/Persian XKB keymap");
    assert_eq!(keymap.translate_keycode(38, 0, 0).as_deref(), Some("a"));
    assert_eq!(keymap.translate_keycode(38, 1, 0).as_deref(), Some("ش"));
}
