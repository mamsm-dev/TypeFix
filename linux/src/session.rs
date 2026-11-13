//! Backend-neutral typing session state used by the X11 event adapter.
//!
//! This module deliberately does not open an X11 connection or subscribe to global
//! events. The platform adapter is responsible for obtaining text from XKB and
//! forwarding focus, navigation, and key events here. Keeping the state machine
//! independent makes its privacy and reset behavior testable without a desktop.

use crate::{Dictionaries, Language, LayoutMap, Suggestion};

const MAX_RUN_WORDS: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    /// Text produced by one key or compose sequence under the active layout.
    Text(String),
    Space,
    Backspace,
    /// Tab accepts the current suggestion, if any.
    Tab,
    /// Focus changes, navigation, pointer clicks, shortcuts, and cancellation
    /// all terminate the current observation window.
    Reset,
    FocusChanged,
    Navigation,
    PointerPressed,
    Shortcut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionAction {
    None,
    Suggestion(Suggestion),
    /// The adapter must replace the trailing typed run with this text.
    /// delete_chars counts Unicode scalar values, not grapheme clusters; a
    /// concrete X11 injector must use XKB/text-input semantics carefully.
    Apply { text: String, delete_chars: usize },
}

#[derive(Default)]
pub struct TypingSession {
    enabled: bool,
    word: String,
    previous: String,
    run: String,
    run_words: usize,
    suggestion: Option<Suggestion>,
}

impl TypingSession {
    pub fn new() -> Self {
        Self { enabled: true, ..Self::default() }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.clear();
        }
    }

    pub fn reset(&mut self) {
        self.clear();
    }

    pub fn suggestion(&self) -> Option<&Suggestion> {
        self.suggestion.as_ref()
    }

    pub fn handle(
        &mut self,
        event: SessionEvent,
        dicts: &Dictionaries,
        source: Language,
        map: &LayoutMap,
    ) -> SessionAction {
        if !self.enabled {
            return SessionAction::None;
        }

        match event {
            SessionEvent::Reset
            | SessionEvent::FocusChanged
            | SessionEvent::Navigation
            | SessionEvent::PointerPressed
            | SessionEvent::Shortcut => {
                self.clear();
                SessionAction::None
            }
            SessionEvent::Text(text) => {
                if text.is_empty() || text.chars().any(char::is_control) {
                    self.clear();
                    return SessionAction::None;
                }
                self.word.push_str(&text);
                if !self.run.is_empty() {
                    self.run.push_str(&text);
                    self.refresh(dicts, source, map)
                } else {
                    SessionAction::None
                }
            }
            SessionEvent::Backspace => {
                self.word.pop();
                if !self.run.is_empty() {
                    self.run.pop();
                    if self.run.is_empty() {
                        self.clear();
                        SessionAction::None
                    } else {
                        self.refresh(dicts, source, map)
                    }
                } else {
                    SessionAction::None
                }
            }
            SessionEvent::Space => self.finish_word(dicts, source, map),
            SessionEvent::Tab => {
                if let Some(suggestion) = self.suggestion.take() {
                    let action = SessionAction::Apply {
                        text: suggestion.converted,
                        delete_chars: self.run.chars().count(),
                    };
                    self.clear();
                    action
                } else {
                    self.clear();
                    SessionAction::None
                }
            }
        }
    }

    fn finish_word(
        &mut self,
        dicts: &Dictionaries,
        source: Language,
        map: &LayoutMap,
    ) -> SessionAction {
        if self.word.is_empty() {
            if !self.run.is_empty() {
                self.run.push(' ');
                self.run_words += 1;
                if self.run_words > MAX_RUN_WORDS {
                    self.clear();
                    return SessionAction::None;
                }
                return self.refresh(dicts, source, map);
            }
            return SessionAction::None;
        }

        let finished = std::mem::take(&mut self.word);
        if !self.run.is_empty() {
            self.run.push_str(&finished);
            self.run.push(' ');
            self.run_words += 1;
            self.previous = finished;
            if self.run_words > MAX_RUN_WORDS {
                self.clear();
                return SessionAction::None;
            }
            return self.refresh(dicts, source, map);
        }

        let converted = map.convert(&finished);
        if dicts.is_mistake_with_min(&finished, source, &converted, 3) {
            let previous_converted = map.convert(&self.previous);
            let prefix = if !self.previous.is_empty()
                && dicts.is_mistake_with_min(&self.previous, source, &previous_converted, 1)
            {
                format!("{} ", self.previous)
            } else {
                String::new()
            };
            self.run = format!("{prefix}{finished} ");
            self.run_words = 1;
            self.previous = finished;
            self.refresh(dicts, source, map)
        } else {
            self.previous = finished;
            SessionAction::None
        }
    }

    fn refresh(
        &mut self,
        dicts: &Dictionaries,
        source: Language,
        map: &LayoutMap,
    ) -> SessionAction {
        let typed = self.run.trim_end();
        let converted = map.convert(typed);
        if dicts.is_mistake_with_min(typed, source, &converted, 3) {
            let suggestion = Suggestion {
                original: typed.to_owned(),
                converted,
                source,
            };
            self.suggestion = Some(suggestion.clone());
            SessionAction::Suggestion(suggestion)
        } else {
            self.suggestion = None;
            SessionAction::None
        }
    }

    fn clear(&mut self) {
        self.word.clear();
        self.previous.clear();
        self.run.clear();
        self.run_words = 0;
        self.suggestion = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};

    fn fixture() -> (Dictionaries, LayoutMap) {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("typefix-session-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("en"), "hello\nworld\nsalam\n").unwrap();
        fs::write(root.join("fa"), "سلام\nدنیا\n").unwrap();
        let dicts = Dictionaries::load(root.join("en"), root.join("fa")).unwrap();
        let mut map = LayoutMap::default();
        for (from, to) in [('s', "س"), ('a', "ا"), ('l', "ل"), ('m', "م")] {
            map.insert(from, to);
        }
        (dicts, map)
    }

    #[test]
    fn offers_suggestion_after_word_boundary_and_tab_applies_it() {
        let (dicts, map) = fixture();
        let mut session = TypingSession::new();
        for ch in "salam".chars() {
            session.handle(SessionEvent::Text(ch.to_string()), &dicts, Language::English, &map);
        }
        assert!(matches!(
            session.handle(SessionEvent::Space, &dicts, Language::English, &map),
            SessionAction::Suggestion(_)
        ));
        assert_eq!(
            session.handle(SessionEvent::Tab, &dicts, Language::English, &map),
            SessionAction::Apply { text: "سلام ".into(), delete_chars: 6 }
        );
        assert!(session.suggestion().is_none());
    }

    #[test]
    fn focus_change_clears_buffer_and_suggestion() {
        let (dicts, map) = fixture();
        let mut session = TypingSession::new();
        for ch in "salam".chars() {
            session.handle(SessionEvent::Text(ch.to_string()), &dicts, Language::English, &map);
        }
        session.handle(SessionEvent::Space, &dicts, Language::English, &map);
        session.handle(SessionEvent::FocusChanged, &dicts, Language::English, &map);
        assert!(session.suggestion().is_none());
        assert_eq!(
            session.handle(SessionEvent::Tab, &dicts, Language::English, &map),
            SessionAction::None
        );
    }

    #[test]
    fn disabling_session_clears_pending_text() {
        let (dicts, map) = fixture();
        let mut session = TypingSession::new();
        for ch in "salam".chars() {
            session.handle(SessionEvent::Text(ch.to_string()), &dicts, Language::English, &map);
        }
        session.handle(SessionEvent::Space, &dicts, Language::English, &map);
        session.set_enabled(false);
        assert_eq!(
            session.handle(SessionEvent::Tab, &dicts, Language::English, &map),
            SessionAction::None
        );
    }

    #[test]
    fn backspace_updates_the_pending_suggestion() {
        let (dicts, map) = fixture();
        let mut session = TypingSession::new();
        for ch in "salam".chars() {
            session.handle(SessionEvent::Text(ch.to_string()), &dicts, Language::English, &map);
        }
        session.handle(SessionEvent::Space, &dicts, Language::English, &map);
        session.handle(SessionEvent::Backspace, &dicts, Language::English, &map);
        assert!(session.suggestion().is_none());
    }
}
