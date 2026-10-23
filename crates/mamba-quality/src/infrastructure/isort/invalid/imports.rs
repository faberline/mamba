//! Text imports follow isort's grammar even when Python rejects the source.
use super::super::model::{add_comment, Binding, Import};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Eager,
    Lazy,
    Cython,
}

pub(super) fn starts(code: &str) -> bool {
    let code = code.strip_prefix("lazy ").unwrap_or(code);
    [
        "from ",
        "from.import",
        "from.cimport",
        "import ",
        "import*",
        "cimport ",
        "cimport*",
    ]
    .iter()
    .any(|prefix| code.starts_with(prefix))
}

pub(super) fn parse(raw: &str) -> Option<(Kind, Import)> {
    let (kind, raw) = raw
        .strip_prefix("lazy ")
        .map_or((Kind::Eager, raw), |raw| (Kind::Lazy, raw));
    let from = raw.starts_with("from");
    let kind = if raw.starts_with("cimport") || from && raw.contains("cimport") {
        Kind::Cython
    } else {
        kind
    };
    let mut code = String::new();
    let mut comments = Vec::new();
    for line in raw.lines() {
        let (text, comment) = line
            .split_once('#')
            .map_or((line, None), |(code, comment)| {
                (code, Some(format!("#{}", comment.trim_end())))
            });
        code.push_str(text.trim().trim_end_matches('\\').trim_end());
        code.push(' ');
        if let Some(comment) = comment {
            comments.push((text.to_owned(), comment));
        }
    }
    if !from && code.contains(';') {
        let parts: Vec<_> = code.split(';').map(str::trim).collect();
        if parts.iter().all(|part| part.starts_with(marker_for(kind))) {
            code = format!(
                "{} {}",
                marker_for(kind),
                parts
                    .iter()
                    .filter_map(|part| part.strip_prefix(marker_for(kind)))
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
    }
    let marker = if kind == Kind::Cython {
        "cimport"
    } else {
        "import"
    };
    let (module, payload) = if from {
        let text = code.strip_prefix("from")?.trim();
        let index = text.match_indices(marker).find_map(|(index, _)| {
            let prefix = &text[..index];
            let suffix = &text[index + marker.len()..];
            (prefix.ends_with(char::is_whitespace) || prefix.chars().all(|c| c == '.'))
                .then_some(index)
                .filter(|_| suffix.is_empty() || suffix.starts_with([' ', '\t', '\n', '(', '*']))
        })?;
        (
            text[..index]
                .replace(['(', ')', '\\'], " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(""),
            text[index + marker.len()..].trim(),
        )
    } else {
        (String::new(), code.strip_prefix(marker)?.trim())
    };
    let payload = payload.replace(['(', ')', '\\'], " ");
    let mut names = Vec::new();
    if payload.contains('{') {
        names.push(Binding {
            name: payload.trim().into(),
            alias: None,
            comment: String::new(),
            opening_comment: String::new(),
        });
    } else {
        let words: Vec<_> = payload
            .replace(',', " ")
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let mut index = 0;
        while index < words.len() {
            let name = words[index].clone();
            index += 1;
            let alias = if words.get(index).is_some_and(|word| word == "as") {
                index += 1;
                let alias = words.get(index).cloned();
                index += usize::from(alias.is_some());
                alias
            } else {
                None
            };
            names.push(Binding {
                name,
                alias,
                comment: String::new(),
                opening_comment: String::new(),
            });
        }
    }
    let mut import = Import {
        module,
        from,
        names,
        leading: Vec::new(),
        comment: String::new(),
        multiline: raw.trim_end().ends_with(')')
            && raw
                .trim_end()
                .trim_end_matches(')')
                .trim_end()
                .ends_with(','),
        section: 0,
    };
    for (prefix, comment) in comments {
        let current = prefix
            .rsplit_once(marker)
            .map_or(prefix.as_str(), |(_, value)| value)
            .replace(['(', ')', ',', '\\'], " ");
        let words = current.split_whitespace().collect::<Vec<_>>().join(" ");
        if let Some(name) = import
            .names
            .iter_mut()
            .find(|name| from && words == super::super::render::binding(name))
        {
            add_comment(&mut name.comment, &comment);
        } else if let Some(name) = import
            .names
            .iter_mut()
            .find(|name| from && name.alias.is_some())
        {
            add_comment(&mut name.opening_comment, &comment);
        } else if from {
            add_comment(&mut import.comment, &comment);
        } else if let Some(name) = import.names.first_mut() {
            add_comment(&mut name.comment, &comment);
        }
    }
    Some((kind, import))
}

fn marker_for(kind: Kind) -> &'static str {
    if kind == Kind::Cython {
        "cimport"
    } else {
        "import"
    }
}
