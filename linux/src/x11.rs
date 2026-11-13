//! X11 session preflight and XKB keymap translation.
//!
//! This module intentionally does not subscribe to raw/global key events. It
//! establishes an unprivileged connection to the current X server, verifies
//! XI2 availability, and compiles the user's configured XKB RMLVO keymap.
//! Event capture and replacement are separate follow-up work.

use std::process::Command;
use x11rb::{
    connection::Connection,
    protocol::xinput::ConnectionExt as XInputConnectionExt,
    rust_connection::RustConnection,
};
use xkbcommon::xkb;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XkbConfig {
    pub rules: String,
    pub model: String,
    pub layouts: Vec<String>,
    pub variants: Vec<String>,
    pub options: String,
}

impl XkbConfig {
    pub fn from_query_output(output: &str) -> Result<Self, String> {
        let mut fields = std::collections::HashMap::new();
        for line in output.lines() {
            if let Some((key, value)) = line.split_once(':') {
                fields.insert(key.trim().to_owned(), value.trim().to_owned());
            }
        }

        let layouts: Vec<String> = fields
            .get("layout")
            .map(|s| s.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect())
            .unwrap_or_default();
        if layouts.is_empty() {
            return Err("XKB query did not report an active layout".into());
        }

        let variants = fields
            .get("variant")
            .map(|s| s.split(',').map(str::trim).map(str::to_owned).collect())
            .unwrap_or_default();

        Ok(Self {
            rules: fields.get("rules").cloned().unwrap_or_default(),
            model: fields.get("model").cloned().unwrap_or_default(),
            layouts,
            variants,
            options: fields.get("options").cloned().unwrap_or_default(),
        })
    }

    pub fn from_current_session() -> Result<Self, String> {
        let output = Command::new("setxkbmap")
            .arg("-query")
            .output()
            .map_err(|e| format!("could not run setxkbmap -query: {e}"))?;
        if !output.status.success() {
            return Err("setxkbmap -query failed".into());
        }
        let text = String::from_utf8(output.stdout)
            .map_err(|_| "setxkbmap returned non-UTF-8 output".to_owned())?;
        Self::from_query_output(&text)
    }

    pub fn compile(&self) -> Result<XkbKeymap, String> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let layout = self.layouts.join(",");
        let variant = self.variants.join(",");
        let keymap = xkb::Keymap::new_from_names(
            &context,
            &self.rules,
            &self.model,
            &layout,
            &variant,
            Some(self.options.clone()),
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .ok_or_else(|| "libxkbcommon could not compile the active XKB layout".to_owned())?;
        Ok(XkbKeymap { keymap })
    }
}

pub struct XkbKeymap {
    keymap: xkb::Keymap,
}

impl XkbKeymap {
    /// Translate an X11 keycode using the requested XKB layout group and
    /// modifier mask. X11 keycodes are already in the XKB keycode namespace;
    /// do not add the evdev +8 offset a second time.
    ///
    /// The caller must supply the current group and modifier state from XKB.
    /// This function does not infer group changes or modifier state itself.
    pub fn translate_keycode(
        &self,
        x11_keycode: u8,
        group: u32,
        modifiers: u32,
    ) -> Option<String> {
        let mut state = xkb::State::new(&self.keymap);
        state.update_mask(modifiers, 0, 0, group, 0, 0);
        let keycode = xkb::Keycode::new(u32::from(x11_keycode));
        let text = state.key_get_utf8(keycode);
        (!text.is_empty() && !text.chars().any(char::is_control)).then_some(text)
    }

    pub fn layout_count(&self) -> u32 {
        self.keymap.num_layouts()
    }
}

pub struct X11SessionConnection {
    _connection: RustConnection,
    pub screen_number: usize,
    pub xi2_version: (u16, u16),
    pub config: XkbConfig,
    pub keymap: XkbKeymap,
}

impl X11SessionConnection {
    /// Connects as the current desktop user. No root privileges or device nodes
    /// are accessed. This is a capability/keymap preflight, not a key listener.
    pub fn connect() -> Result<Self, String> {
        let (connection, screen_number) =
            x11rb::connect(None).map_err(|e| format!("cannot connect to X11 display: {e}"))?;
        let version = connection
            .xinput_xi_query_version(2, 0)
            .map_err(|e| format!("XInput2 is unavailable: {e}"))?
            .reply()
            .map_err(|e| format!("XInput2 version negotiation failed: {e}"))?;
        if version.major_version < 2 {
            return Err("XInput2 version 2.0 or newer is required".into());
        }

        let config = XkbConfig::from_current_session()?;
        let keymap = config.compile()?;
        Ok(Self {
            _connection: connection,
            screen_number,
            xi2_version: (version.major_version, version.minor_version),
            config,
            keymap,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_layouts_and_options() {
        let config = XkbConfig::from_query_output(
            "rules: evdev\nmodel: pc105\nlayout: us,ir\nvariant: ,\noptions: grp:alt_shift_toggle\n",
        )
        .unwrap();
        assert_eq!(config.layouts, vec!["us", "ir"]);
        assert_eq!(config.options, "grp:alt_shift_toggle");
        assert_eq!(config.variants, vec!["", ""]);
    }

    #[test]
    fn rejects_missing_layout_instead_of_guessing_us() {
        assert!(XkbConfig::from_query_output("rules: evdev\nmodel: pc105\n").is_err());
    }

    #[test]
    fn compiles_and_translates_us_and_persian_groups() {
        let config = XkbConfig {
            rules: "evdev".into(),
            model: "pc105".into(),
            layouts: vec!["us".into(), "ir".into()],
            variants: vec!["".into(), "".into()],
            options: String::new(),
        };
        let keymap = config.compile().expect("xkb-data and libxkbcommon installed");
        // X11 keycode 38 is the physical A key. Its output changes by group.
        assert_eq!(keymap.translate_keycode(38, 0, 0).as_deref(), Some("a"));
        assert_eq!(keymap.translate_keycode(38, 1, 0).as_deref(), Some("ش"));
    }
}
