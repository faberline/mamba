//! Private function uses retain scope so recursive method calls do not count.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, Stmt, StmtClassDef, StmtFunctionDef,
};
use ruff_text_size::{Ranged, TextSize};
use std::collections::BTreeSet;

#[derive(Default)]
struct Uses<'a> {
    class: &'a str,
    names: BTreeSet<String>,
    attributes: Vec<(String, Option<TextSize>)>,
    scope: Option<TextSize>,
    path: Vec<String>,
    functions: Vec<(String, &'a StmtFunctionDef)>,
}

impl<'a> Visitor<'a> for Uses<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if let Stmt::FunctionDef(function) = statement {
            if function.name.starts_with("__") && !function.name.ends_with("__") {
                self.functions.push((self.path.join("."), function));
            }
            let before = self.scope.replace(function.start());
            self.path.push(function.name.to_string());
            walk_stmt(self, statement);
            self.path.pop();
            self.scope = before;
        } else {
            walk_stmt(self, statement);
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Name(value) if value.ctx.is_load() || value.ctx.is_del() => {
                self.names.insert(value.id.to_string());
            }
            Expr::Attribute(value) if value.ctx.is_load() || value.ctx.is_del() => {
                let receiver = matches!(value.value.as_ref(), Expr::Name(value) if matches!(value.id.as_str(), "self" | "cls") || value.id.as_str() == self.class)
                    || matches!(value.value.as_ref(), Expr::Call(call) if matches!(call.func.as_ref(), Expr::Name(value) if value.id.as_str() == "type") && matches!(call.arguments.args.first(), Some(Expr::Name(value)) if value.id.as_str() == "self"));
                if receiver {
                    self.attributes.push((value.attr.to_string(), self.scope));
                }
            }
            _ => {}
        }
        walk_expr(self, expression);
    }
}

impl Checker<'_> {
    pub(super) fn check_private_methods(&mut self, class: &StmtClassDef) {
        let mut uses = Uses {
            class: class.name.as_str(),
            ..Uses::default()
        };
        uses.visit_body(&class.body);
        for (path, function) in uses.functions {
            if uses.names.contains(function.name.as_str())
                || uses.attributes.iter().any(|(name, scope)| {
                    name == function.name.as_str() && *scope != Some(function.start())
                })
            {
                continue;
            }
            let parameters =
                super::display::function_parameters(&function.parameters, &self.source.text);
            let name = if path.is_empty() {
                function.name.to_string()
            } else {
                format!("{path}.{}", function.name)
            };
            self.emit(
                "W0238",
                "unused-private-member",
                &format!(
                    "Unused private member `{}.{name}({parameters})`",
                    class.name
                ),
                self.declaration_range(function.name.range()),
            );
        }
    }
}
