//! Global and nonlocal declarations apply throughout their enclosing suite.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_comprehension, walk_except_handler, walk_expr, walk_stmt, Visitor},
    Comprehension, ExceptHandler,
};
use std::collections::BTreeSet;

#[derive(Default)]
struct Declarations {
    globals: BTreeSet<String>,
    nonlocals: BTreeSet<String>,
    exception_aliases: BTreeMap<String, TextSize>,
    collect_values: bool,
    value_writes: BTreeMap<String, Option<TextSize>>,
    unsupported_values: bool,
}

impl Declarations {
    fn value_write(&mut self, name: &str, location: TextSize) {
        self.value_writes
            .entry(name.to_owned())
            .and_modify(|previous| {
                if *previous != Some(location) {
                    *previous = None;
                }
            })
            .or_insert(Some(location));
    }
}

impl<'a> Visitor<'a> for Declarations {
    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(value) = handler;
        if let Some(name) = &value.name {
            self.exception_aliases
                .entry(name.id.to_string())
                .or_insert(name.start());
        }
        walk_except_handler(self, handler);
    }

    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Global(statement) => {
                self.globals
                    .extend(statement.names.iter().map(ToString::to_string));
            }
            Stmt::Nonlocal(statement) => {
                self.nonlocals
                    .extend(statement.names.iter().map(ToString::to_string));
            }
            Stmt::FunctionDef(function) if self.collect_values => {
                self.value_write(function.name.as_str(), function.name.start());
                for decorator in &function.decorator_list {
                    self.visit_expr(&decorator.expression);
                }
                self.visit_parameters(&function.parameters);
                if let Some(returns) = &function.returns {
                    self.visit_expr(returns);
                }
            }
            Stmt::ClassDef(class) if self.collect_values => {
                self.value_write(class.name.as_str(), class.name.start());
                for decorator in &class.decorator_list {
                    self.visit_expr(&decorator.expression);
                }
                if let Some(arguments) = &class.arguments {
                    self.visit_arguments(arguments);
                }
            }
            Stmt::Match(_) if self.collect_values => self.unsupported_values = true,
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, expression: &'a Expr) {
        if !self.collect_values {
            walk_expr(self, expression);
            return;
        }
        match expression {
            Expr::Name(name) if !name.ctx.is_load() => {
                self.value_write(name.id.as_str(), name.start())
            }
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    for parameter in parameters.iter_non_variadic_params() {
                        if let Some(default) = parameter.default.as_deref() {
                            self.visit_expr(default);
                        }
                    }
                }
            }
            _ => walk_expr(self, expression),
        }
    }

    fn visit_comprehension(&mut self, value: &'a Comprehension) {
        if !self.collect_values {
            walk_comprehension(self, value);
            return;
        }
        // Targets belong to the comprehension scope; named expressions in
        // its value/filter can write the enclosing owner and are still walked.
        self.visit_expr(&value.iter);
        for filter in &value.ifs {
            self.visit_expr(filter);
        }
    }
}

impl Scopes {
    // This is a set of lexical writes, not an execution-order plan. A sole
    // source location only permits an already proved alias's own declaration.
    pub(super) fn annotation_value_writes(
        &self,
        body: &[Stmt],
    ) -> Option<BTreeMap<String, Option<TextSize>>> {
        let mut declarations = Declarations {
            collect_values: true,
            ..Default::default()
        };
        for statement in body {
            declarations.visit_stmt(statement);
        }
        for (name, location) in declarations.exception_aliases.clone() {
            declarations.value_write(&name, location);
        }
        (!declarations.unsupported_values).then_some(declarations.value_writes)
    }
    pub(super) fn predeclare_exception_aliases(&mut self, body: &[Stmt]) {
        let mut declarations = Declarations::default();
        for statement in body {
            declarations.visit_stmt(statement);
        }
        for (name, first_definition) in declarations.exception_aliases {
            let index = self.binding_scope(&name);
            if index + 1 != self.stack.len() {
                continue;
            }
            self.stack[index].bindings.entry(name).or_insert(Binding {
                ty: None,
                first_definition,
                infer_from_assignment: true,
                callable: None,
                module: None,
                explicit: true,
                module_hidden: Some(false),
                annotation_form: None,
                annotation_module: None,
            });
        }
    }

    pub(super) fn redirects(&self, body: &[Stmt]) -> BTreeMap<String, usize> {
        let mut declarations = Declarations::default();
        for statement in body {
            declarations.visit_stmt(statement);
        }
        let mut redirects: BTreeMap<_, _> = declarations
            .globals
            .into_iter()
            .map(|name| (name, 0))
            .collect();
        for name in declarations.nonlocals {
            if let Some(index) = self
                .stack
                .iter()
                .enumerate()
                .rev()
                .find_map(|(index, scope)| {
                    (scope.kind == Kind::Function && scope.bindings.contains_key(&name))
                        .then_some(index)
                })
            {
                redirects.insert(name, index);
            }
        }
        redirects
    }
}
