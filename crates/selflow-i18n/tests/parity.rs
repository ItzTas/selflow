use std::collections::{BTreeMap, BTreeSet};

use selflow_i18n::catalog::catalog;
use selflow_i18n::language::Language;

fn ids(language: Language) -> BTreeMap<&'static str, BTreeSet<String>> {
    catalog(language)
        .iter()
        .map(|(file, text)| {
            let ids = text
                .lines()
                .filter_map(|line| line.split_once(" =").map(|(id, _)| id))
                .filter(|id| {
                    id.starts_with(|c: char| c.is_ascii_lowercase())
                        && id
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                })
                .map(str::to_owned)
                .collect();
            (*file, ids)
        })
        .collect()
}

#[test]
fn every_message_exists_in_both_languages() {
    assert_eq!(ids(Language::En), ids(Language::PtBr));
}
