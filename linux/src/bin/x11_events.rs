//! Explicit, foreground-only X11 event metadata smoke test.
//!
//! This diagnostic observes server-wide key metadata only. It never decodes
//! keycodes into text, logs typed content, or injects/replaces input. Because
//! X11 has weak per-application isolation, run it only when intentionally
//! testing the current desktop session and stop it with Ctrl+C.

use typefix_core::x11::X11SessionConnection;

fn main() {
    let session = match X11SessionConnection::connect() {
        Ok(session) => session,
        Err(error) => {
            eprintln!("X11 connection failed: {error}");
            std::process::exit(1);
        }
    };

    println!("X11 event smoke test (metadata only)");
    println!("XI2: {}.{}", session.xi2_version.0, session.xi2_version.1);
    println!("Layouts: {}", session.config.layouts.join(","));
    println!("No key text is decoded or stored. Stop with Ctrl+C.");

    if let Err(error) = session.subscribe_key_events() {
        eprintln!("Could not subscribe to XI2 device key events: {error}");
        std::process::exit(1);
    }

    loop {
        match session.wait_for_key_event() {
            Ok(event) => println!(
                "{} keycode={} device={} source={} modifiers=0x{:x} group={} time={}",
                if event.pressed { "press" } else { "release" },
                event.keycode,
                event.device_id,
                event.source_id,
                event.effective_modifiers,
                event.effective_group,
                event.timestamp,
            ),
            Err(error) => {
                eprintln!("X11 event stream stopped: {error}");
                std::process::exit(1);
            }
        }
    }
}
