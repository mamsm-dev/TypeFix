use std::{
    io::Write,
    process::{Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

// These integration tests share one Xvfb server and mutate its global XKB/focus state.
static X11_TEST_LOCK: Mutex<()> = Mutex::new(());

use x11rb::{
    connection::Connection,
    protocol::{
        xinput::{ConnectionExt as XInputConnectionExt, EventMask, XIEventMask},
        xkb::{ConnectionExt as XkbConnectionExt, Group},
        xtest::ConnectionExt as XTestConnectionExt,
        xproto::{ConnectionExt as XprotoConnectionExt, CreateWindowAux, InputFocus, WindowClass},
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
    let _guard = X11_TEST_LOCK.lock().unwrap();
    // Generate a two-group map. It is loaded onto the XI2 source keyboard
    // after the first event reveals the actual device ID used by Xvfb.
    let map = Command::new("setxkbmap")
        .args(["-layout", "us,ir", "-option", "", "-print"])
        .output()
        .expect("setxkbmap installed");
    assert!(
        map.status.success(),
        "could not compile us,ir XKB map: {}",
        String::from_utf8_lossy(&map.stderr)
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
    let mut source_keyboard_id = None;
    let mut master_keyboard_id = None;
    while Instant::now() < deadline && !(saw_shifted_a_press && saw_a_release) {
        if let Some(event) = connection.poll_for_event().unwrap() {
            match event {
                Event::XinputKeyPress(e) if e.detail == 38 => {
                    source_keyboard_id = Some(e.sourceid);
                    master_keyboard_id = Some(e.deviceid);
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

    let source_keyboard_id = source_keyboard_id.expect("XI2 key event exposed source keyboard ID");
    let master_keyboard_id = master_keyboard_id.expect("XI2 key event exposed master keyboard ID");
    assert_ne!(source_keyboard_id, master_keyboard_id, "XI2 source and master devices should be distinct in this test");
    let display = std::env::var("DISPLAY").expect("Xvfb DISPLAY is set");
    let mut loader = Command::new("xkbcomp")
        .args(["-w", "0", "-i", &master_keyboard_id.to_string(), "-", &display])
        .stdin(Stdio::piped())
        .spawn()
        .expect("xkbcomp installed");
    loader.stdin.take().expect("xkbcomp stdin").write_all(&map.stdout).unwrap();
    let loaded = loader.wait().expect("wait for xkbcomp");
    assert!(loaded.success(), "xkbcomp could not load the two-group map on XI2 master keyboard");

    // XI2 reports effective group state for the master keyboard (deviceid),
    // while sourceid identifies the physical/virtual slave that generated input.
    // Configure and lock the master device whose group snapshot is in XI2 events.
    let state_before = connection.xkb_get_state(u16::from(master_keyboard_id)).unwrap().reply().unwrap();
    eprintln!("XKB group before lock on XI2 master device {} (source {}): {:?}", master_keyboard_id, source_keyboard_id, state_before.group);
    connection.xkb_latch_lock_state(
        u16::from(master_keyboard_id),
        x11rb::protocol::xproto::ModMask::from(0u16),
        x11rb::protocol::xproto::ModMask::from(0u16),
        true,
        Group::from(1),
        x11rb::protocol::xproto::ModMask::from(0u16),
        false,
        0u16,
    ).unwrap().check().unwrap();
    connection.flush().unwrap();
    let state_after = connection.xkb_get_state(u16::from(master_keyboard_id)).unwrap().reply().unwrap();
    eprintln!("XKB group after lock on XI2 master device {}: {:?}", master_keyboard_id, state_after.group);
    assert_eq!(state_after.group, Group::from(1), "XKB master keyboard did not lock to group 1");

    // Use the XTEST 2.2 device-aware request so the synthetic key is sent to
    // the exact XI2 source keyboard whose XKB group was locked above.
    let xtest_version = connection.xtest_get_version(2, 2).unwrap().reply().unwrap();
    assert!(
        xtest_version.major_version > 2
            || (xtest_version.major_version == 2 && xtest_version.minor_version >= 2),
        "XTEST 2.2 device-aware input is required"
    );
    let xtest_device_id = u8::try_from(source_keyboard_id).expect("XTEST device IDs are represented as u8");
    connection.xtest_fake_input(2, 38, 0, root, 0, 0, xtest_device_id).unwrap().check().unwrap();
    connection.xtest_fake_input(3, 38, 0, root, 0, 0, xtest_device_id).unwrap().check().unwrap();
    connection.flush().unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut press_groups = Vec::new();
    let mut release_groups = Vec::new();
    while Instant::now() < deadline && !(press_groups.contains(&1) && release_groups.contains(&1)) {
        if let Some(event) = connection.poll_for_event().unwrap() {
            match event {
                Event::XinputKeyPress(e) if e.detail == 38 => {
                    eprintln!("post-switch XI2 press: device={} source={} group base={} latched={} locked={} effective={}",
                        e.deviceid, e.sourceid, e.group.base, e.group.latched, e.group.locked, e.group.effective);
                    press_groups.push(e.group.effective);
                }
                Event::XinputKeyRelease(e) if e.detail == 38 => {
                    eprintln!("post-switch XI2 release: device={} source={} group base={} latched={} locked={} effective={}",
                        e.deviceid, e.sourceid, e.group.base, e.group.latched, e.group.locked, e.group.effective);
                    release_groups.push(e.group.effective);
                }
                _ => {}
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }
    let final_state = connection.xkb_get_state(u16::from(master_keyboard_id)).unwrap().reply().unwrap();
    eprintln!("XKB group after synthetic key on XI2 master device {}: {:?}", master_keyboard_id, final_state.group);
    assert_eq!(final_state.group, Group::from(1), "synthetic input unexpectedly changed the master keyboard group");
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

#[test]
fn xvfb_xi2_focus_selection_on_root_observes_child_focus_transitions() {
    let _guard = X11_TEST_LOCK.lock().unwrap();
    let (connection, screen) = x11rb::connect(None).expect("connect to Xvfb");
    let version = connection.xinput_xi_query_version(2, 0).unwrap().reply().unwrap();
    assert!(version.major_version >= 2, "XInput2 2.0 is required");

    let root = connection.setup().roots[screen].root;
    // XI2 FocusIn and FocusOut are event numbers 9 and 10 in XI2.
    let focus_mask = XIEventMask::from((1u32 << 9) | (1u32 << 10));
    connection.xinput_xi_select_events(
        root,
        &[EventMask { deviceid: 1, mask: vec![focus_mask] }],
    ).unwrap().check().unwrap();

    let child = connection.generate_id().unwrap();
    connection.create_window(
        0,
        child,
        root,
        0,
        0,
        120,
        80,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new(),
    ).unwrap().check().unwrap();
    connection.map_window(child).unwrap().check().unwrap();
    connection.flush().unwrap();

    connection.set_input_focus(InputFocus::PARENT, child, x11rb::CURRENT_TIME).unwrap().check().unwrap();
    connection.flush().unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut saw_root_focus_event = false;
    let mut observed = Vec::new();
    while Instant::now() < deadline && !saw_root_focus_event {
        if let Some(event) = connection.poll_for_event().unwrap() {
            match event {
                Event::XinputFocusIn(e) => {
                    observed.push(("in", e.event, e.detail));
                    saw_root_focus_event |= e.event == root;
                }
                Event::XinputFocusOut(e) => {
                    observed.push(("out", e.event, e.detail));
                    saw_root_focus_event |= e.event == root;
                },
                _ => {}
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }

    connection.destroy_window(child).unwrap().check().unwrap();
    connection.flush().unwrap();

    assert!(
        saw_root_focus_event,
        "selecting XI2 FocusIn/FocusOut on root did not report a child focus transition; observed: {observed:?}"
    );
}

#[test]
fn x11_session_api_polls_root_propagated_focus_transitions() {
    let _guard = X11_TEST_LOCK.lock().unwrap();
    use typefix_core::x11::{X11SessionConnection, X11SessionEvent};

    let layout = Command::new("setxkbmap")
        .args(["-layout", "us,ir"])
        .status()
        .expect("setxkbmap installed");
    assert!(layout.success(), "could not set deterministic XKB layout");

    let session = X11SessionConnection::connect().expect("connect X11 session API");
    session.subscribe_session_events().expect("subscribe to XI2 key and focus events");

    let (client, screen) = x11rb::connect(None).expect("connect second X11 client");
    let root = client.setup().roots[screen].root;
    let child = client.generate_id().unwrap();
    client.create_window(
        0,
        child,
        root,
        0,
        0,
        100,
        60,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new(),
    ).unwrap().check().unwrap();
    client.map_window(child).unwrap().check().unwrap();
    client.set_input_focus(InputFocus::PARENT, child, x11rb::CURRENT_TIME).unwrap().check().unwrap();
    client.flush().unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut observed_focus = false;
    while Instant::now() < deadline && !observed_focus {
        if let Some(event) = session.poll_for_session_event().expect("poll session events") {
            if let X11SessionEvent::Focus(focus) = event {
                observed_focus = focus.event_window == root;
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }

    client.destroy_window(child).unwrap().check().unwrap();
    client.flush().unwrap();

    assert!(observed_focus, "session API did not expose a root-propagated XI2 focus event");
}
