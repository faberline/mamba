//! Import order retains Pylint's isort classification and leave-module ordering.
mod permission;
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeSet;
#[derive(Clone)]
struct Import {
    range: TextRange,
    package: String,
    display: String,
    from: bool,
    nested: bool,
    exempt: bool,
    section: u8,
}
#[derive(Default)]
struct Imports {
    values: Vec<Import>,
    depth: usize,
    exempt: bool,
}
impl<'a> Visitor<'a> for Imports {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            return;
        }
        match statement {
            Stmt::Import(value) => {
                for name in &value.names {
                    let first = value.names[0].name.as_str();
                    let package = name.name.as_str().split('.').next().unwrap().to_string();
                    self.values.push(Import {
                        range: value.range(),
                        display: if first.split('.').next() == Some(package.as_str()) {
                            first.to_string()
                        } else {
                            package.clone()
                        },
                        package,
                        from: false,
                        nested: self.depth > 0,
                        exempt: self.exempt,
                        section: 0,
                    });
                }
            }
            Stmt::ImportFrom(value) => {
                let module = value.module.as_ref().map_or("", |value| value.as_str());
                let base = if module.is_empty() {
                    value.names[0].name.as_str().split('.').next().unwrap()
                } else {
                    module.split('.').next().unwrap()
                };
                let package = format!("{}{base}", if value.level > 0 { "." } else { "" });
                self.values.push(Import {
                    range: value.range(),
                    package,
                    display: format!("{module}.{}", value.names[0].name),
                    from: true,
                    nested: self.depth > 0,
                    exempt: self.exempt,
                    section: 0,
                });
            }
            _ => {
                self.depth += 1;
                let previous = self.exempt;
                if let Stmt::If(value) = statement {
                    let text = super::values::path(&value.test);
                    self.exempt |= text.is_some_and(|value| {
                        value.last().is_some_and(|name| name == "TYPE_CHECKING")
                    });
                    self.exempt |= super::import_rules::sys_guard(&value.test)
                        || super::import_rules::platform_guard(&value.test);
                }
                walk_stmt(self, statement);
                self.exempt = previous;
                self.depth -= 1;
            }
        }
    }
}
fn group(values: &[Import], section: u8) -> String {
    let values: Vec<_> = values
        .iter()
        .filter(|value| value.section == section)
        .collect();
    if values.is_empty() {
        return String::new();
    }
    let names: Vec<_> = if values.len() > 6 {
        values[..3]
            .iter()
            .chain(values[values.len() - 3..].iter())
            .map(|value| format!("\"{}\"", value.display))
            .collect()
    } else {
        values
            .iter()
            .map(|value| format!("\"{}\"", value.display))
            .collect()
    };
    let names = if values.len() > 6 {
        format!("{} (...) {}", names[..3].join(", "), names[3..].join(", "))
    } else {
        names.join(", ")
    };
    let label = match section {
        1 => "third party",
        2 => "first party",
        _ => "local",
    };
    format!(
        "{label} import{} {names}",
        if values.len() > 1 { "s" } else { "" }
    )
}
fn out_of_order(values: &[Import]) -> String {
    let third = group(values, 1);
    let first = group(values, 2);
    let local = group(values, 3);
    let third_delimiter = if third.is_empty() {
        ""
    } else if !first.is_empty() && !local.is_empty() {
        ", "
    } else if !first.is_empty() || !local.is_empty() {
        " and "
    } else {
        ""
    };
    let first_delimiter = if first.is_empty() {
        String::new()
    } else {
        format!(
            "{}{}",
            if !third.is_empty() && !local.is_empty() {
                ", "
            } else {
                " "
            },
            if local.is_empty() { "" } else { "and " }
        )
    };
    format!("{third}{third_delimiter}{first}{first_delimiter}{local}")
}
impl Checker<'_> {
    pub(super) fn check_import_order(&mut self, body: &[Stmt]) -> Option<String> {
        let mut imports = Imports::default();
        imports.visit_body(body);
        for value in &mut imports.values {
            value.section = if value.package.starts_with('.') {
                3
            } else if super::defaults::fixed().import_standard(&value.package) {
                0
            } else if super::defaults::fixed().import_third_party(&value.package) {
                1
            } else {
                if let Some(error) = permission::failure(self, &value.package) {
                    return Some(error);
                }
                if self
                    .modules
                    .project_module(self.source.project, &value.package)
                    || self.modules.project_native(&value.package)
                {
                    2
                } else {
                    1
                }
            };
        }
        let mut previous = Vec::new();
        for value in &imports.values {
            let wrong: Vec<_> = previous
                .iter()
                .filter(|other: &&Import| other.section > value.section)
                .cloned()
                .collect();
            if !value.nested && !wrong.is_empty() {
                let label = match value.section {
                    0 => "standard",
                    1 => "third party",
                    2 => "first party",
                    _ => "local",
                };
                self.emit(
                    "C0411",
                    "wrong-import-order",
                    &format!(
                        "{label} import \"{}\" should be placed before {}",
                        value.display,
                        out_of_order(&wrong)
                    ),
                    value.range,
                );
            }
            let line = ruff_source_file::LineIndex::from_source_text(&self.source.text)
                .line_column(value.range.start(), &self.source.text)
                .line
                .get() as u32;
            if !value.nested
                && value.section > 0
                && !super::directives::suppressed(
                    &self.directives,
                    "C0411",
                    "wrong-import-order",
                    line,
                )
            {
                previous.push(value.clone());
            }
        }
        let mut seen_import = BTreeSet::new();
        let mut seen_from = BTreeSet::new();
        let mut previous = None;
        let mut values: Vec<_> = imports.values.iter().collect();
        values.sort_by_key(|value| {
            if value.section == 0 {
                0
            } else if value.section == 3 {
                2
            } else {
                1
            }
        });
        for value in values {
            let seen = if value.from {
                &mut seen_from
            } else {
                &mut seen_import
            };
            if previous
                .as_ref()
                .is_some_and(|previous| previous != &value.package)
                && seen.contains(&value.package)
                && !value.exempt
            {
                self.emit(
                    "C0412",
                    "ungrouped-imports",
                    &format!("Imports from package {} are not grouped", value.package),
                    value.range,
                );
            }
            previous = Some(value.package.clone());
            let line = ruff_source_file::LineIndex::from_source_text(&self.source.text)
                .line_column(value.range.start(), &self.source.text)
                .line
                .get() as u32;
            if !super::directives::suppressed(&self.directives, "C0412", "ungrouped-imports", line)
            {
                seen.insert(value.package.clone());
            }
        }
        None
    }
}
