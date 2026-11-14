//! In-memory XKB Compose/dead-key state.
//!
//! This module only turns keysyms into committed UTF-8 sequences. It does not
//! observe input, retain typed history, log text, or inject replacement text.
//! The caller owns lifecycle resets (focus changes, cancellation, keymap changes).

use std::ffi::OsStr;

use xkbcommon::xkb;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeOutcome {
    /// No Compose sequence is active; the caller may translate this keysym normally.
    Passthrough,
    /// A Compose/dead-key sequence is incomplete; do not forward partial text.
    Composing,
    /// A complete Compose sequence produced committed UTF-8 text.
    Committed(String),
    /// The current sequence was cancelled or had no valid committed result.
    Cancelled,
}

pub struct ComposeProcessor {
    // Keep the context alive for at least as long as the compose state.
    _context: xkb::Context,
    state: xkb::compose::State,
}

impl ComposeProcessor {
    /// Build a Compose table using the user's active locale and Compose files.
    pub fn new(locale: &OsStr) -> Result<Self, String> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let table = xkb::compose::Table::new_from_locale(
            &context,
            locale,
            xkb::compose::COMPILE_NO_FLAGS,
        )
        .map_err(|_| format!("could not load an XKB Compose table for locale {}", locale.to_string_lossy()))?;
        let state = xkb::compose::State::new(&table, xkb::compose::STATE_NO_FLAGS);
        Ok(Self { _context: context, state })
    }

    /// Feed one key symbol from a key-press event.
    ///
    /// When no sequence is active, an ignored keysym is returned as
    /// Passthrough so the caller can use ordinary XKB text translation.
    /// While composing, ignored modifier keysyms do not leak partial text.
    pub fn feed(&mut self, keysym: xkb::Keysym) -> ComposeOutcome {
        let result = self.state.feed(keysym);
        let status = self.state.status();

        if result == xkb::compose::FeedResult::Ignored {
            return if status == xkb::compose::Status::Composing {
                ComposeOutcome::Composing
            } else {
                ComposeOutcome::Passthrough
            };
        }

        match status {
            xkb::compose::Status::Nothing => ComposeOutcome::Passthrough,
            xkb::compose::Status::Composing => ComposeOutcome::Composing,
            xkb::compose::Status::Composed => {
                let committed = self.state.utf8();
                self.state.reset();
                match committed {
                    Some(text) if !text.is_empty() && !text.chars().any(char::is_control) => {
                        ComposeOutcome::Committed(text)
                    }
                    _ => ComposeOutcome::Cancelled,
                }
            }
            xkb::compose::Status::Cancelled => {
                self.state.reset();
                ComposeOutcome::Cancelled
            }
        }
    }

    /// Discard any partial sequence. Call on focus changes, navigation,
    /// cancellation, disable, and keymap/locale replacement.
    pub fn reset(&mut self) {
        self.state.reset();
    }

    pub fn is_composing(&self) -> bool {
        self.state.status() == xkb::compose::Status::Composing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_COMPOSE: &str = r#"
<dead_acute> <e> : "é" eacute
<Multi_key> <o> <c> : "©" copyright
"#;

    fn processor() -> ComposeProcessor {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let table = xkb::compose::Table::new_from_buffer(
            &context,
            TEST_COMPOSE,
            "en_US.UTF-8",
            xkb::compose::FORMAT_TEXT_V1,
            xkb::compose::COMPILE_NO_FLAGS,
        )
        .expect("test Compose table compiles");
        let state = xkb::compose::State::new(&table, xkb::compose::STATE_NO_FLAGS);
        ComposeProcessor { _context: context, state }
    }

    fn keysym(name: &str) -> xkb::Keysym {
        xkb::keysym_from_name(name, xkb::KEYSYM_NO_FLAGS)
    }

    #[test]
    fn composes_dead_acute_and_letter_without_emitting_partial_text() {
        let mut compose = processor();
        assert_eq!(compose.feed(keysym("dead_acute")), ComposeOutcome::Composing);
        assert!(compose.is_composing());
        assert_eq!(compose.feed(keysym("e")), ComposeOutcome::Committed("é".into()));
        assert!(!compose.is_composing());
    }

    #[test]
    fn composes_multi_key_sequence() {
        let mut compose = processor();
        assert_eq!(compose.feed(keysym("Multi_key")), ComposeOutcome::Composing);
        assert_eq!(compose.feed(keysym("o")), ComposeOutcome::Composing);
        assert_eq!(compose.feed(keysym("c")), ComposeOutcome::Committed("©".into()));
    }

    #[test]
    fn reset_discards_partial_sequence_and_restores_passthrough() {
        let mut compose = processor();
        assert_eq!(compose.feed(keysym("dead_acute")), ComposeOutcome::Composing);
        compose.reset();
        assert!(!compose.is_composing());
        assert_eq!(compose.feed(keysym("e")), ComposeOutcome::Passthrough);
    }
}
