use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

const FA_LIMIT: usize = 30_000;
const PERSIAN_KEYS: &str = ",;'[]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language { English, Persian }

impl Language {
    pub fn parse(s: &str) -> Option<Self> {
        match s { "en" => Some(Self::English), "fa" => Some(Self::Persian), _ => None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub original: String,
    pub converted: String,
    pub source: Language,
}

#[derive(Default)]
pub struct Dictionaries {
    en_rank: HashMap<String, usize>,
    fa_rank: HashMap<String, usize>,
    en_all: HashSet<String>,
}

impl Dictionaries {
    pub fn load(en: impl AsRef<Path>, fa: impl AsRef<Path>) -> io::Result<Self> {
        let en_rank = load_ranked(en)?;
        let fa_rank = load_ranked(fa)?;
        let en_all = en_rank.keys().cloned().collect();
        Ok(Self { en_rank, fa_rank, en_all })
    }

    pub fn with_extra_english_words(mut self, path: impl AsRef<Path>) -> io::Result<Self> {
        if path.as_ref().exists() {
            for line in BufReader::new(File::open(path)?).lines() {
                let word = normalize(&line?);
                if !word.is_empty() { self.en_all.insert(word); }
            }
        }
        Ok(self)
    }

    pub fn is_mistake(&self, typed: &str, language: Language, converted: &str) -> bool {
        self.is_mistake_with_min(typed, language, converted, 3)
    }

    /// Same detection rules with a caller-selected minimum length. The session
    /// adapter uses a lower threshold only for a preceding word joined to a
    /// longer, already-confirmed wrong-layout run (matching Windows behavior).
    pub fn is_mistake_with_min(
        &self,
        typed: &str,
        language: Language,
        converted: &str,
        min_letters: usize,
    ) -> bool {
        let word = normalize(typed);
        if word.chars().count() < min_letters { return false; }
        match language {
            Language::Persian => {
                let english = normalize(converted);
                if english.chars().count() < 3 || !english.is_ascii() { return false; }
                let Some(&en) = self.en_rank.get(&english) else { return false };
                match self.persian_rank(&word) {
                    Some(fa) => en < fa,
                    None => true,
                }
            }
            Language::English => {
                if !word.chars().any(char::is_alphabetic)
                    || !word.chars().all(|c| c.is_alphabetic() || PERSIAN_KEYS.contains(c))
                    || self.en_all.contains(&word) { return false; }
                if word.contains('\\'') {
                    let prefix = word.split('\\'').next().unwrap_or("");
                    if self.en_all.contains(prefix) { return false; }
                }
                self.persian_rank(&normalize(converted)).is_some()
            }
        }
    }

    fn persian_rank(&self, word: &str) -> Option<usize> {
        self.fa_rank.get(word).copied().filter(|rank| *rank < FA_LIMIT)
    }
}

fn load_ranked(path: impl AsRef<Path>) -> io::Result<HashMap<String, usize>> {
    let mut map = HashMap::new();
    for (rank, line) in BufReader::new(File::open(path)?).lines().enumerate() {
        let line = line?;
        let word = normalize(line.split_whitespace().next().unwrap_or(""));
        if !word.is_empty() { map.entry(word).or_insert(rank); }
    }
    Ok(map)
}

pub fn normalize(s: &str) -> String {
    let s = s.to_lowercase().replace('ي', "ی").replace('ك', "ک");
    let s: String = s.chars().filter(|c| !matches!(*c as u32, 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06ED)).collect();
    s.trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace()).to_string()
}

/// Maps characters on one layout to the characters produced by the same physical keys
/// on another layout. The platform backend supplies this map from the active XKB keymap.
#[derive(Default, Clone)]
pub struct LayoutMap { pairs: HashMap<char, String> }

impl LayoutMap {
    pub fn insert(&mut self, source: char, target: impl Into<String>) {
        self.pairs.insert(source, target.into());
    }

    pub fn convert(&self, input: &str) -> String {
        input.chars().map(|c| self.pairs.get(&c).cloned().unwrap_or_else(|| c.to_string())).collect()
    }
}

pub fn suggest(dicts: &Dictionaries, map: &LayoutMap, typed: &str, language: Language) -> Option<Suggestion> {
    let converted = map.convert(typed);
    dicts.is_mistake(typed, language, &converted).then(|| Suggestion {
        original: typed.to_owned(), converted, source: language,
    })
}

pub mod compose;
pub mod session;
pub mod x11;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn dictionaries() -> Dictionaries {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("typefix-test-{}", nonce));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("en"), "hello\\nworld\\nwant\\n").unwrap();
        std::fs::write(root.join("fa"), "سلام\\nدنیا\\n").unwrap();
        Dictionaries::load(root.join("en"), root.join("fa")).unwrap()
    }

    #[test]
    fn normalizes_arabic_variants() {
        assert_eq!(normalize(" كِتاب "), "کتاب");
        assert_eq!(normalize("ي"), "ی");
    }

    #[test]
    fn rejects_short_ambiguous_words() {
        assert!(!dictionaries().is_mistake("hi", Language::English, "سلام"));
    }

    #[test]
    fn preserves_known_english_words() {
        assert!(!dictionaries().is_mistake("hello", Language::English, "سلام"));
    }

    #[test]
    fn layout_map_converts_by_character() {
        let mut map = LayoutMap::default();
        map.insert('s', "س");
        map.insert('l', "م");
        assert_eq!(map.convert("sl"), "سم");
    }

    #[test]
    fn accepts_common_persian_alternate() {
        assert!(dictionaries().is_mistake("salam", Language::English, "سلام"));
    }
}
