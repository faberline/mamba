//! Non-ASCII name checks retain the original node categories and ranges.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{Expr, Stmt, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};
impl Checker<'_> {
    pub(super) fn check_non_ascii_name(&mut self, name: &str, label: &str, range: TextRange) {
        if name.is_ascii() {
            return;
        }
        let (rule, symbol, detail) = match label {
            "File" => ("W2402", "non-ascii-file-name", "."),
            "Module" => (
                "C2403",
                "non-ascii-module-import",
                ", use an ASCII-only alias for import.",
            ),
            _ => ("C2401", "non-ascii-name", ", consider renaming it."),
        };
        self.emit(
            rule,
            symbol,
            &format!("{label} name \"{name}\" contains a non-ASCII character{detail}"),
            range,
        );
    }
    pub(super) fn check_non_ascii_function(&mut self, function: &StmtFunctionDef) {
        self.check_non_ascii_name(
            function.name.as_str(),
            "Function",
            self.declaration_range(function.name.range()),
        );
        for parameter in function
            .parameters
            .posonlyargs
            .iter()
            .chain(&function.parameters.args)
            .chain(&function.parameters.kwonlyargs)
        {
            self.check_non_ascii_name(
                parameter.parameter.name.as_str(),
                "Argument",
                parameter.parameter.name.range(),
            );
        }
    }
    pub(super) fn check_non_ascii_import(&mut self, statement: &Stmt) {
        let names = match statement {
            Stmt::Import(value) => &value.names,
            Stmt::ImportFrom(value) => &value.names,
            _ => return,
        };
        for name in names {
            self.check_non_ascii_name(
                name.asname.as_ref().unwrap_or(&name.name).as_str(),
                "Module",
                statement.range(),
            );
        }
    }
    pub(super) fn check_non_ascii_target(&mut self, value: &Expr, direct: bool) {
        match value {
            Expr::Name(value) => {
                let label = if self.scopes.in_class() {
                    "Attribute"
                } else {
                    "Variable"
                };
                if !self.scopes.in_function() || direct {
                    self.check_non_ascii_name(value.id.as_str(), label, value.range());
                }
            }
            Expr::Tuple(value) => {
                for value in &value.elts {
                    self.check_non_ascii_target(value, direct);
                }
            }
            Expr::List(value) => {
                for value in &value.elts {
                    self.check_non_ascii_target(value, direct);
                }
            }
            Expr::Starred(value) => self.check_non_ascii_target(&value.value, direct),
            _ => {}
        }
    }
}
