//! Minimal `key = value` configuration file (`~/.config/typefix/config`).
//!
//! ```text
//! # Shortcut that converts the selected text between English and Persian.
//! shortcut = Ctrl+Alt+Space
//! # Also watch typing and suggest conversions as notifications (off by default).
//! suggest = false
//! # Settings written by the UI:
//! theme = system      # system, light or dark
//! language = en       # UI language: en or fa
//! source = en         # direction for suggestions: en or fa
//! ```

use std::{env, fs, path::PathBuf};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Config {
    pub shortcut: Option<String>,
    pub suggest: Option<bool>,
    pub theme: Option<String>,
    pub language: Option<String>,
    pub source: Option<String>,
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
                "theme" => match value {
                    "system" | "light" | "dark" => config.theme = Some(value.to_owned()),
                    _ => return Err(format!("line {}: theme must be system, light or dark", index + 1)),
                },
                "language" => match value {
                    "en" | "fa" => config.language = Some(value.to_owned()),
                    _ => return Err(format!("line {}: language must be en or fa", index + 1)),
                },
                "source" => match value {
                    "en" | "fa" => config.source = Some(value.to_owned()),
                    _ => return Err(format!("line {}: source must be en or fa", index + 1)),
                },
                other => return Err(format!("line {}: unknown setting {other:?}", index + 1)),
            }
        }
        Ok(config)
    }

    /// Serialises the settings that are set, in the file format.
    pub fn to_text(&self) -> String {
        let mut text = String::from("# TypeFix settings (edited by the TypeFix window or by hand)\n");
        if let Some(value) = &self.shortcut {
            text.push_str(&format!("shortcut = {value}\n"));
        }
        if let Some(value) = self.suggest {
            text.push_str(&format!("suggest = {value}\n"));
        }
        if let Some(value) = &self.theme {
            text.push_str(&format!("theme = {value}\n"));
        }
        if let Some(value) = &self.language {
            text.push_str(&format!("language = {value}\n"));
        }
        if let Some(value) = &self.source {
            text.push_str(&format!("source = {value}\n"));
        }
        text
    }

    /// Writes the config file, creating its directory if needed.
    pub fn save(&self) -> Result<(), String> {
        let path = config_path().ok_or("cannot determine the config directory")?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        fs::write(&path, self.to_text()).map_err(|e| format!("cannot write {}: {e}", path.display()))
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
    fn round_trips_through_text() {
        let config = Config {
            shortcut: Some("Ctrl+Alt+K".into()),
            suggest: Some(false),
            theme: Some("dark".into()),
            language: Some("fa".into()),
            source: Some("en".into()),
        };
        assert_eq!(Config::parse(&config.to_text()).unwrap(), config);
    }

    #[test]
    fn rejects_unknown_theme_and_language() {
        assert!(Config::parse("theme = purple").is_err());
        assert!(Config::parse("language = de").is_err());
        assert!(Config::parse("source = xx").is_err());
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
