//! Function checks use complete lexical usage before reporting unused names.
use super::{defaults, docstring, Checker};
use crate::infrastructure::project::symbols::DefinitionKind;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, Stmt, StmtFunctionDef,
};
use ruff_text_size::{Ranged, TextRange};

#[derive(Default)]
struct LocalsCalls {
    offsets: Vec<ruff_text_size::TextSize>,
}
impl<'a> Visitor<'a> for LocalsCalls {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if !matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            walk_stmt(self, statement);
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Call(call) = expression {
            if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "locals") {
                self.offsets.push(call.start());
            }
        }
        if !matches!(expression, Expr::Lambda(_)) {
            walk_expr(self, expression);
        }
    }
}

impl Checker<'_> {
    pub(super) fn check_annotation_names(&mut self, value: &Expr) {
        let Expr::StringLiteral(value) = value else {
            return;
        };
        let Ok(document) = mamba_language::application::quality_syntax::parse(
            value.value.to_str(),
            self.modules.target(),
            true,
        ) else {
            return;
        };
        if !document.is_valid() {
            return;
        }
        struct Names(Vec<String>);
        impl<'a> Visitor<'a> for Names {
            fn visit_expr(&mut self, value: &'a Expr) {
                if let Expr::Name(value) = value {
                    self.0.push(value.id.to_string());
                }
                walk_expr(self, value);
            }
        }
        let [Stmt::Expr(value)] = document.parsed.syntax().body.as_slice() else {
            return;
        };
        let mut names = Names(Vec::new());
        names.visit_expr(&value.value);
        for name in names.0 {
            self.scopes.use_name(&name);
        }
    }
    pub(super) fn check_outer_shadowing(&mut self) {
        let mut shadows: Vec<_> = self
            .scopes
            .declarations()
            .iter()
            .filter_map(|(name, definition)| {
                let outer = self.scopes.outer(name)?;
                (!defaults::fixed().ignored(
                    name,
                    matches!(definition.kind, DefinitionKind::Parameter { .. }),
                ))
                .then_some((name.clone(), definition.range, outer.range.start()))
            })
            .collect();
        shadows.sort_by_key(|(_, range, _)| range.start());
        for (name, range, outer) in shadows {
            let line = ruff_source_file::LineIndex::from_source_text(&self.source.text)
                .line_column(outer, &self.source.text)
                .line
                .get();
            self.emit(
                "W0621",
                "redefined-outer-name",
                &format!("Redefining name '{name}' from outer scope (line {line})"),
                range,
            );
        }
    }

    pub(super) fn check_function_header(&mut self, function: &StmtFunctionDef) {
        for parameter in function.parameters.iter() {
            if self.check_bad_name(parameter.name().as_str(), parameter.name().range()) {
                continue;
            }
            if !defaults::fixed().valid_name(parameter.name().as_str(), "argument") {
                self.emit(
                    "C0103",
                    "invalid-name",
                    &format!(
                        "Argument name \"{}\" doesn't conform to snake_case naming style",
                        parameter.name()
                    ),
                    parameter.name().range(),
                );
            }
        }
        if function.parameters.vararg.is_some()
            && function
                .parameters
                .args
                .iter()
                .any(|parameter| parameter.default.is_some())
        {
            self.emit("W1113", "keyword-arg-before-vararg", &format!("Keyword argument before variable positional arguments list in the definition of {} function", function.name),
                TextRange::new(function.start(), function.name.end()));
        }
    }

    pub(super) fn check_unused_function(
        &mut self,
        function: &StmtFunctionDef,
        receiver: Option<&str>,
    ) {
        let body = &function.body[usize::from(docstring(&function.body))..];
        if self.unused_abstract_method(function) {
            return;
        }
        if !body.is_empty()
            && body
                .iter()
                .all(|statement| matches!(statement, Stmt::Raise(_)))
        {
            return;
        }
        let mut locals = LocalsCalls::default();
        locals.visit_body(body);
        for (name, definition) in self.scopes.unused() {
            if receiver == Some(name.as_str()) {
                continue;
            }
            let argument = matches!(definition.kind, DefinitionKind::Parameter { .. });
            if argument && self.inherited_unused_argument(function, &name) {
                continue;
            }
            if defaults::fixed().ignored(&name, argument)
                || (argument && defaults::fixed().callback(function.name.as_str()))
            {
                continue;
            }
            let possible = !argument
                && locals
                    .offsets
                    .iter()
                    .any(|offset| *offset > definition.range.start());
            let (rule, symbol, message) = if argument {
                (
                    "W0613",
                    "unused-argument",
                    format!("Unused argument '{name}'"),
                )
            } else if possible {
                (
                    "W0641",
                    "possibly-unused-variable",
                    format!("Possibly unused variable '{name}'"),
                )
            } else {
                (
                    "W0612",
                    "unused-variable",
                    format!("Unused variable '{name}'"),
                )
            };
            let variadic = matches!(
                definition.kind,
                DefinitionKind::Parameter { variadic: true }
            );
            let before = self.diagnostics.len();
            let range = if variadic {
                let before = &self.source.text[..function.name.start().to_usize()];
                let line = before.rfind('\n').map_or(0, |offset| offset + 1);
                TextRange::new(
                    ruff_text_size::TextSize::try_from(line).unwrap(),
                    function.name.end(),
                )
            } else if argument {
                function
                    .parameters
                    .posonlyargs
                    .iter()
                    .chain(&function.parameters.args)
                    .chain(&function.parameters.kwonlyargs)
                    .find(|parameter| parameter.parameter.name.as_str() == name)
                    .map_or(definition.range, |parameter| parameter.parameter.range())
            } else if matches!(definition.kind, DefinitionKind::Class) {
                self.declaration_range(definition.range)
            } else {
                definition.range
            };
            self.emit(rule, symbol, &message, range);
            if variadic && self.diagnostics.len() > before {
                let diagnostic = self.diagnostics.last_mut().unwrap();
                diagnostic.location.end_column = None;
                diagnostic.location.end_offset = None;
            }
        }
    }
}
