//! Text as search compares it (section 6.1): without case, accents, the vowel points of
//! Arabic and Hebrew or compatibility forms, with the letters that have no accent to take
//! off read as the Latin letters people type for them, and curly quotes as straight ones.
//! "lodz" finds "Łódź", "strasse" finds "Straße", "ешь" finds "ёшь", "don't" finds "don’t".
//!
//! The search bar's suggestions compare words the same way in the frontend (`foldWord` in
//! `viewer/suggestions.ts`); the two follow the same rules.

use unicode_normalization::char::{compose, decompose_compatible};

/// Appends `c`, folded, to `out`, the text folded so far: lower case, its compatibility
/// decomposition (so ligatures, full-width forms, superscripts and Arabic's presentation
/// forms are their plain letters) without the marks [`is_ignored_mark`] names, put back
/// together (so the syllables of Korean stay syllables, and a match can't end inside one),
/// each run of spaces as one space and none at the start.
///
/// Some characters fold into nothing (an accent, a soft hyphen, Arabic's tatweel, which
/// only stretches a word, a second space), and some
/// change the last character of `out` instead of adding one (an accent that was a
/// character of its own after its letter, a Korean jamo after the one it joins): either
/// way `out` is no longer, and the caller counts `c` with the character before it.
pub fn push(out: &mut Vec<char>, c: char) {
    if c.is_whitespace() {
        push_space(out);
        return;
    }
    match c {
        '\u{AD}' | '\u{640}' => return,
        // Й, Ї and Ў are letters of their own, not И, І and У with an accent (Ё is Е).
        'й' | 'Й' => return out.push('й'),
        'ї' | 'Ї' => return out.push('ї'),
        'ў' | 'Ў' => return out.push('ў'),
        // Apostrophes and quotes, curly or as primes, as the straight ones people type
        // (before the decomposition, which would make a double prime two single ones).
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '\u{2035}' | '\u{2BC}' => {
            return out.push('\'');
        }
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' | '\u{2036}' => {
            return out.push('"');
        }
        _ => {}
    }
    if let Some(last) = out.last_mut() {
        let letter = match (*last, c) {
            ('и', '\u{306}') => Some('й'),
            ('і', '\u{308}') => Some('ї'),
            ('у', '\u{306}') => Some('ў'),
            _ => None,
        };
        if let Some(letter) = letter {
            *last = letter;
            return;
        }
    }
    decompose_compatible(c, |part| {
        if part.is_whitespace() {
            push_space(out);
            return;
        }
        for lower in part.to_lowercase() {
            if is_ignored_mark(lower) {
                continue;
            }
            match letter(lower) {
                Letter::One(l) => push_composed(out, l),
                Letter::Two(a, b) => {
                    out.push(a);
                    out.push(b);
                }
            }
        }
    });
}

/// `text` folded (see [`push`]), without a space at the end.
pub fn fold(text: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        push(&mut out, c);
    }
    if out.last() == Some(&' ') {
        out.pop();
    }
    out
}

fn push_space(out: &mut Vec<char>) {
    if out.last().is_some_and(|c| *c != ' ') {
        out.push(' ');
    }
}

/// `c` after the last character of `out`, as one character where the two compose (the
/// jamo of a Korean syllable; a mark kept, after its letter).
fn push_composed(out: &mut Vec<char>, c: char) {
    if let Some(last) = out.last_mut()
        && let Some(both) = compose(*last, c)
    {
        *last = both;
        return;
    }
    out.push(c);
}

/// The marks a search looks past: the combining accents of Latin, Greek and Cyrillic
/// letters; Hebrew's points and cantillation marks (not its punctuation: maqaf, paseq, sof
/// pasuq); Arabic's vowel marks, shadda, sukun, hamza and madda above or below a letter
/// (so أ, إ and آ are found by ا), superscript alef, and the small marks of Quranic text.
/// Text is mostly written without these, and readers type it so. Not the marks of other
/// scripts: the vowel signs and viramas of Indic scripts, Thai's vowels and the voicing
/// marks of Japanese kana change the letter, and leaving them out would have "กน" find
/// "กิน".
fn is_ignored_mark(c: char) -> bool {
    matches!(c,
        '\u{300}'..='\u{36F}'
        | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}'
        | '\u{20D0}'..='\u{20FF}'
        | '\u{FE20}'..='\u{FE2F}'
        // Hebrew
        | '\u{591}'..='\u{5BD}'
        | '\u{5BF}'
        | '\u{5C1}'..='\u{5C2}'
        | '\u{5C4}'..='\u{5C5}'
        | '\u{5C7}'
        // Arabic
        | '\u{610}'..='\u{61A}'
        | '\u{64B}'..='\u{65F}'
        | '\u{670}'
        | '\u{6D6}'..='\u{6DC}'
        | '\u{6DF}'..='\u{6E4}'
        | '\u{6E7}'..='\u{6E8}'
        | '\u{6EA}'..='\u{6ED}'
        | '\u{8D3}'..='\u{8E1}'
        | '\u{8E3}'..='\u{8FF}')
}

enum Letter {
    One(char),
    Two(char, char),
}

/// A lower-case letter that has no accent to take off, as the letters people type for it
/// (Danish and Norwegian ø, Polish ł, Croatian, Serbian and Vietnamese đ, Icelandic and
/// Faroese ð and þ, Maltese ħ, Turkish and Azerbaijani ı, German ß, æ and œ, Sami ŧ), and
/// Greek's final sigma as the sigma it is.
fn letter(c: char) -> Letter {
    match c {
        'ø' => Letter::One('o'),
        'ł' => Letter::One('l'),
        'đ' | 'ð' => Letter::One('d'),
        'ħ' => Letter::One('h'),
        'ı' => Letter::One('i'),
        'ŧ' => Letter::One('t'),
        'ς' => Letter::One('σ'),
        'ß' => Letter::Two('s', 's'),
        'æ' => Letter::Two('a', 'e'),
        'œ' => Letter::Two('o', 'e'),
        'þ' => Letter::Two('t', 'h'),
        c => Letter::One(c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folded(text: &str) -> String {
        fold(text).into_iter().collect()
    }

    #[test]
    fn letters_fold_to_what_people_type() {
        assert_eq!(folded("Łódź"), "lodz");
        assert_eq!(folded("Straße"), "strasse");
        assert_eq!(folded("STRAẞE"), "strasse");
        assert_eq!(folded("Øresund Ærø"), "oresund aero");
        assert_eq!(folded("œuvre Þórshöfn"), "oeuvre thorshofn");
        assert_eq!(
            folded("Đakovo Ħamrun Iğdır İstanbul"),
            "dakovo hamrun igdir istanbul"
        );
        assert_eq!(folded("Tiếng Việt"), "tieng viet");
        assert_eq!(folded("ΆΘΗΝΑ ὁδός"), "αθηνα οδοσ");
        assert_eq!(folded("Ёлка"), "елка");
        // Ligatures, full-width forms and superscripts are their letters.
        assert_eq!(folded("ﬁnd ＡＢＣ x²"), "find abc x2");
        // Spaces as one, none at the ends; soft hyphens go.
        assert_eq!(folded("  a \u{A0}\t b\u{AD}c  "), "a bc");
    }

    #[test]
    fn an_accent_written_after_its_letter_goes_too() {
        assert_eq!(folded("cafe\u{301}"), folded("café"));
        assert_eq!(folded("Lo\u{301}dz\u{301}"), "lodz");
    }

    #[test]
    fn short_i_and_its_kin_stay_letters() {
        assert_eq!(folded("Мой ЙОД"), "мой йод");
        assert_eq!(folded("мои\u{306}"), "мой");
        assert_eq!(folded("Її Ўў"), "її ўў");
        assert_eq!(folded("і\u{308} у\u{306}"), "ї ў");
        assert_ne!(folded("мой"), folded("мои"));
    }

    #[test]
    fn arabic_and_hebrew_are_compared_without_their_points() {
        // Hebrew with points, cantillation and a presentation form (shin with its dot).
        assert_eq!(folded("שָׁלוֹם"), "שלום");
        assert_eq!(folded("בְּרֵאשִׁ֖ית"), "בראשית");
        assert_eq!(folded("\u{FB2A}לום"), "שלום");
        assert_eq!(folded("עַל־פְּנֵי"), "על־פני");
        // Arabic with its vowels, shadda and tatweel; alef and waw with hamza or madda.
        assert_eq!(folded("مُحَمَّد"), "محمد");
        assert_eq!(folded("كـــتـــاب"), "كتاب");
        assert_eq!(folded("أحمد إسلام آمن مؤمن"), "احمد اسلام امن مومن");
        // Presentation forms, as some PDFs have them, are the letters.
        assert_eq!(folded("\u{FEE3}\u{FEA4}\u{FEE4}\u{FEAA}"), "محمد");
    }

    #[test]
    fn curly_quotes_are_straight() {
        assert_eq!(folded("Don’t ‘quote’ me"), "don't 'quote' me");
        assert_eq!(
            folded("“Hello,” „Hallo“ 5′ 3″"),
            "\"hello,\" \"hallo\" 5' 3\""
        );
        assert_eq!(folded("пʼять"), "п'ять");
    }

    #[test]
    fn other_scripts_keep_their_marks() {
        // Thai vowels, a Hindi vowel sign and virama, Japanese voicing.
        assert_eq!(folded("กิน"), "กิน");
        assert_eq!(folded("हिन्दी"), "हिन्दी");
        assert_ne!(folded("が"), "か");
        assert_eq!(folded("が"), folded("か\u{3099}"));
        // Korean syllables stay whole, typed or decomposed.
        assert_eq!(folded("한국"), "한국");
        assert_eq!(folded("\u{1112}\u{1161}\u{11AB}"), "한");
    }
}
