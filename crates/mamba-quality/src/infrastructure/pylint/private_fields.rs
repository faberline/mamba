//! Private variable and attribute uses follow the original separate scans.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, ExprAttribute, ExprName, Stmt, StmtClassDef, StmtFunctionDef,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

#[derive(Default)]
struct Returns {
    names: BTreeSet<String>,
}
impl<'a> Visitor<'a> for Returns {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if let Stmt::Return(value) = statement {
            if let Some(Expr::Name(value)) = value.value.as_deref() {
                self.names.insert(value.id.to_string());
            }
        }
        walk_stmt(self, statement);
    }
}

#[derive(Default)]
struct Uses<'a> {
    names: BTreeSet<String>,
    reads: Vec<&'a ExprAttribute>,
    variables: Vec<&'a ExprName>,
    attributes: Vec<(&'a ExprAttribute, BTreeSet<String>)>,
    returned: BTreeSet<String>,
}

fn private(name: &str) -> bool {
    name.starts_with("__") && !name.ends_with("__") && name.chars().any(|value| value != '_')
}

impl Uses<'_> {
    fn constructor_returns(function: &StmtFunctionDef) -> BTreeSet<String> {
        if function.name.as_str() != "__new__" {
            return BTreeSet::new();
        }
        let mut returns = Returns::default();
        returns.visit_body(&function.body);
        returns.names
    }
}

impl<'a> Visitor<'a> for Uses<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if let Stmt::FunctionDef(function) = statement {
            let previous =
                std::mem::replace(&mut self.returned, Self::constructor_returns(function));
            walk_stmt(self, statement);
            self.returned = previous;
        } else {
            walk_stmt(self, statement);
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Name(value) if value.ctx.is_load() || value.ctx.is_del() => {
                self.names.insert(value.id.to_string());
            }
            Expr::Name(value) if value.ctx.is_store() && private(value.id.as_str()) => {
                self.variables.push(value);
            }
            Expr::Attribute(value) if value.ctx.is_load() || value.ctx.is_del() => {
                self.reads.push(value);
            }
            Expr::Attribute(value) if value.ctx.is_store() && private(value.attr.as_str()) => {
                self.attributes.push((value, self.returned.clone()));
            }
            _ => {}
        }
        walk_expr(self, expression);
    }
}

impl Checker<'_> {
    pub(super) fn check_private_fields(&mut self, class: &StmtClassDef) {
        let mut uses = Uses::default();
        uses.visit_body(&class.body);
        for value in &uses.variables {
            if uses.names.contains(value.id.as_str())
                || uses.reads.iter().any(|read| match read.value.as_ref() {
                    Expr::Name(receiver) => {
                        read.attr.as_str() == value.id.as_str()
                            && (matches!(receiver.id.as_str(), "self" | "cls")
                                || receiver.id.as_str() == class.name.as_str())
                    }
                    _ => true,
                })
            {
                continue;
            }
            self.emit(
                "W0238",
                "unused-private-member",
                &format!("Unused private member `{}.{}`", class.name, value.id),
                value.range(),
            );
        }
        for (value, returned) in &uses.attributes {
            let Expr::Name(assigned) = value.value.as_ref() else {
                continue;
            };
            if uses.reads.iter().any(|read| {
                let Expr::Name(receiver) = read.value.as_ref() else {
                    return false;
                };
                read.attr.as_str() == value.attr.as_str()
                    && (((assigned.id.as_str() == "cls"
                        || assigned.id.as_str() == class.name.as_str())
                        && (matches!(receiver.id.as_str(), "cls" | "self")
                            || receiver.id.as_str() == class.name.as_str()))
                        || ((assigned.id.as_str() == "self"
                            || returned.contains(assigned.id.as_str()))
                            && receiver.id.as_str() == "self"))
            }) {
                continue;
            }
            self.emit(
                "W0238",
                "unused-private-member",
                &format!("Unused private member `{}.{}`", class.name, value.attr),
                value.range(),
            );
        }
    }
}
