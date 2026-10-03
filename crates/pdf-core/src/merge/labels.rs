//! Page labels for combined pages: each page keeps the label it had, and the labels are
//! written back as the fewest rules that produce them.

use crate::labels::{LabelRule, LabelStyle};

/// What a page's label is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PageLabel {
    pub style: LabelStyle,
    pub prefix: String,
    pub number: u32,
}

/// The label parts of physical page `page` under `rules` (sorted by start page). Pages
/// before the first rule, and pages of a document without labels, are numbered 1, 2, 3…
/// as readers number them.
pub(crate) fn page_label(rules: &[LabelRule], page: usize) -> PageLabel {
    match rules.iter().rev().find(|r| r.start_page <= page) {
        Some(rule) => PageLabel {
            style: rule.style,
            prefix: rule.prefix.clone(),
            number: rule
                .first_number
                .saturating_add(u32::try_from(page - rule.start_page).unwrap_or(u32::MAX)),
        },
        None => PageLabel {
            style: LabelStyle::Decimal,
            prefix: String::new(),
            number: u32::try_from(page + 1).unwrap_or(u32::MAX),
        },
    }
}

/// The fewest rules giving page `offset + i` the label `labels[i]`.
pub(crate) fn rules_for(labels: &[PageLabel], offset: usize) -> Vec<LabelRule> {
    let mut rules: Vec<LabelRule> = Vec::new();
    let mut previous: Option<&PageLabel> = None;
    for (i, label) in labels.iter().enumerate() {
        let continues = previous.is_some_and(|p| {
            p.style == label.style
                && p.prefix == label.prefix
                && (label.style == LabelStyle::None
                    || p.number.checked_add(1) == Some(label.number))
        });
        if !continues {
            rules.push(LabelRule {
                start_page: offset + i,
                style: label.style,
                prefix: label.prefix.clone(),
                // A prefix-only label has no number; /St stays at its default.
                first_number: if label.style == LabelStyle::None {
                    1
                } else {
                    label.number.max(1)
                },
            });
        }
        previous = Some(label);
    }
    rules
}

/// True if `rules` label every page 1, 2, 3…, the same as having no labels.
pub(crate) fn is_plain(rules: &[LabelRule]) -> bool {
    matches!(rules, [only] if *only == LabelRule::decimal_from_one(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(start_page: usize, style: LabelStyle, prefix: &str, first_number: u32) -> LabelRule {
        LabelRule {
            start_page,
            style,
            prefix: prefix.into(),
            first_number,
        }
    }

    #[test]
    fn pages_keep_their_labels_through_reordering() {
        let a = vec![
            rule(0, LabelStyle::LowerRoman, "", 1),
            rule(3, LabelStyle::Decimal, "", 1),
        ];
        let b = vec![rule(0, LabelStyle::UpperLetters, "App-", 1)];
        // a: i ii iii 1 2 3; b: App-A App-B; unlabeled c: 1 2.
        let picks: Vec<PageLabel> = vec![
            page_label(&a, 0),
            page_label(&a, 1),
            page_label(&a, 4), // "2": skips "iii" and "1"
            page_label(&a, 5),
            page_label(&b, 1),
            page_label(&b, 0), // reversed
            page_label(&[], 0),
            page_label(&[], 1),
        ];
        let rules = rules_for(&picks, 0);
        assert_eq!(
            rules,
            vec![
                rule(0, LabelStyle::LowerRoman, "", 1),
                rule(2, LabelStyle::Decimal, "", 2),
                rule(4, LabelStyle::UpperLetters, "App-", 2),
                rule(5, LabelStyle::UpperLetters, "App-", 1),
                rule(6, LabelStyle::Decimal, "", 1),
            ]
        );
        let shown = crate::labels::labels_for_pages(&rules, picks.len());
        assert_eq!(shown, ["i", "ii", "2", "3", "App-B", "App-A", "1", "2"]);
    }

    #[test]
    fn consecutive_numbers_across_sources_share_a_rule_and_offsets_apply() {
        let labels = vec![page_label(&[], 0), page_label(&[], 1)];
        assert!(is_plain(&rules_for(&labels, 0)));
        assert_eq!(
            rules_for(&labels, 7),
            vec![rule(7, LabelStyle::Decimal, "", 1)]
        );
        let prefix_only = vec![rule(0, LabelStyle::None, "Cover", 1)];
        let labels = vec![page_label(&prefix_only, 0), page_label(&prefix_only, 0)];
        assert_eq!(rules_for(&labels, 0).len(), 1);
    }
}
