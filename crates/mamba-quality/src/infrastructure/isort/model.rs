use crate::domain::{
    profile::FixedProfile,
    source::{ProjectInfo, PythonVersion},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub name: String,
    pub alias: Option<String>,
    pub comment: String,
    pub opening_comment: String,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub from: bool,
    pub names: Vec<Binding>,
    pub leading: Vec<String>,
    pub comment: String,
    pub multiline: bool,
    pub section: u8,
}

pub fn section(module: &str, project: &ProjectInfo, profile: &FixedProfile) -> u8 {
    if module.starts_with('.') {
        return 4;
    }
    let base = module.split('.').next().unwrap_or(module);
    if base == "__future__" {
        return 0;
    }
    if profile.standard_library[project.python_version.name()]
        .iter()
        .any(|name| name == base)
    {
        return 1;
    }
    if super::placement::first_party(module, project) {
        3
    } else {
        2
    }
}

pub fn module_key(name: &str, version: PythonVersion) -> String {
    let characters = super::sorting::characters(version);
    let dots = name.bytes().take_while(|byte| *byte == b'.').count();
    if dots > 0 {
        format!("{}_{}", &name[..dots], characters.lowercase(&name[dots..]))
    } else {
        characters.lowercase(name)
    }
}

pub fn name_key(name: &str, version: PythonVersion) -> String {
    let characters = super::sorting::characters(version);
    let uppercase = characters.all_uppercase(name);
    let kind = if uppercase && name.chars().count() > 1 {
        'A'
    } else if name
        .chars()
        .next()
        .is_some_and(|character| characters.uppercase(character))
    {
        'B'
    } else {
        'C'
    };
    format!("{kind}{}", characters.lowercase(name))
}

/// Python's natural key compares whole text runs before numeric runs.
/// This keeps `bob2.apple` before `bob.apple`, as in the reference sorter.
pub fn natural_cmp(left: &str, right: &str, version: PythonVersion) -> std::cmp::Ordering {
    let left = super::sorting::characters(version).digits(left);
    let right = super::sorting::characters(version).digits(right);
    let left = left.as_str();
    let right = right.as_str();
    let mut left = left;
    let mut right = right;
    loop {
        let left_number = left
            .bytes()
            .position(|byte| byte.is_ascii_digit())
            .unwrap_or(left.len());
        let right_number = right
            .bytes()
            .position(|byte| byte.is_ascii_digit())
            .unwrap_or(right.len());
        let text_order = left[..left_number].cmp(&right[..right_number]);
        if text_order != std::cmp::Ordering::Equal {
            return text_order;
        }
        left = &left[left_number..];
        right = &right[right_number..];
        if left.is_empty() || right.is_empty() {
            return left.is_empty().cmp(&right.is_empty()).reverse();
        }
        let left_end = left
            .bytes()
            .position(|byte| !byte.is_ascii_digit())
            .unwrap_or(left.len());
        let right_end = right
            .bytes()
            .position(|byte| !byte.is_ascii_digit())
            .unwrap_or(right.len());
        let left_digits = left[..left_end].trim_start_matches('0');
        let right_digits = right[..right_end].trim_start_matches('0');
        let numeric_order = left_digits
            .len()
            .cmp(&right_digits.len())
            .then(left_digits.cmp(right_digits));
        if numeric_order != std::cmp::Ordering::Equal {
            return numeric_order;
        }
        left = &left[left_end..];
        right = &right[right_end..];
    }
}

pub fn add_comment(target: &mut String, comment: &str) {
    let comment = comment.trim_start_matches('#').trim();
    if target.is_empty() {
        *target = if comment.is_empty() {
            "#".into()
        } else {
            format!("# {comment}")
        };
    } else if !target
        .trim_start_matches('#')
        .trim()
        .split("; ")
        .any(|item| item == comment)
        && !comment.is_empty()
    {
        target.push_str("; ");
        target.push_str(comment);
    }
}

pub fn newline(source: &str) -> &'static str {
    if source.contains("\r\n") {
        "\r\n"
    } else if source.contains('\r') {
        "\r"
    } else {
        "\n"
    }
}

/// The original file checker tracks section changes separately from output bytes.
pub(super) fn reference_changed(
    source: &crate::domain::source::SourceSnapshot,
    workspace: &crate::domain::source::QualityWorkspace,
    profile: &FixedProfile,
    output: &str,
) -> bool {
    let normalized;
    let source = if source.text.contains('\r') {
        normalized = crate::domain::source::SourceSnapshot {
            text: source.text.replace("\r\n", "\n").replace('\r', "\n"),
            ..source.clone()
        };
        &normalized
    } else {
        source
    };
    let Ok(document) = mamba_language::application::quality_syntax::parse(
        &source.text,
        super::super::native::target(workspace, source),
        source.is_stub,
    ) else {
        return source.text.trim_end() != output.trim_end();
    };
    if document.is_valid() {
        source.text.trim_end() != output.replace("\r\n", "\n").replace('\r', "\n").trim_end()
    } else {
        super::invalid::format_with_changes(source, workspace, profile, &document)
            .map(|(_, _, changed)| changed)
            .unwrap_or(true)
    }
}
