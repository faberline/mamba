//! Local checks retain default message text and original traversal order.
use super::Checker;
use crate::infrastructure::project;
use mamba_language::application::quality_syntax::python_ast::{
    Comprehension, Expr, Stmt, StmtGlobal,
};
use project::symbols::DefinitionKind;
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeSet;

pub(super) fn target_names(expression: &Expr) -> Vec<String> {
    match expression {
        Expr::Name(name) => vec![name.id.to_string()],
        Expr::Tuple(tuple) => tuple.elts.iter().flat_map(target_names).collect(),
        Expr::List(list) => list.elts.iter().flat_map(target_names).collect(),
        Expr::Starred(starred) => target_names(&starred.value),
        _ => Vec::new(),
    }
}

pub(super) fn late_imports(body: &[Stmt]) -> BTreeSet<u32> {
    let mut non_import = false;
    let mut late = BTreeSet::new();
    for (index, statement) in body.iter().enumerate() {
        match statement {
            Stmt::Import(_) | Stmt::ImportFrom(_) if non_import => {
                late.insert(statement.start().to_u32());
            }
            Stmt::Assign(assignment) => {
                non_import |= !assignment.targets.iter().all(|target| matches!(target, Expr::Name(name) if name.id.as_str().starts_with("__") && name.id.as_str().ends_with("__")));
            }
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) | Stmt::For(_) | Stmt::While(_) => {
                non_import = true
            }
            Stmt::Expr(_) if index != 0 || !super::docstring(body) => non_import = true,
            _ => {}
        }
    }
    late
}

impl Checker<'_> {
    pub(super) fn check_builtin_shadowing(&mut self) {
        let shadows: Vec<_> = self
            .scopes
            .declarations()
            .iter()
            .filter(|(name, definition)| {
                let module = match &definition.kind {
                    DefinitionKind::Import(import) => Some(import.module.as_str()),
                    _ => None,
                };
                self.modules.builtin(name)
                    && name.as_str() != "__doc__"
                    && !super::defaults::fixed().builtin_allowed(name, module)
            })
            .map(|(name, definition)| {
                let range = if matches!(definition.kind, DefinitionKind::Class) {
                    self.declaration_range(definition.range)
                } else {
                    definition.range
                };
                (name.clone(), range)
            })
            .collect();
        for (name, range) in shadows {
            self.emit(
                "W0622",
                "redefined-builtin",
                &format!("Redefining built-in '{name}'"),
                range,
            );
        }
    }

    pub(super) fn check_global(&mut self, statement: &StmtGlobal) {
        for name in &statement.names {
            self.check_non_ascii_name(name.as_str(), "Constant", statement.range());
        }
        if self.scopes.is_module() {
            self.emit(
                "W0604",
                "global-at-module-level",
                "Using the global statement at the module level",
                statement.range(),
            );
            return;
        }
        let index = ruff_source_file::LineIndex::from_source_text(&self.source.text);
        let line = index.line_column(statement.start(), &self.source.text).line;
        let mut default_message = true;
        for name in &statement.names {
            let imported = self.scopes.global_import(name.as_str());
            let assigned = self
                .scopes
                .last_assignment(name.as_str())
                .is_some_and(|range| {
                    index.line_column(range.start(), &self.source.text).line > line
                });
            if !assigned && !imported {
                self.emit(
                    "W0602",
                    "global-variable-not-assigned",
                    &format!("Using global for '{name}' but no assignment is done"),
                    statement.range(),
                );
                default_message = false;
            } else if !imported && self.scopes.global(name.as_str()).is_none() {
                self.emit(
                    "W0601",
                    "global-variable-undefined",
                    &format!("Global variable '{name}' undefined at the module level"),
                    statement.range(),
                );
                default_message = false;
            }
        }
        if default_message {
            self.emit(
                "W0603",
                "global-statement",
                "Using the global statement",
                statement.range(),
            );
        }
    }

    pub(super) fn check_same_assignment(&mut self, target: &Expr, value: &Expr) {
        if let (Expr::Name(target), Expr::Name(value)) = (target, value) {
            if target.id.as_str() == value.id.as_str() {
                self.emit(
                    "W0127",
                    "self-assigning-variable",
                    &format!("Assigning the same variable '{}' to itself", target.id),
                    target.range(),
                );
            }
        }
        if let (Expr::Name(_), Expr::Lambda(lambda)) = (target, value) {
            self.emit("C3001", "unnecessary-lambda-assignment", "Lambda expression assigned to a variable. Define a function using the \"def\" keyword instead.", lambda.range());
        }
    }

    pub(super) fn check_comprehension(
        &mut self,
        elt: &Expr,
        generators: &[Comprehension],
        kind: &str,
        range: TextRange,
    ) {
        let [generator] = generators else {
            return;
        };
        if !generator.ifs.is_empty() || generator.is_async {
            return;
        }
        if let (Expr::Name(elt), Expr::Name(target)) = (elt, &generator.target) {
            if elt.id.as_str() == target.id.as_str() {
                let source = super::display::expression(&generator.iter, &self.source.text);
                self.emit(
                    "R1721",
                    "unnecessary-comprehension",
                    &format!("Unnecessary use of a comprehension, use {kind}({source}) instead."),
                    range,
                );
            }
        }
    }

    pub(super) fn check_literal_name(&mut self, target: &Expr, value: &Expr) {
        if let Expr::If(value) = value {
            if let Some((truth, true)) = self.native_guard_constant(&value.test) {
                self.check_literal_name(target, if truth { &value.body } else { &value.orelse });
                return;
            }
        }
        let Expr::Name(target) = target else {
            return;
        };
        if !self.scopes.is_module() || super::defaults::fixed().good(target.id.as_str()) {
            return;
        }
        if matches!(self.callable_target(value), Some(super::callables::Target::Instance(class, origin)) if self.enum_subtype(class.clone(), origin.clone(), "Enum"))
        {
            return;
        }
        let rebound_class = self
            .scopes
            .declarations()
            .get(target.id.as_str())
            .is_some_and(|definition| {
                matches!(
                    definition.kind,
                    crate::infrastructure::project::symbols::DefinitionKind::Class
                )
            });
        let scalar = self.static_value(value).builtin_name().is_some_and(|name| {
            matches!(
                name,
                "int" | "float" | "complex" | "str" | "bytes" | "bool" | "NoneType"
            )
        });
        if scalar && self.check_bad_name(target.id.as_str(), target.range()) {
            return;
        }
        let import_fallback = self
            .index
            .modules
            .iter()
            .find(|module| module.project == self.source.project && module.name == self.module_name)
            .and_then(|module| module.document.as_ref())
            .is_some_and(|document| {
                super::reassignments::import_fallback(
                    &document.parsed.syntax().body,
                    target.id.as_str(),
                    target.start().to_u32(),
                )
            });
        let variable =
            rebound_class || import_fallback || self.reassigned_names.contains(target.id.as_str());
        if variable
            && super::defaults::fixed().valid_name(target.id.as_str(), "const")
            && (self
                .scopes
                .first_expression(target.id.as_str(), self.scopes.current_index())
                .is_some_and(|value| {
                    matches!(
                        self.operand(value),
                        super::operand_inference::Operand::Unknown
                    )
                })
                || self
                    .source_binding(
                        target.id.as_str(),
                        &super::resolution::Context::Scope(self.scopes.current_index()),
                    )
                    .is_some_and(|(value, _)| {
                        matches!(
                            self.operand(&value),
                            super::operand_inference::Operand::Unknown
                        )
                    }))
        {
            return;
        }
        let module_alias = self.module_expression(value).is_some()
            && !super::defaults::fixed().valid_name(target.id.as_str(), "variable");
        let instance = matches!(
            self.callable_target(value),
            Some(super::callables::Target::Instance(_, _))
        ) && !super::defaults::fixed().valid_name(target.id.as_str(), "variable");
        let inferred_collection = variable
            && (matches!(
                self.static_value(value),
                super::values::Value::Sequence(_, _) | super::values::Value::Mapping(_)
            ) || matches!(self.callable_target(value), Some(super::callables::Target::Value(value))
                if matches!(value.as_ref(), super::values::Value::Sequence(_, _) | super::values::Value::Mapping(_))));
        if (scalar
            || inferred_collection
            || instance
            || module_alias
            || matches!(
                value,
                Expr::NumberLiteral(_)
                    | Expr::StringLiteral(_)
                    | Expr::BytesLiteral(_)
                    | Expr::BooleanLiteral(_)
                    | Expr::NoneLiteral(_)
            ))
            && !super::defaults::fixed().valid_name(
                target.id.as_str(),
                if variable { "variable" } else { "const" },
            )
        {
            let (label, style) = if variable {
                ("Variable", "snake_case")
            } else {
                ("Constant", "UPPER_CASE")
            };
            self.emit(
                "C0103",
                "invalid-name",
                &format!(
                    "{label} name \"{}\" doesn't conform to {style} naming style",
                    target.id
                ),
                target.range(),
            );
        }
    }
}
