use typefix_core::x11::X11SessionConnection;

fn main() {
    match X11SessionConnection::connect() {
        Ok(session) => {
            println!("X11 connection: available");
            println!(
                "XInput2: {}.{}",
                session.xi2_version.0, session.xi2_version.1
            );
            println!("XKB layouts: {}", session.config.layouts.join(", "));
            println!("XKB groups compiled: {}", session.keymap.layout_count());
            println!("Screen: {}", session.screen_number);
            println!("No key events were captured.");
        }
        Err(error) => {
            eprintln!("X11 preflight failed: {error}");
            std::process::exit(1);
        }
    }
}
