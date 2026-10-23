//! Class deprecations use import text and exempt direct version-guarded imports.
use super::{deprecated_data, import_rules, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Stmt,
};
use ruff_text_size::{Ranged, TextRange};

struct Guard {
    range: TextRange,
    direct: bool,
    found: bool,
}
impl<'a> Visitor<'a> for Guard {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if statement.range() == self.range {
            self.found = self.direct;
            return;
        }
        let previous = self.direct;
        self.direct = matches!(statement, Stmt::If(value) if import_rules::sys_guard(&value.test));
        walk_stmt(self, statement);
        self.direct = previous;
    }
}

impl Checker<'_> {
    pub(super) fn check_deprecated_classes_import(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Import(value) => {
                for alias in &value.names {
                    if let Some((module, name)) = alias.name.as_str().split_once('.') {
                        self.emit_deprecated_class(module, name, value.range());
                    }
                }
            }
            Stmt::ImportFrom(value) => {
                let document = self
                    .modules
                    .source_index(self.source.project, self.module_name)
                    .and_then(|index| self.index.modules[index].document.as_ref());
                if let Some(document) = document {
                    let mut guard = Guard {
                        range: value.range(),
                        direct: false,
                        found: false,
                    };
                    guard.visit_body(&document.parsed.syntax().body);
                    if guard.found {
                        return;
                    }
                }
                let imports = crate::infrastructure::project::imports(
                    std::slice::from_ref(statement),
                    self.module_name,
                    self.is_package,
                );
                if let Some(import) = imports.first() {
                    if deprecated_data::fixed(self.modules.target())
                        .classes
                        .contains_key(&import.module)
                    {
                        for alias in &value.names {
                            self.emit_deprecated_class(
                                &import.module,
                                alias.name.as_str(),
                                value.range(),
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
