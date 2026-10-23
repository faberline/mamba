//! Character properties come from the three locked Python reference runtimes.
use crate::domain::source::{DependencySource, PythonVersion};
use serde::Deserialize;
use std::{cmp::Ordering, collections::BTreeMap, sync::OnceLock};

#[derive(Deserialize)]
struct Tables {
    versions: BTreeMap<String, Characters>,
}

#[derive(Deserialize)]
pub struct Characters {
    python: String,
    unicode: String,
    upper: Vec<[u32; 2]>,
    lower: Vec<[u32; 2]>,
    title: Vec<[u32; 2]>,
    decimal: Vec<[u32; 2]>,
    lower_map: BTreeMap<u32, String>,
    case_ignorable: Vec<[u32; 2]>,
}

const DATA: &str = include_str!("../../../assets/isort/sorting.json");

fn range(ranges: &[[u32; 2]], character: char) -> Option<[u32; 2]> {
    let point = character as u32;
    ranges
        .binary_search_by(|range| {
            if range[1] < point {
                Ordering::Less
            } else if range[0] > point {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .ok()
        .map(|index| ranges[index])
}

pub fn characters(version: PythonVersion) -> &'static Characters {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    &TABLES
        .get_or_init(|| serde_json::from_str(DATA).expect("locked character data"))
        .versions[version.name()]
}

pub fn source(version: PythonVersion) -> DependencySource {
    static DATA_SHA256: OnceLock<String> = OnceLock::new();
    let data = characters(version);
    let sha256 = DATA_SHA256.get_or_init(|| super::super::metadata::sha256(DATA.as_bytes()));
    DependencySource {
        kind: "embedded-character-data".into(),
        path: None,
        identity: format!(
            "isort 9.0.2 ordering: CPython {}, Unicode {}",
            data.python, data.unicode
        ),
        sha256: Some(sha256.clone()),
    }
}

impl Characters {
    fn cased(&self, character: char) -> bool {
        range(&self.upper, character).is_some()
            || range(&self.lower, character).is_some()
            || range(&self.title, character).is_some()
    }

    /// Python's full lowercase mapping includes a contextual final sigma.
    /// Unassigned characters keep their spelling in the selected Unicode version.
    pub fn lowercase(&self, text: &str) -> String {
        if text.is_ascii() {
            return text.to_ascii_lowercase();
        }
        let characters: Vec<_> = text.chars().collect();
        let ignorable: Vec<_> = characters
            .iter()
            .map(|character| range(&self.case_ignorable, *character).is_some())
            .collect();
        let mut next_cased = vec![false; characters.len()];
        let mut following = false;
        for (index, &character) in characters.iter().enumerate().rev() {
            next_cased[index] = following;
            if !ignorable[index] {
                following = self.cased(character);
            }
        }
        let mut preceding = false;
        let mut result = String::with_capacity(text.len());
        for (index, &character) in characters.iter().enumerate() {
            if character == '\u{3a3}' && preceding && !next_cased[index] {
                result.push('\u{3c2}');
            } else if let Some(mapped) = self.lower_map.get(&(character as u32)) {
                result.push_str(mapped);
            } else {
                result.push(character);
            }
            if !ignorable[index] {
                preceding = self.cased(character);
            }
        }
        result
    }

    pub fn uppercase(&self, character: char) -> bool {
        range(&self.upper, character).is_some()
    }
    pub fn all_uppercase(&self, name: &str) -> bool {
        name.chars().any(|character| self.uppercase(character))
            && !name.chars().any(|character| {
                range(&self.lower, character).is_some() || range(&self.title, character).is_some()
            })
    }
    pub fn digits(&self, text: &str) -> String {
        text.chars()
            .map(|character| {
                range(&self.decimal, character).map_or(character, |range| {
                    char::from(b'0' + ((character as u32 - range[0]) % 10) as u8)
                })
            })
            .collect()
    }
}
