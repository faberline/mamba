//! Basic semantic diagnostics retain declaration timing and lexical frames.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, ExprContext, Stmt, StmtFunctionDef, UnaryOp,
};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct FunctionData {
    returned_value: bool,
    yields: Vec<TextRange>,
    yield_from: Vec<TextRange>,
    globals: BTreeMap<String, TextRange>,
    nonlocals: BTreeSet<String>,
    names: Vec<(String, TextRange)>,
}
impl<'a> Visitor<'a> for FunctionData {
    fn visit_stmt(&mut self, value: &'a Stmt) {
        match value {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => return,
            Stmt::Return(value) => {
                self.returned_value |= value
                    .value
                    .as_deref()
                    .is_some_and(|value| !matches!(value, Expr::NoneLiteral(_)))
            }
            Stmt::Global(value) => {
                for name in &value.names {
                    self.globals.insert(name.to_string(), value.range());
                }
            }
            Stmt::Nonlocal(value) => self
                .nonlocals
                .extend(value.names.iter().map(ToString::to_string)),
            _ => {}
        }
        walk_stmt(self, value);
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        match value {
            Expr::Lambda(_) => return,
            Expr::Yield(_) => self.yields.push(value.range()),
            Expr::YieldFrom(_) => {
                self.yields.push(value.range());
                self.yield_from.push(value.range());
            }
            Expr::Name(value) if value.ctx == ExprContext::Load => {
                self.names.push((value.id.to_string(), value.range()))
            }
            _ => {}
        }
        walk_expr(self, value);
    }
}

fn too_many_starred(value: &Expr) -> bool {
    let Expr::Tuple(tuple) = value else {
        return false;
    };
    let mut stars = 0;
    for value in &tuple.elts {
        match value {
            Expr::Tuple(_) => return too_many_starred(value),
            Expr::Starred(_) => stars += 1,
            _ => {}
        }
    }
    stars > 1
}

impl Checker<'_> {
    pub(super) fn check_function_semantics(&mut self, function: &StmtFunctionDef) {
        let mut data = FunctionData::default();
        data.visit_body(&function.body);
        let range = self.declaration_range(function.name.range());
        if function.is_async {
            for range in &data.yield_from {
                self.emit(
                    "E1700",
                    "yield-inside-async-function",
                    "Yield inside async function",
                    *range,
                );
            }
        }
        for name in data
            .nonlocals
            .intersection(&data.globals.keys().cloned().collect())
        {
            self.emit(
                "E0115",
                "nonlocal-and-global",
                &format!("Name '{name}' is nonlocal and global"),
                range,
            );
        }
        let index = LineIndex::from_source_text(&self.source.text);
        for (name, range) in &data.names {
            if let Some(declaration) = data.globals.get(name) {
                if index
                    .line_column(declaration.start(), &self.source.text)
                    .line
                    > index.line_column(range.start(), &self.source.text).line
                {
                    self.emit(
                        "E0118",
                        "used-prior-global-declaration",
                        &format!("Name '{name}' is used prior to global declaration"),
                        *range,
                    );
                }
            }
        }
        if self.scopes.in_class() && function.name.as_str() == "__init__" {
            if !data.yields.is_empty() {
                self.emit(
                    "E0100",
                    "init-is-generator",
                    "__init__ method is a generator",
                    range,
                );
            } else if data.returned_value {
                self.emit(
                    "E0101",
                    "return-in-init",
                    "Explicit return in __init__",
                    range,
                );
            }
        }
        let mut parameters = BTreeSet::new();
        for parameter in function.parameters.iter() {
            if !parameters.insert(parameter.name().to_string()) {
                self.emit(
                    "E0108",
                    "duplicate-argument-name",
                    &format!(
                        "Duplicate argument name '{}' in function definition",
                        parameter.name()
                    ),
                    parameter.name().range(),
                );
            }
        }
    }

    pub(super) fn check_semantic_statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Return(_) if !self.scopes.in_function() => self.emit(
                "E0104",
                "return-outside-function",
                "Return outside function",
                statement.range(),
            ),
            Stmt::Nonlocal(value) => {
                for name in &value.names {
                    if !self.scopes.has_nonlocal_binding(name.as_str()) {
                        self.emit(
                            "E0117",
                            "nonlocal-without-binding",
                            &format!("nonlocal name {name} found without binding"),
                            value.range(),
                        );
                    }
                }
            }
            Stmt::Assign(value) => {
                if let Some(first) = value.targets.first() {
                    if matches!(first, Expr::Starred(_)) {
                        self.emit(
                            "E0113",
                            "invalid-star-assignment-target",
                            "Starred assignment target must be in a list or tuple",
                            value.range(),
                        );
                    } else if too_many_starred(first) {
                        self.emit(
                            "E0112",
                            "too-many-star-expressions",
                            "More than one starred expression in assignment",
                            value.range(),
                        );
                    }
                }
                if let Expr::Starred(starred) = value.value.as_ref() {
                    self.emit(
                        "E0114",
                        "star-needs-assignment-target",
                        "Can use starred expression only in assignment target",
                        starred.range(),
                    );
                }
            }
            _ => {}
        }
    }

    pub(super) fn check_semantic_expression(&mut self, value: &Expr) {
        match value {
            Expr::Yield(_) | Expr::YieldFrom(_) if !self.scopes.in_function() => self.emit(
                "E0105",
                "yield-outside-function",
                "Yield outside function",
                value.range(),
            ),
            Expr::UnaryOp(unary) => {
                if let Expr::UnaryOp(inner) = unary.operand.as_ref() {
                    if unary.op == inner.op
                        && matches!(unary.op, UnaryOp::UAdd | UnaryOp::USub)
                        && inner.start().to_u32() == unary.start().to_u32() + 1
                    {
                        self.emit(
                            "E0107",
                            "nonexistent-operator",
                            &format!("Use of the non-existent {}{} operator", unary.op, unary.op),
                            unary.range(),
                        );
                    } else if unary.op == UnaryOp::Not && inner.op == UnaryOp::Not {
                        let before = super::display::expression(value, &self.source.text);
                        let after = super::display::expression(&inner.operand, &self.source.text);
                        self.emit(
                            "C0117",
                            "unnecessary-negation",
                            &format!("Consider changing \"{before}\" to \"{after}\""),
                            unary.range(),
                        );
                    }
                }
            }
            _ => {}
        }
    }
}
