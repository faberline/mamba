//! Import entry order is independent of local value and naming checks.
use super::Checker;
use crate::infrastructure::project;
use mamba_language::application::quality_syntax::python_ast::Stmt;
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

impl Checker<'_> {
    pub(super) fn check_imports(&mut self, statement: &Stmt) {
        let imports = project::imports(
            std::slice::from_ref(statement),
            self.module_name,
            self.is_package,
        );
        if matches!(statement, Stmt::Import(_)) {
            self.check_import_references(&imports);
        }
        for import in &imports {
            if import.explicit
                && (!self.is_package || !super::defaults::fixed().allow_package_reexport())
            {
                self.emit(
                    "C0414",
                    "useless-import-alias",
                    "Import alias does not rename original package",
                    import.range,
                );
            }
        }
        if let Stmt::ImportFrom(value) = statement {
            let mut names = BTreeSet::new();
            let mut counts = std::collections::BTreeMap::new();
            for alias in &value.names {
                *counts.entry(alias.name.as_str()).or_insert(0) += 1;
            }
            for alias in &value.names {
                if names.insert(alias.name.as_str()) && counts[alias.name.as_str()] > 1 {
                    let line = ruff_source_file::LineIndex::from_source_text(&self.source.text)
                        .line_column(value.start(), &self.source.text)
                        .line
                        .get();
                    self.emit(
                        "W0404",
                        "reimported",
                        &format!("Reimport '{}' (imported line {line})", alias.name),
                        value.range(),
                    );
                }
            }
            self.check_import_references(&imports);
        }
        if !self.scopes.is_module() {
            let names: Vec<_> = imports
                .iter()
                .map(|import| {
                    import.member.as_ref().map_or_else(
                        || import.module.clone(),
                        |member| format!("{}.{member}", import.module),
                    )
                })
                .collect();
            self.emit(
                "C0415",
                "import-outside-toplevel",
                &format!("Import outside toplevel ({})", names.join(", ")),
                statement.range(),
            );
        }
        if let Stmt::Import(import) = statement {
            if import.names.len() > 1 {
                let names: Vec<_> = import
                    .names
                    .iter()
                    .map(|alias| alias.name.as_str())
                    .collect();
                self.emit(
                    "C0410",
                    "multiple-imports",
                    &format!("Multiple imports on one line ({})", names.join(", ")),
                    statement.range(),
                );
            }
        }
        let relative_valid = self.check_import_statement(statement);
        let missing_deprecated_from = matches!(statement, Stmt::ImportFrom(_))
            && imports
                .first()
                .is_some_and(|import| !self.modules.exists(self.source.project, &import.module));
        if !missing_deprecated_from {
            self.check_deprecated_classes_import(statement);
        }
        if self.late_imports.contains(&statement.start().to_u32()) {
            let names = match statement {
                Stmt::Import(import) => &import.names,
                Stmt::ImportFrom(import) => &import.names,
                _ => unreachable!("import statement"),
            };
            let names: Vec<_> = names
                .iter()
                .map(|alias| {
                    alias.asname.as_ref().map_or_else(
                        || alias.name.to_string(),
                        |name| format!("{} as {name}", alias.name),
                    )
                })
                .collect();
            let names = names.join(", ");
            let text = if let Stmt::ImportFrom(import) = statement {
                format!(
                    "from {}{} import {names}",
                    ".".repeat(import.level as usize),
                    import.module.as_ref().map_or("", |module| module.as_str())
                )
            } else {
                format!("import {names}")
            };
            self.emit(
                "C0413",
                "wrong-import-position",
                &format!("Import \"{text}\" should be placed at the top of the module"),
                statement.range(),
            );
        }
        if matches!(statement, Stmt::ImportFrom(_)) {
            self.check_non_ascii_import(statement);
        }
        let mut imports_seen = BTreeSet::new();
        for import in imports {
            self.scopes.record_import(&import);
            if import.member.as_deref() == Some("*") {
                self.emit(
                    "W0401",
                    "wildcard-import",
                    &format!("Wildcard import {}", import.module),
                    import.range,
                );
                if let Some(names) = self.modules.members(self.source.project, &import.module) {
                    self.scopes.expand_wildcard(&import, names);
                }
            }
            if self.type_checking {
                self.scopes.use_name(&import.local);
            }
            if relative_valid {
                let missing = !self.modules.exists(self.source.project, &import.module);
                if !matches!(statement, Stmt::ImportFrom(_))
                    || !missing
                    || imports_seen.insert(import.module.clone())
                {
                    self.check_import(import);
                }
            }
        }
        if let Stmt::ImportFrom(value) = statement {
            if let Some(module) = &value.module {
                self.check_deprecated_import(module.as_str(), value.range());
            }
        } else {
            self.check_non_ascii_import(statement);
        }
        if missing_deprecated_from {
            self.check_deprecated_classes_import(statement);
        }
    }
}
