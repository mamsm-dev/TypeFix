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

#[test]
fn xvfb_delivers_xi2_device_key_events_and_tracks_layout_group_switch() {
    // Configure two real XKB groups and a deterministic group-toggle shortcut.
    let configure = Command::new("setxkbmap")
        .args(["-layout", "us,ir", "-option", "grp:alt_shift_toggle", "-verbose", "10"])
        .output()
        .expect("setxkbmap installed");
    assert!(
        configure.status.success(),
        "could not configure us,ir XKB groups: {}",
        String::from_utf8_lossy(&configure.stderr)
    );
    eprintln!("setxkbmap configuration:\n{}", String::from_utf8_lossy(&configure.stdout));
    eprintln!("setxkbmap diagnostics:\n{}", String::from_utf8_lossy(&configure.stderr));

    let query = Command::new("setxkbmap").arg("-query").output().expect("query XKB");
    assert!(query.status.success());
    let query_text = String::from_utf8_lossy(&query.stdout);
    assert!(query_text.contains("layout:") && query_text.contains("us,ir"), "unexpected XKB layout: {query_text}");

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

    // Send the configured two-key chord as one XTest chord. This avoids
    // inter-process timing gaps between individual keydown/keyup commands.
    run_xdotool(&["key", "--clearmodifiers", "Alt_L+Shift_L"]);
    run_xdotool(&["key", "a"]);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut press_groups = Vec::new();
    let mut release_groups = Vec::new();
    while Instant::now() < deadline && !(press_groups.contains(&1) && release_groups.contains(&1)) {
        if let Some(event) = connection.poll_for_event().unwrap() {
            match event {
                Event::XinputKeyPress(e) if e.detail == 38 => {
                    press_groups.push(e.group.effective);
                }
                Event::XinputKeyRelease(e) if e.detail == 38 => {
                    release_groups.push(e.group.effective);
                }
                _ => {}
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }
    assert!(press_groups.contains(&1), "XI2 did not report Persian group 1 on key press; observed groups: {press_groups:?}");
    assert!(release_groups.contains(&1), "XI2 did not report Persian group 1 on key release; observed groups: {release_groups:?}");

    // Compile the active XKB configuration and verify the same physical key
    // translates differently in group 1. No typed text is captured or passed
    // into the application by this metadata-level test.
    let config = typefix_core::x11::XkbConfig::from_current_session()
        .expect("read active XKB configuration");
    let keymap = config.compile().expect("compile active XKB keymap");
    assert_eq!(keymap.translate_keycode(38, 0, 0).as_deref(), Some("a"));
    assert_eq!(keymap.translate_keycode(38, 1, 0).as_deref(), Some("ش"));
}
