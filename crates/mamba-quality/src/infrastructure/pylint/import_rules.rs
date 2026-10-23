//! Import entry checks retain original statement ranges and checker order.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Stmt,
};
use ruff_text_size::Ranged;

struct ImportModule<'a> {
    range: ruff_text_size::TextRange,
    name: Option<&'a str>,
    relative: bool,
}
impl<'a> Visitor<'a> for ImportModule<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if let Stmt::ImportFrom(value) = statement {
            if value.range() == self.range {
                self.name = Some(value.module.as_ref().map_or("", |value| value.as_str()));
                self.relative = value.level > 0;
                return;
            }
        }
        walk_stmt(self, statement);
    }
}
impl Checker<'_> {
    pub(super) fn check_import_references(
        &mut self,
        imports: &[crate::infrastructure::project::Import],
    ) {
        for import in imports {
            for previous in self.scopes.reimports(import) {
                let line = ruff_source_file::LineIndex::from_source_text(&self.source.text)
                    .line_column(previous.first.range.start(), &self.source.text)
                    .line
                    .get();
                let (rule, symbol, label, name) = if previous.shadowed {
                    (
                        "W0416",
                        "shadowed-import",
                        "Shadowed",
                        import.alias.as_deref().unwrap_or(&import.local),
                    )
                } else {
                    (
                        "W0404",
                        "reimported",
                        "Reimport",
                        import.member.as_deref().unwrap_or(&import.module),
                    )
                };
                self.emit(
                    rule,
                    symbol,
                    &format!("{label} '{name}' (imported line {line})"),
                    import.range,
                );
            }
        }
    }
    pub(super) fn import_message_module(
        &self,
        import: &crate::infrastructure::project::Import,
    ) -> String {
        let document = self
            .modules
            .source_index(self.source.project, self.module_name)
            .and_then(|index| self.index.modules[index].document.as_ref());
        let Some(document) = document else {
            return import.module.clone();
        };
        let mut finder = ImportModule {
            range: import.range,
            name: None,
            relative: false,
        };
        finder.visit_body(&document.parsed.syntax().body);
        finder.name.unwrap_or(&import.module).to_string()
    }
    pub(super) fn relative_import(&self, range: ruff_text_size::TextRange) -> bool {
        let Some(document) = self
            .modules
            .source_index(self.source.project, self.module_name)
            .and_then(|index| self.index.modules[index].document.as_ref())
        else {
            return false;
        };
        let mut finder = ImportModule {
            range,
            name: None,
            relative: false,
        };
        finder.visit_body(&document.parsed.syntax().body);
        finder.relative
    }
    pub(super) fn check_import_module_members(
        &mut self,
        import: &crate::infrastructure::project::Import,
    ) {
        let mut names = import.module.split('.');
        let mut module = names.next().unwrap_or("").to_string();
        let mut rest: Vec<_> = names.collect();
        if let Some(member) = &import.member {
            if member != "*" {
                rest.push(member);
            }
        }
        if !self.modules.exists(self.source.project, &module) {
            return;
        }
        for name in rest {
            let original_kind = (!self.modules.project_module(self.source.project, &module))
                .then(|| {
                    super::stdlib_facts::module(&module, self.modules.target())
                        .and_then(|module| module.members.get(name))
                        .map(|member| member.kind.as_str())
                        .or_else(|| {
                            super::import_members::kind(&module, name, self.modules.target())
                        })
                })
                .flatten();
            if original_kind.is_none() && !self.modules.member(self.source.project, &module, name) {
                self.emit(
                    "E0611",
                    "no-name-in-module",
                    &format!("No name '{name}' in module '{module}'"),
                    import.range,
                );
                return;
            }
            if original_kind.is_some_and(|kind| kind == "import" || kind == "class") {
                return;
            }
            if matches!(
                self.modules.call_target(self.source.project, &module, name),
                Some(super::callables::Target::Imported(_))
            ) {
                return;
            }
            module = format!("{module}.{name}");
            if !self.modules.exists(self.source.project, &module) {
                return;
            }
        }
    }
    pub(super) fn check_import_statement(&mut self, statement: &Stmt) -> bool {
        match statement {
            Stmt::Import(value) => {
                for alias in &value.names {
                    let parts: Vec<_> = alias.name.as_str().split('.').collect();
                    if parts.len() == 2
                        && alias
                            .asname
                            .as_ref()
                            .is_some_and(|name| name.as_str() == parts[1])
                    {
                        self.emit(
                            "R0402",
                            "consider-using-from-import",
                            &format!("Use 'from {} import {}' instead", parts[0], parts[1]),
                            value.range(),
                        );
                    }
                    self.check_deprecated_import(alias.name.as_str(), value.range());
                }
            }
            Stmt::ImportFrom(value) => {
                if value.level > 0 {
                    let imported_package = value.module.as_ref().is_some_and(|name| {
                        self.index.modules.iter().any(|module| {
                            module.project == self.source.project
                                && module.name == name.as_str()
                                && module.is_package
                        })
                    });
                    let depth = (self.module_name.split('.').count()
                        + usize::from(self.is_package))
                    .max(if value.module.is_some() && !imported_package {
                        1
                    } else {
                        2
                    });
                    if value.level as usize >= depth {
                        self.emit(
                            "E0402",
                            "relative-beyond-top-level",
                            "Attempted relative import beyond top-level package",
                            value.range(),
                        );
                        return false;
                    }
                }
                if let Some(module) = &value.module {
                    if module.as_str() == "__future__"
                        && self
                            .statements
                            .contexts
                            .get(&value.start().to_u32())
                            .is_some_and(|context| context.misplaced_future)
                    {
                        self.emit(
                            "W0410",
                            "misplaced-future",
                            "__future__ import is not the first non docstring statement",
                            value.range(),
                        );
                    }
                }
            }
            _ => {}
        }
        true
    }
    pub(super) fn check_deprecated_import(&mut self, name: &str, range: ruff_text_size::TextRange) {
        let minor = match self.modules.target() {
            mamba_language::application::quality_syntax::PythonTarget::Py312 => 12,
            mamba_language::application::quality_syntax::PythonTarget::Py313 => 13,
            mamba_language::application::quality_syntax::PythonTarget::Py314 => 14,
        };
        if super::defaults::fixed().deprecated_module(name, minor) {
            self.emit(
                "W4901",
                "deprecated-module",
                &format!("Deprecated module '{name}'"),
                range,
            );
        }
    }
}

pub(super) fn sys_guard(
    value: &mamba_language::application::quality_syntax::python_ast::Expr,
) -> bool {
    use mamba_language::application::quality_syntax::python_ast::Expr;
    match value {
        Expr::Compare(value) => {
            let left = if let Expr::Subscript(value) = &value.operands[0] {
                value.value.as_ref()
            } else {
                &value.operands[0]
            };
            super::values::path(left).is_some_and(|value| value == ["sys", "version_info"])
        }
        Expr::Attribute(_) => super::values::path(value)
            .is_some_and(|value| value == ["six", "PY2"] || value == ["six", "PY3"]),
        _ => false,
    }
}
pub(super) fn platform_guard(
    value: &mamba_language::application::quality_syntax::python_ast::Expr,
) -> bool {
    use mamba_language::application::quality_syntax::python_ast::Expr;
    let Expr::Compare(value) = value else {
        return false;
    };
    let left = if let Expr::Subscript(value) = &value.operands[0] {
        value.value.as_ref()
    } else {
        &value.operands[0]
    };
    super::values::path(left)
        .is_some_and(|value| value == ["os", "name"] || value == ["sys", "platform"])
}
