use x11rb::{
    connection::Connection,
    protocol::xinput::{ConnectionExt as XInputConnectionExt, EventMask, XIEventMask},
};

/// Xvfb can validate the real XI2 connection and that the server accepts the
/// root-window device-event selection. It cannot prove event delivery without
/// generating keyboard input; this test deliberately does not use XTest,
/// xdotool, or any other input-injection mechanism.
#[test]
fn xvfb_accepts_xi2_device_key_press_release_selection_on_root() {
    let (connection, screen) = x11rb::connect(None).expect("connect to Xvfb");
    let version = connection
        .xinput_xi_query_version(2, 0)
        .expect("send XI2 version request")
        .reply()
        .expect("negotiate XI2 version");
    assert!(
        version.major_version >= 2,
        "XInput2 2.0 is required"
    );

    let root = connection.setup().roots[screen].root;
    let mask = XIEventMask::KEY_PRESS | XIEventMask::KEY_RELEASE;
    connection
        .xinput_xi_select_events(
            root,
            &[EventMask {
                deviceid: 1, // XIAllMasterDevices
                mask: vec![mask],
            }],
        )
        .expect("send XI2 event selection")
        .check()
        .expect("X server must accept root key press/release selection");

    // A successful checked request proves the server accepted the selection;
    // it does not claim that root receives key events from every focus target.
    connection.flush().expect("flush XI2 selection");
}
