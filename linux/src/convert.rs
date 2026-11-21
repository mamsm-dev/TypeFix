//! Direction detection and conversion for text selected by the user.

use crate::LayoutMap;

fn is_persian_char(c: char) -> bool {
    matches!(c as u32, 0x0600..=0x06FF)
}

/// Converts selected text between the English and Persian keyboard layouts.
///
/// The direction is detected from the text itself: mostly Persian letters are
/// converted to the English layout, anything else to the Persian layout.
/// Characters without a mapping are kept. Returns `None` when the text has no
/// letters or conversion would not change it.
pub fn convert_selection(text: &str, en_to_fa: &LayoutMap, fa_to_en: &LayoutMap) -> Option<String> {
    let persian = text.chars().filter(|c| is_persian_char(*c)).count();
    let latin = text.chars().filter(char::is_ascii_alphabetic).count();
    if persian == 0 && latin == 0 {
        return None;
    }
    let map = if persian > latin { fa_to_en } else { en_to_fa };
    let converted = map.convert(text);
    (converted != text).then_some(converted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn maps() -> (LayoutMap, LayoutMap) {
        let pairs = [('s', "س"), ('g', "ل"), ('h', "ا"), ('l', "م")];
        let mut en_to_fa = LayoutMap::default();
        let mut fa_to_en = LayoutMap::default();
        for (en, fa) in pairs {
            en_to_fa.insert(en, fa);
            fa_to_en.insert(fa.chars().next().unwrap(), en.to_string());
        }
        (en_to_fa, fa_to_en)
    }

    #[test]
    fn converts_english_typed_text_to_persian() {
        let (en_to_fa, fa_to_en) = maps();
        assert_eq!(convert_selection("sghl", &en_to_fa, &fa_to_en).as_deref(), Some("سلام"));
    }

    #[test]
    fn converts_persian_text_back_to_english_layout() {
        let (en_to_fa, fa_to_en) = maps();
        assert_eq!(convert_selection("سلام", &en_to_fa, &fa_to_en).as_deref(), Some("sghl"));
    }

    #[test]
    fn keeps_unmapped_characters_and_whitespace() {
        let (en_to_fa, fa_to_en) = maps();
        assert_eq!(convert_selection("sg\nhl 1", &en_to_fa, &fa_to_en).as_deref(), Some("سل\nام 1"));
    }

    #[test]
    fn returns_none_without_letters_or_changes() {
        let (en_to_fa, fa_to_en) = maps();
        assert_eq!(convert_selection("123 !?", &en_to_fa, &fa_to_en), None);
        assert_eq!(convert_selection("xyz", &en_to_fa, &fa_to_en), None);
    }
}
