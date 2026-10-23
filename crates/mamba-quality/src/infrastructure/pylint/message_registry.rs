//! Pinned message IDs and registration order are embedded binary data.
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
struct Message {
    code: String,
    symbol: String,
}
#[derive(Deserialize)]
struct Retired {
    kind: String,
    reference: String,
}
#[derive(Deserialize)]
struct Registry {
    messages: Vec<Message>,
    retired: BTreeMap<String, Retired>,
    enabled: BTreeSet<String>,
    ordered_enable: BTreeMap<String, Vec<String>>,
    ordered_disable: BTreeMap<String, Vec<String>>,
    ordered_aliases: BTreeMap<String, Vec<String>>,
    useless_exempt: BTreeSet<String>,
}
fn fixed() -> &'static Registry {
    static VALUE: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();
    VALUE.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/pylint-message-registry.json"))
            .expect("locked Pylint message registry")
    })
}
pub(super) enum Error {
    Unknown,
    Retired(String),
}

pub(super) fn resolve(target: &str, enable: bool) -> Result<Vec<String>, Error> {
    let registry = fixed();
    let ordered = if enable {
        &registry.ordered_enable
    } else {
        &registry.ordered_disable
    };
    let uppercase = target.to_ascii_uppercase();
    let lowercase = target.to_ascii_lowercase();
    if let Some(codes) = ordered.get(&uppercase).or_else(|| ordered.get(&lowercase)) {
        return Ok(codes.clone());
    }
    if lowercase.starts_with("rp") {
        return Ok(Vec::new());
    }
    let normalized = if target.as_bytes().get(1).is_some_and(u8::is_ascii_digit) {
        uppercase.as_str()
    } else {
        target
    };
    if let Some(codes) = registry.ordered_aliases.get(normalized) {
        return Ok(codes.clone());
    }
    if let Some(retired) = registry.retired.get(normalized) {
        let change = if retired.kind == "removed" {
            "removed from pylint"
        } else {
            "moved to an optional extension"
        };
        return Err(Error::Retired(format!(
            "'{target}' was {change}, see {}.",
            retired.reference
        )));
    }
    Err(Error::Unknown)
}
pub(super) fn enabled(rule: &str) -> bool {
    fixed().enabled.contains(rule)
}
pub(super) fn symbol(rule: &str) -> &'static str {
    fixed()
        .messages
        .iter()
        .find(|message| message.code == rule)
        .map(|message| message.symbol.as_str())
        .expect("registered message code")
}
pub(super) fn useless_exempt(rule: &str) -> bool {
    fixed().useless_exempt.contains(rule)
}
