//! Page labels (`/PageLabels` number tree in the document catalog).
//!
//! Labels are edited as a list of [`LabelRule`]s. Each rule starts at a physical page and
//! runs until the next rule. A rule at page 0 always exists in a written tree, because the
//! format requires one.
//!
//! Labels are read leniently and shown as stored. They are written only when the user
//! changes them ([`set_rules`] does nothing when the rules are already stored that way), so
//! a file whose labels the user did not edit keeps its `/PageLabels` bytes.

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::{Error, Result};
use crate::objects::text_string;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LabelStyle {
    /// Prefix only, no number.
    None,
    /// 1 2 3 (`/S /D`)
    Decimal,
    /// I II III (`/S /R`)
    UpperRoman,
    /// i ii iii (`/S /r`)
    LowerRoman,
    /// A B C … Z AA BB (`/S /A`)
    UpperLetters,
    /// a b c … z aa bb (`/S /a`)
    LowerLetters,
}

impl LabelStyle {
    fn pdf_name(self) -> Option<&'static str> {
        match self {
            LabelStyle::None => None,
            LabelStyle::Decimal => Some("D"),
            LabelStyle::UpperRoman => Some("R"),
            LabelStyle::LowerRoman => Some("r"),
            LabelStyle::UpperLetters => Some("A"),
            LabelStyle::LowerLetters => Some("a"),
        }
    }

    fn from_pdf_name(name: &[u8]) -> LabelStyle {
        match name {
            b"D" => LabelStyle::Decimal,
            b"R" => LabelStyle::UpperRoman,
            b"r" => LabelStyle::LowerRoman,
            b"A" => LabelStyle::UpperLetters,
            b"a" => LabelStyle::LowerLetters,
            _ => LabelStyle::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelRule {
    /// Physical page index (0-based) where this rule starts.
    pub start_page: usize,
    pub style: LabelStyle,
    pub prefix: String,
    /// Number of the first page in the range (`/St`), 1 or higher.
    pub first_number: u32,
}

impl LabelRule {
    pub fn decimal_from_one(start_page: usize) -> LabelRule {
        LabelRule {
            start_page,
            style: LabelStyle::Decimal,
            prefix: String::new(),
            first_number: 1,
        }
    }
}

/// The longest label shown, in characters. Roman numerals and letters grow with the
/// number ("MMMM…", "aaaa…"), and a damaged or hostile file can ask for numbers in the
/// billions; labels past this length would only cost memory on every page.
pub const MAX_LABEL_CHARS: usize = 256;

/// Formats `n` (1-based) in `style`, without prefix. Roman numerals and letters that would
/// be longer than [`MAX_LABEL_CHARS`] are written as decimal numbers instead.
pub fn format_number(n: u32, style: LabelStyle) -> String {
    const MAX: u32 = MAX_LABEL_CHARS as u32;
    let too_long = match style {
        // One "M" per thousand, plus at most 12 characters for the rest ("CMXCIX" is
        // 6; "DCCCLXXXVIII" is the longest, 12).
        LabelStyle::UpperRoman | LabelStyle::LowerRoman => n / 1000 + 12 > MAX,
        LabelStyle::UpperLetters | LabelStyle::LowerLetters => n.saturating_sub(1) / 26 >= MAX,
        LabelStyle::None | LabelStyle::Decimal => false,
    };
    if too_long {
        return n.to_string();
    }
    match style {
        LabelStyle::None => String::new(),
        LabelStyle::Decimal => n.to_string(),
        LabelStyle::UpperRoman => roman(n),
        LabelStyle::LowerRoman => roman(n).to_ascii_lowercase(),
        LabelStyle::UpperLetters => letters(n, b'A'),
        LabelStyle::LowerLetters => letters(n, b'a'),
    }
}

fn roman(mut n: u32) -> String {
    const TABLE: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, digits) in TABLE {
        while n >= value {
            out.push_str(digits);
            n -= value;
        }
    }
    out
}

/// PDF letter numbering: a…z, then aa…zz, then aaa…zzz (one letter repeated).
fn letters(n: u32, base: u8) -> String {
    if n == 0 {
        return String::new();
    }
    // (n - 1) % 26 < 26, so the cast is lossless and base + index stays within the letters.
    let index = ((n - 1) % 26) as u8;
    let repeat = (n - 1) / 26 + 1;
    let letter = char::from(base + index);
    std::iter::repeat_n(letter, repeat as usize).collect()
}

/// The label of physical page `page` under `rules`, which must be sorted by start page.
/// Pages before the first rule (only possible in malformed files) get decimal numbers.
/// Labels are cut at [`MAX_LABEL_CHARS`] characters (a very long prefix in a damaged file).
pub fn label_for_page(rules: &[LabelRule], page: usize) -> String {
    match rules.iter().rev().find(|r| r.start_page <= page) {
        Some(rule) => {
            let offset = u32::try_from(page - rule.start_page).unwrap_or(u32::MAX);
            let n = rule.first_number.saturating_add(offset);
            let mut label = format!("{}{}", rule.prefix, format_number(n, rule.style));
            if let Some((cut, _)) = label.char_indices().nth(MAX_LABEL_CHARS) {
                label.truncate(cut);
            }
            label
        }
        None => (page + 1).to_string(),
    }
}

/// Every page's label under `rules` (sorted by start page).
pub fn labels_for_pages(rules: &[LabelRule], page_count: usize) -> Vec<String> {
    (0..page_count).map(|p| label_for_page(rules, p)).collect()
}

/// Sorts the rules, checks them against `page_count`, and inserts the default rule at page
/// 0 if none exists.
pub fn normalize_rules(mut rules: Vec<LabelRule>, page_count: usize) -> Result<Vec<LabelRule>> {
    rules.sort_by_key(|r| r.start_page);
    for pair in rules.windows(2) {
        if pair[0].start_page == pair[1].start_page {
            return Err(Error::InvalidArgument(format!(
                "two label rules start at page {}",
                pair[0].start_page + 1
            )));
        }
    }
    for rule in &rules {
        if rule.start_page >= page_count {
            return Err(Error::InvalidArgument(format!(
                "label rule starts at page {}, but the document has {page_count} pages",
                rule.start_page + 1
            )));
        }
        if rule.first_number == 0 {
            return Err(Error::InvalidArgument(
                "label numbers start at 1 or higher".into(),
            ));
        }
        // `/St` is written as a PDF integer.
        if i32::try_from(rule.first_number).is_err() {
            return Err(Error::InvalidArgument(format!(
                "label number {} is too large",
                rule.first_number
            )));
        }
    }
    if rules.first().is_none_or(|r| r.start_page != 0) {
        rules.insert(0, LabelRule::decimal_from_one(0));
    }
    Ok(rules)
}

/// Reads the document's label rules exactly as stored. Returns an empty list when the
/// document has no `/PageLabels`.
pub fn read_rules(doc: &PdfDocument) -> Result<Vec<LabelRule>> {
    let catalog = doc.catalog()?;
    let Some(tree) = catalog.get_dict("PageLabels")? else {
        return Ok(Vec::new());
    };
    let mut rules = Vec::new();
    collect_number_tree(&tree, &mut rules, 0)?;
    rules.sort_by_key(|r| r.start_page);
    rules.dedup_by_key(|r| r.start_page);
    Ok(rules)
}

/// Walks a number tree node (`/Nums` leaves, `/Kids` intermediate nodes).
fn collect_number_tree(node: &PdfObject, out: &mut Vec<LabelRule>, depth: u32) -> Result<()> {
    // Guards against reference cycles in malformed files.
    if depth > 32 {
        return Ok(());
    }
    if let Some(nums) = node.get_dict("Nums")? {
        let len = i32::try_from(nums.len()?).unwrap_or(i32::MAX);
        let mut i = 0;
        while i + 1 < len {
            let key = nums.get_array(i)?;
            let value = nums.get_array(i + 1)?;
            i += 2;
            let (Some(key), Some(value)) = (key, value) else {
                continue;
            };
            let Ok(start) = usize::try_from(key.as_int()?) else {
                continue;
            };
            out.push(rule_from_dict(start, &value)?);
        }
    }
    if let Some(kids) = node.get_dict("Kids")? {
        for i in 0..i32::try_from(kids.len()?).unwrap_or(i32::MAX) {
            if let Some(kid) = kids.get_array(i)? {
                collect_number_tree(&kid, out, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn rule_from_dict(start_page: usize, dict: &PdfObject) -> Result<LabelRule> {
    let style = match dict.get_dict("S")? {
        Some(s) if s.is_name()? => LabelStyle::from_pdf_name(&s.as_name()?),
        _ => LabelStyle::None,
    };
    let prefix = match dict.get_dict("P")? {
        Some(p) if p.is_string()? => p.as_string_lossy()?,
        _ => String::new(),
    };
    let first_number = match dict.get_dict("St")? {
        Some(st) if st.is_number()? => u32::try_from(st.as_int()?)
            .ok()
            .filter(|n| *n >= 1)
            .unwrap_or(1),
        _ => 1,
    };
    Ok(LabelRule {
        start_page,
        style,
        prefix,
        first_number,
    })
}

/// Sets the document's labels to `rules`; an empty `rules` removes them ("Remove all
/// labels"). Writes nothing and returns false when the document already stores exactly
/// these rules, so a commit that changes nothing leaves the file and the undo history
/// alone.
pub fn set_rules(doc: &mut PdfDocument, rules: Vec<LabelRule>) -> Result<bool> {
    let rules = if rules.is_empty() {
        rules
    } else {
        let page_count = usize::try_from(doc.page_count()?).unwrap_or(0);
        normalize_rules(rules, page_count)?
    };
    let stored = doc.catalog()?.get_dict("PageLabels")?.is_some();
    if (rules.is_empty() && !stored) || (stored && !rules.is_empty() && read_rules(doc)? == rules) {
        return Ok(false);
    }
    write_rules(doc, rules)?;
    Ok(true)
}

/// Replaces the document's `/PageLabels` with a flat number tree built from `rules`.
/// An empty `rules` removes the labels entirely ("Remove all labels").
///
/// An existing tree that is an indirect object is rewritten in place (any `/Kids` it had
/// are no longer referenced), so the catalog itself does not change.
pub fn write_rules(doc: &mut PdfDocument, rules: Vec<LabelRule>) -> Result<()> {
    let mut catalog = doc.catalog()?;
    if rules.is_empty() {
        catalog.dict_delete("PageLabels")?;
        return Ok(());
    }
    let page_count = usize::try_from(doc.page_count()?).unwrap_or(0);
    let rules = normalize_rules(rules, page_count)?;

    let mut nums = doc.new_array()?;
    for rule in &rules {
        let mut dict = doc.new_dict()?;
        if let Some(name) = rule.style.pdf_name() {
            dict.dict_put("S", PdfObject::new_name(name)?)?;
        }
        if !rule.prefix.is_empty() {
            dict.dict_put("P", text_string(doc, &rule.prefix)?)?;
        }
        if rule.first_number != 1 {
            // normalize_rules checked that the number fits.
            let st = i32::try_from(rule.first_number).unwrap_or(i32::MAX);
            dict.dict_put("St", PdfObject::new_int(st)?)?;
        }
        let start = i32::try_from(rule.start_page)
            .map_err(|_| Error::InvalidArgument("page index too large".into()))?;
        nums.array_push(PdfObject::new_int(start)?)?;
        nums.array_push(dict)?;
    }
    match catalog.get_dict("PageLabels")? {
        Some(mut tree) if tree.is_indirect()? && tree.is_dict()? => {
            tree.dict_delete("Kids")?;
            tree.dict_delete("Limits")?;
            tree.dict_put("Nums", nums)?;
        }
        _ => {
            let mut tree = doc.new_dict()?;
            tree.dict_put("Nums", nums)?;
            let tree = doc.add_object(&tree)?;
            catalog.dict_put("PageLabels", tree)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roman_numerals() {
        let lower: Vec<_> = (1..=12)
            .map(|n| format_number(n, LabelStyle::LowerRoman))
            .collect();
        assert_eq!(
            lower,
            [
                "i", "ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x", "xi", "xii"
            ]
        );
        assert_eq!(format_number(1994, LabelStyle::UpperRoman), "MCMXCIV");
        assert_eq!(format_number(3999, LabelStyle::UpperRoman), "MMMCMXCIX");
        assert_eq!(format_number(4000, LabelStyle::UpperRoman), "MMMM");
    }

    #[test]
    fn letters_past_z_repeat_the_letter() {
        assert_eq!(format_number(1, LabelStyle::LowerLetters), "a");
        assert_eq!(format_number(26, LabelStyle::LowerLetters), "z");
        assert_eq!(format_number(27, LabelStyle::LowerLetters), "aa");
        assert_eq!(format_number(28, LabelStyle::LowerLetters), "bb");
        assert_eq!(format_number(52, LabelStyle::LowerLetters), "zz");
        assert_eq!(format_number(53, LabelStyle::UpperLetters), "AAA");
    }

    #[test]
    fn huge_numbers_and_prefixes_stay_short() {
        // Up to the limit, the style is kept...
        // 244 "M"s and "DCCCLXXXVIII".
        assert_eq!(format_number(244_888, LabelStyle::UpperRoman).len(), 256);
        assert_eq!(
            format_number(26 * 256, LabelStyle::LowerLetters),
            "z".repeat(256)
        );
        // ...beyond it, the number is written in decimal.
        assert_eq!(format_number(245_000, LabelStyle::UpperRoman), "245000");
        assert_eq!(
            format_number(26 * 256 + 1, LabelStyle::LowerLetters),
            "6657"
        );
        assert_eq!(
            format_number(u32::MAX, LabelStyle::LowerRoman),
            u32::MAX.to_string()
        );
        let long_prefix = LabelRule {
            start_page: 0,
            style: LabelStyle::Decimal,
            prefix: "é".repeat(10_000),
            first_number: 1,
        };
        assert_eq!(
            label_for_page(&[long_prefix], 0).chars().count(),
            MAX_LABEL_CHARS
        );
    }

    #[test]
    fn rules_apply_with_prefix_and_start() {
        let rules = vec![
            LabelRule {
                start_page: 0,
                style: LabelStyle::LowerRoman,
                prefix: String::new(),
                first_number: 1,
            },
            LabelRule::decimal_from_one(4),
            LabelRule {
                start_page: 10,
                style: LabelStyle::Decimal,
                prefix: "A-".into(),
                first_number: 3,
            },
            LabelRule {
                start_page: 12,
                style: LabelStyle::None,
                prefix: "Cover".into(),
                first_number: 1,
            },
        ];
        let labels: Vec<_> = (0..13).map(|p| label_for_page(&rules, p)).collect();
        assert_eq!(
            labels,
            [
                "i", "ii", "iii", "iv", "1", "2", "3", "4", "5", "6", "A-3", "A-4", "Cover"
            ]
        );
    }

    #[test]
    fn normalize_adds_first_page_rule_and_rejects_bad_input() {
        let rules = normalize_rules(vec![LabelRule::decimal_from_one(3)], 10).unwrap();
        assert_eq!(rules[0], LabelRule::decimal_from_one(0));
        assert_eq!(rules.len(), 2);
        assert!(normalize_rules(vec![LabelRule::decimal_from_one(10)], 10).is_err());
        assert!(
            normalize_rules(
                vec![
                    LabelRule::decimal_from_one(2),
                    LabelRule::decimal_from_one(2)
                ],
                10
            )
            .is_err()
        );
        let zero = LabelRule {
            first_number: 0,
            ..LabelRule::decimal_from_one(0)
        };
        assert!(normalize_rules(vec![zero], 10).is_err());
        let huge = LabelRule {
            first_number: u32::MAX,
            ..LabelRule::decimal_from_one(0)
        };
        assert!(normalize_rules(vec![huge], 10).is_err());
    }

    #[test]
    fn set_rules_writes_only_changes() {
        use crate::testgen::{SampleSpec, sample_document};
        let mut doc = sample_document(&SampleSpec {
            pages: 6,
            ..SampleSpec::default()
        })
        .unwrap();
        // Removing labels the document does not have changes nothing.
        assert!(!set_rules(&mut doc, Vec::new()).unwrap());
        let rules = vec![
            LabelRule {
                style: LabelStyle::LowerRoman,
                ..LabelRule::decimal_from_one(0)
            },
            LabelRule::decimal_from_one(2),
        ];
        assert!(set_rules(&mut doc, rules.clone()).unwrap());
        let tree = doc
            .catalog()
            .unwrap()
            .get_dict("PageLabels")
            .unwrap()
            .unwrap();
        let object = tree.as_indirect().unwrap();
        // The same rules, in another order: nothing to write.
        let reversed: Vec<_> = rules.iter().rev().cloned().collect();
        assert!(!set_rules(&mut doc, reversed).unwrap());
        // A change rewrites the tree object in place.
        let changed = vec![
            LabelRule::decimal_from_one(0),
            LabelRule::decimal_from_one(3),
        ];
        assert!(set_rules(&mut doc, changed.clone()).unwrap());
        let tree = doc
            .catalog()
            .unwrap()
            .get_dict("PageLabels")
            .unwrap()
            .unwrap();
        assert_eq!(tree.as_indirect().unwrap(), object);
        assert_eq!(read_rules(&doc).unwrap(), changed);
        assert!(set_rules(&mut doc, Vec::new()).unwrap());
        assert!(read_rules(&doc).unwrap().is_empty());
    }
}
