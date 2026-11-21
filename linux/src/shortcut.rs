//! Parsing of user-configurable shortcut strings such as `Ctrl+Alt+Space`.

use std::fmt;

/// X11 modifier mask bits used when grabbing a shortcut.
pub const SHIFT_MASK: u16 = 1 << 0;
pub const CONTROL_MASK: u16 = 1 << 2;
/// Alt is Mod1 and Super is Mod4 on virtually every X11 keyboard setup.
pub const ALT_MASK: u16 = 1 << 3;
pub const SUPER_MASK: u16 = 1 << 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub super_key: bool,
    /// X11 keysym name of the non-modifier key, e.g. `space`, `F9` or `c`.
    pub key: String,
}

impl Shortcut {
    /// Parses `Modifier+Modifier+Key`. Modifier names are case-insensitive:
    /// `Ctrl`/`Control`, `Alt`, `Shift`, `Super`/`Win`/`Meta`. At least one
    /// modifier is required so ordinary typing is never intercepted.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut shortcut = Shortcut {
            shift: false,
            control: false,
            alt: false,
            super_key: false,
            key: String::new(),
        };
        for part in text.split('+').map(str::trim) {
            if part.is_empty() {
                return Err(format!("invalid shortcut {text:?}: use a form like Ctrl+Alt+Space"));
            }
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => shortcut.control = true,
                "alt" => shortcut.alt = true,
                "shift" => shortcut.shift = true,
                "super" | "win" | "meta" => shortcut.super_key = true,
                _ => {
                    if !shortcut.key.is_empty() {
                        return Err(format!("invalid shortcut {text:?}: more than one non-modifier key"));
                    }
                    shortcut.key = part.to_owned();
                }
            }
        }
        if shortcut.key.is_empty() {
            return Err(format!("invalid shortcut {text:?}: no key given"));
        }
        if shortcut.modifier_mask() == 0 {
            return Err(format!("invalid shortcut {text:?}: include at least one of Ctrl, Alt, Shift, Super"));
        }
        Ok(shortcut)
    }

    pub fn modifier_mask(&self) -> u16 {
        let mut mask = 0;
        if self.shift {
            mask |= SHIFT_MASK;
        }
        if self.control {
            mask |= CONTROL_MASK;
        }
        if self.alt {
            mask |= ALT_MASK;
        }
        if self.super_key {
            mask |= SUPER_MASK;
        }
        mask
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.control {
            write!(f, "Ctrl+")?;
        }
        if self.alt {
            write!(f, "Alt+")?;
        }
        if self.shift {
            write!(f, "Shift+")?;
        }
        if self.super_key {
            write!(f, "Super+")?;
        }
        write!(f, "{}", self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_and_key_case_insensitively() {
        let shortcut = Shortcut::parse("ctrl + ALT+Space").unwrap();
        assert!(shortcut.control && shortcut.alt && !shortcut.shift && !shortcut.super_key);
        assert_eq!(shortcut.key, "Space");
        assert_eq!(shortcut.modifier_mask(), CONTROL_MASK | ALT_MASK);
        assert_eq!(shortcut.to_string(), "Ctrl+Alt+Space");
    }

    #[test]
    fn accepts_super_aliases_and_function_keys() {
        let shortcut = Shortcut::parse("Win+Shift+F9").unwrap();
        assert_eq!(shortcut.modifier_mask(), SUPER_MASK | SHIFT_MASK);
        assert_eq!(shortcut.key, "F9");
    }

    #[test]
    fn rejects_invalid_shortcuts() {
        assert!(Shortcut::parse("").is_err());
        assert!(Shortcut::parse("Ctrl+").is_err());
        assert!(Shortcut::parse("Ctrl+Alt").is_err());
        assert!(Shortcut::parse("Space").is_err());
        assert!(Shortcut::parse("Ctrl+A+B").is_err());
    }
}
