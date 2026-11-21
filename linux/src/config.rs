//! Minimal `key = value` configuration file (`~/.config/typefix/config`).
//!
//! ```text
//! # Shortcut that converts the selected text between English and Persian.
//! shortcut = Ctrl+Alt+Space
//! # Also watch typing and suggest conversions as notifications (off by default).
//! suggest = false
//! ```

use std::{env, fs, path::PathBuf};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Config {
    pub shortcut: Option<String>,
    pub suggest: Option<bool>,
}

pub fn config_path() -> Option<PathBuf> {
    let base = match env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("typefix").join("config"))
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut config = Config::default();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("line {}: expected `key = value`", index + 1));
            };
            let value = value.trim().trim_matches('"');
            match key.trim() {
                "shortcut" => config.shortcut = Some(value.to_owned()),
                "suggest" => {
                    config.suggest = Some(match value.to_ascii_lowercase().as_str() {
                        "true" | "yes" | "on" | "1" => true,
                        "false" | "no" | "off" | "0" => false,
                        _ => return Err(format!("line {}: suggest must be true or false", index + 1)),
                    })
                }
                other => return Err(format!("line {}: unknown setting {other:?}", index + 1)),
            }
        }
        Ok(config)
    }

    /// Loads the user's config file; a missing file means defaults.
    pub fn load() -> Result<Self, String> {
        let Some(path) = config_path() else { return Ok(Config::default()) };
        if !path.is_file() {
            return Ok(Config::default());
        }
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_settings_comments_and_quotes() {
        let config = Config::parse("# comment\n\nshortcut = \"Ctrl+Shift+F9\"\nsuggest = yes\n").unwrap();
        assert_eq!(config.shortcut.as_deref(), Some("Ctrl+Shift+F9"));
        assert_eq!(config.suggest, Some(true));
    }

    #[test]
    fn empty_config_is_default() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn rejects_bad_lines() {
        assert!(Config::parse("shortcut").is_err());
        assert!(Config::parse("suggest = maybe").is_err());
        assert!(Config::parse("colour = red").is_err());
    }
}
