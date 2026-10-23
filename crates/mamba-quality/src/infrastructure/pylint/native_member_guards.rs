//! Opaque member checks retain body guards within the current lexical scope.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    ExceptHandler, Expr, ExprAttribute, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeMap;

#[derive(Clone)]
enum Guard {
    Test(Expr),
    Try(bool),
    Suppress(Expr),
}
pub(super) struct Guards {
    guards: BTreeMap<u32, Vec<Guard>>,
    postponed: bool,
    pub(super) functions: std::cell::RefCell<super::function_members::Cache>,
}
struct Scan {
    guards: Vec<Guard>,
    found: BTreeMap<u32, Vec<Guard>>,
}
impl Scan {
    fn scope(&mut self, walk: impl FnOnce(&mut Self)) {
        let previous = self.guards.clone();
        self.guards.retain(|guard| !matches!(guard, Guard::Test(_)));
        walk(self);
        self.guards = previous;
    }
    fn body(&mut self, body: &[Stmt], guard: Option<Guard>) {
        if let Some(guard) = guard {
            self.guards.push(guard);
            self.visit_body(body);
            self.guards.pop();
        } else {
            self.visit_body(body);
        }
    }
}
impl<'a> Visitor<'a> for Scan {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {
                self.scope(|scan| walk_stmt(scan, statement));
            }
            Stmt::If(value) => {
                self.visit_expr(&value.test);
                self.body(&value.body, Some(Guard::Test((*value.test).clone())));
                for clause in &value.elif_else_clauses {
                    if let Some(test) = &clause.test {
                        self.visit_expr(test);
                    }
                    self.body(
                        &clause.body,
                        clause.test.as_ref().map(|value| Guard::Test(value.clone())),
                    );
                }
            }
            Stmt::Try(value) => {
                let catches = value.handlers.iter().any(|handler| match handler {
                    ExceptHandler::ExceptHandler(value) => {
                        value.type_.as_deref().is_some_and(catches)
                    }
                });
                self.body(&value.body, Some(Guard::Try(catches)));
                for handler in &value.handlers {
                    self.guards.push(Guard::Try(false));
                    self.visit_except_handler(handler);
                    self.guards.pop();
                }
                self.body(&value.orelse, Some(Guard::Try(catches)));
                self.body(&value.finalbody, Some(Guard::Try(catches)));
            }
            Stmt::With(value) => {
                let previous = self.guards.len();
                self.guards.extend(
                    value
                        .items
                        .iter()
                        .map(|item| Guard::Suppress(item.context_expr.clone())),
                );
                walk_stmt(self, statement);
                self.guards.truncate(previous);
            }
            _ => walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if matches!(
            expression,
            Expr::Lambda(_)
                | Expr::ListComp(_)
                | Expr::SetComp(_)
                | Expr::DictComp(_)
                | Expr::Generator(_)
        ) {
            self.scope(|scan| walk_expr(scan, expression));
            return;
        }
        if let Expr::Attribute(value) = expression {
            self.found
                .insert(value.start().to_u32(), self.guards.clone());
        }
        if let Expr::If(value) = expression {
            self.visit_expr(&value.test);
            self.guards.push(Guard::Test((*value.test).clone()));
            self.visit_expr(&value.body);
            self.guards.pop();
            self.visit_expr(&value.orelse);
        } else {
            walk_expr(self, expression);
        }
    }
}
impl Guards {
    pub(super) fn new(body: &[Stmt]) -> Self {
        let mut scan = Scan {
            guards: Vec::new(),
            found: BTreeMap::new(),
        };
        scan.visit_body(body);
        let postponed = body.iter().any(|statement| {
            matches!(statement, Stmt::ImportFrom(value) if value.module.as_ref().is_some_and(|name| name.as_str() == "__future__") && value.names.iter().any(|name| name.name.as_str() == "annotations"))
        });
        Self {
            guards: scan.found,
            postponed,
            functions: std::cell::RefCell::default(),
        }
    }
}
impl Checker<'_> {
    pub(super) fn native_member_guarded(&self, attribute: &ExprAttribute) -> bool {
        if self.annotation_context
            && (self.native_member_guards.postponed
                || self.modules.target()
                    == mamba_language::application::quality_syntax::PythonTarget::Py314)
        {
            return true;
        }
        self.native_member_guards
            .guards
            .get(&attribute.start().to_u32())
            .is_some_and(|guards| {
                let caught = guards
                    .iter()
                    .rev()
                    .find_map(|guard| match guard {
                        Guard::Try(value) => Some(*value),
                        _ => None,
                    })
                    .unwrap_or(false);
                caught
                    || guards.iter().any(|guard| match guard {
                        Guard::Try(_) => false,
                        Guard::Test(test) => self
                            .native_guard_constant(test)
                            .is_some_and(|(truth, constant)| constant && !truth),
                        Guard::Suppress(value) => self.native_suppresses_attribute_error(value),
                    })
            })
    }
}

fn catches(expression: &Expr) -> bool {
    struct Names(bool);
    impl<'a> Visitor<'a> for Names {
        fn visit_expr(&mut self, expression: &'a Expr) {
            if matches!(expression, Expr::Name(value) if value.id.as_str() == "AttributeError") {
                self.0 = true;
            } else if !self.0 {
                walk_expr(self, expression);
            }
        }
    }
    let mut names = Names(false);
    names.visit_expr(expression);
    names.0
}
