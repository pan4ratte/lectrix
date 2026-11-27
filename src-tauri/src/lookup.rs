//! Looking selected text up on the web (section 6.1, ADR 0037): the address of a search or
//! a translation on one of a few websites fixed here, so the webview can have no other page
//! opened, and only the text goes into it.

use crate::ipc::WebLookup;

/// A lookup's web address, and whether the text had to be shortened to fit it.
#[derive(Debug, PartialEq, Eq)]
pub struct Lookup {
    pub url: String,
    pub shortened: bool,
}

/// The address that looks `text` up on `service`, at most `max_len` bytes long; `language`
/// (a language tag, "ru-RU") is the one translations go into and the Wikipedia searched.
/// The text's lines and spaces run together into single spaces; a text too long for the
/// address loses its end, at a space where there is one. `None` for a text of nothing but
/// spaces.
pub fn lookup_url(
    service: WebLookup,
    text: &str,
    language: &str,
    max_len: usize,
) -> Option<Lookup> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    let lang = primary_language(language);
    let prefix = match service {
        WebLookup::GoogleTranslate => format!(
            "https://translate.google.com/?sl=auto&tl={}&op=translate&text=",
            google_language(language, lang)
        ),
        WebLookup::Deepl => {
            // DeepL fills its boxes from the fragment only when it names a source language
            // ("auto" and "-" leave the page empty), and detects the text's language
            // whatever that is, unless it is the target: then it ignores the target too.
            let target = deepl_language(language, lang);
            let source = if target == "en" { "de" } else { "en" };
            format!("https://www.deepl.com/translator#{source}/{target}/")
        }
        WebLookup::Google => "https://www.google.com/search?q=".to_string(),
        WebLookup::DuckDuckGo => "https://duckduckgo.com/?q=".to_string(),
        WebLookup::Wikipedia => format!(
            "https://{}.wikipedia.org/w/index.php?search=",
            wikipedia_language(lang)
        ),
        WebLookup::GoogleScholar => "https://scholar.google.com/scholar?q=".to_string(),
        WebLookup::SemanticScholar => "https://www.semanticscholar.org/search?q=".to_string(),
        WebLookup::Jstor => "https://www.jstor.org/action/doBasicSearch?Query=".to_string(),
    };
    // DeepL splits its fragment at a slash or a bar, encoded or not, and drops a lone
    // backslash; a backslash before each keeps them.
    let fragment = service == WebLookup::Deepl;
    let room = max_len.saturating_sub(prefix.len());
    let mut encoded = String::new();
    let mut kept = 0;
    // How long the encoding was where the last space began.
    let mut at_space = None;
    let mut piece = String::new();
    for (i, c) in text.char_indices() {
        piece.clear();
        if fragment && matches!(c, '/' | '|' | '\\') {
            encode_char('\\', &mut piece);
        }
        encode_char(c, &mut piece);
        if encoded.len() + piece.len() > room {
            break;
        }
        if c == ' ' {
            at_space = Some(encoded.len());
        }
        encoded.push_str(&piece);
        kept = i + c.len_utf8();
    }
    let shortened = kept < text.len();
    if shortened && let Some(len) = at_space {
        encoded.truncate(len);
    }
    if encoded.is_empty() {
        return None;
    }
    Some(Lookup {
        url: format!("{prefix}{encoded}"),
        shortened,
    })
}

/// Appends `c` percent-encoded (RFC 3986: all but the unreserved characters).
fn encode_char(c: char, out: &mut String) {
    use std::fmt::Write;
    if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
        out.push(c);
        return;
    }
    let mut bytes = [0; 4];
    for b in c.encode_utf8(&mut bytes).bytes() {
        // Writing to a String can't fail.
        let _ = write!(out, "%{b:02X}");
    }
}

/// The language setting as stored: "system", or a language tag of two or three letters,
/// with a region or script of two to four letters or digits ("zh-TW"); anything else is
/// "system".
pub fn language_setting(value: &str) -> String {
    let mut parts = value.split('-');
    let primary = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();
    let valid = (2..=3).contains(&primary.len())
        && primary.bytes().all(|b| b.is_ascii_alphabetic())
        && rest.len() <= 1
        && rest
            .iter()
            .all(|s| (2..=4).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric()));
    if valid {
        value.to_string()
    } else {
        crate::store::SYSTEM_LANGUAGE.to_string()
    }
}

/// The language of a tag ("pt" of "pt-BR"), in lower case; English for a tag that isn't
/// one, so the address can only hold letters there.
fn primary_language(tag: &str) -> String {
    let primary = tag.split(['-', '_']).next().unwrap_or("");
    if (2..=3).contains(&primary.len()) && primary.bytes().all(|b| b.is_ascii_alphabetic()) {
        primary.to_ascii_lowercase()
    } else {
        "en".to_string()
    }
}

/// Google Translate's code for the language: Chinese in its two scripts, Hebrew by its
/// old code, Norwegian as one.
fn google_language(tag: &str, lang: String) -> String {
    let region = tag.split(['-', '_']).skip(1).map(str::to_ascii_lowercase);
    match lang.as_str() {
        "zh" => {
            let traditional = region
                .into_iter()
                .any(|r| matches!(r.as_str(), "tw" | "hk" | "mo" | "hant"));
            if traditional { "zh-TW" } else { "zh-CN" }.to_string()
        }
        "he" => "iw".to_string(),
        "nb" | "nn" => "no".to_string(),
        _ => lang,
    }
}

/// DeepL's code for the language: Chinese in traditional script as "zh-hant".
fn deepl_language(tag: &str, lang: String) -> String {
    if lang == "zh" && google_language(tag, lang.clone()) == "zh-TW" {
        "zh-hant".to_string()
    } else {
        lang
    }
}

/// The Wikipedia in the language: Norwegian Bokmål's is "no".
fn wikipedia_language(lang: String) -> String {
    if lang == "nb" { "no".to_string() } else { lang }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(service: WebLookup, text: &str) -> String {
        lookup_url(service, text, "ru-RU", 2048).unwrap().url
    }

    #[test]
    fn every_website_gets_the_text_encoded() {
        let text = "Café & co?";
        let q = "Caf%C3%A9%20%26%20co%3F";
        assert_eq!(
            url(WebLookup::GoogleTranslate, text),
            format!("https://translate.google.com/?sl=auto&tl=ru&op=translate&text={q}")
        );
        assert_eq!(
            url(WebLookup::Deepl, text),
            format!("https://www.deepl.com/translator#en/ru/{q}")
        );
        assert_eq!(
            url(WebLookup::Google, text),
            format!("https://www.google.com/search?q={q}")
        );
        assert_eq!(
            url(WebLookup::DuckDuckGo, text),
            format!("https://duckduckgo.com/?q={q}")
        );
        assert_eq!(
            url(WebLookup::Wikipedia, text),
            format!("https://ru.wikipedia.org/w/index.php?search={q}")
        );
        assert_eq!(
            url(WebLookup::GoogleScholar, text),
            format!("https://scholar.google.com/scholar?q={q}")
        );
        assert_eq!(
            url(WebLookup::SemanticScholar, text),
            format!("https://www.semanticscholar.org/search?q={q}")
        );
        assert_eq!(
            url(WebLookup::Jstor, text),
            format!("https://www.jstor.org/action/doBasicSearch?Query={q}")
        );
    }

    #[test]
    fn lines_and_spaces_run_together() {
        assert_eq!(
            url(WebLookup::Google, "  one\r\ntwo\t three \n"),
            "https://www.google.com/search?q=one%20two%20three"
        );
        assert_eq!(lookup_url(WebLookup::Google, " \n\t", "en", 2048), None);
    }

    #[test]
    fn deepl_keeps_slashes_bars_and_backslashes_with_a_backslash() {
        // As DeepL was found to read them (2026-11-28): "and/or" alone comes as "and".
        assert_eq!(
            url(WebLookup::Deepl, r"and/or a\b c|d"),
            "https://www.deepl.com/translator#en/ru/and%5C%2For%20a%5C%5Cb%20c%5C%7Cd"
        );
    }

    #[test]
    fn deepl_names_a_source_other_than_the_target() {
        let deepl = |tag: &str| lookup_url(WebLookup::Deepl, "x", tag, 2048).unwrap().url;
        assert_eq!(deepl("de-DE"), "https://www.deepl.com/translator#en/de/x");
        assert_eq!(deepl("en-GB"), "https://www.deepl.com/translator#de/en/x");
        assert_eq!(
            deepl("zh-TW"),
            "https://www.deepl.com/translator#en/zh-hant/x"
        );
        assert_eq!(deepl("zh-CN"), "https://www.deepl.com/translator#en/zh/x");
        assert_eq!(deepl("nb"), "https://www.deepl.com/translator#en/nb/x");
    }

    #[test]
    fn language_setting_is_system_or_a_tag() {
        for ok in ["system", "ru", "zh-TW", "nb", "pt-BR", "fil"] {
            assert_eq!(language_setting(ok), ok);
        }
        for bad in ["", "russian", "zh-TW-x", "ru/../", "r", "en-", "e1"] {
            assert_eq!(language_setting(bad), "system", "{bad}");
        }
    }

    #[test]
    fn languages() {
        let tl = |tag: &str| {
            let u = lookup_url(WebLookup::GoogleTranslate, "x", tag, 2048)
                .unwrap()
                .url;
            u["https://translate.google.com/?sl=auto&tl=".len()..]
                .split('&')
                .next()
                .unwrap()
                .to_string()
        };
        assert_eq!(tl("en-US"), "en");
        assert_eq!(tl("DE"), "de");
        assert_eq!(tl("zh-CN"), "zh-CN");
        assert_eq!(tl("zh-Hant-TW"), "zh-TW");
        assert_eq!(tl("he-IL"), "iw");
        assert_eq!(tl("nb-NO"), "no");
        // Not a language tag: nothing of it reaches the address.
        assert_eq!(tl("x/../evil.com#"), "en");
        assert_eq!(tl(""), "en");
        assert_eq!(
            lookup_url(WebLookup::Wikipedia, "x", "nb-NO", 2048)
                .unwrap()
                .url,
            "https://no.wikipedia.org/w/index.php?search=x"
        );
        assert_eq!(
            lookup_url(WebLookup::Wikipedia, "x", "evil.com/", 2048)
                .unwrap()
                .url,
            "https://en.wikipedia.org/w/index.php?search=x"
        );
    }

    #[test]
    fn long_text_is_cut_at_a_space() {
        let prefix = "https://www.google.com/search?q=".len();
        let got = lookup_url(WebLookup::Google, "alpha beta gamma", "en", prefix + 14).unwrap();
        assert_eq!(got.url, "https://www.google.com/search?q=alpha");
        assert!(got.shortened);
        // One long word is cut where it no longer fits, never inside a character's encoding.
        let got = lookup_url(WebLookup::Google, "ééééé", "en", prefix + 13).unwrap();
        assert_eq!(got.url, "https://www.google.com/search?q=%C3%A9%C3%A9");
        assert!(got.shortened);
        let whole = lookup_url(WebLookup::Google, "alpha beta", "en", 2048).unwrap();
        assert!(!whole.shortened);
        assert!(lookup_url(WebLookup::Google, "é", "en", prefix + 3).is_none());
    }
}
