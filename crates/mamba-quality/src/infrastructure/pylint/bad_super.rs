//! Explicit super arguments retain the original enclosing-class and raw-call rules.
use super::{
    callables::Target,
    classes::{Class, MethodKind},
    resolution::Context,
    scopes::Kind,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, ExprCall, Stmt, StmtFunctionDef,
};
use ruff_text_size::Ranged;
use std::rc::Rc;

struct Calls<'a, 'b> {
    checker: &'a mut Checker<'b>,
    class: Rc<Class>,
    origin: Context,
}
impl<'a> Visitor<'a> for Calls<'_, '_> {
    fn visit_stmt(&mut self, value: &'a Stmt) {
        match value {
            Stmt::ClassDef(_) => return,
            Stmt::FunctionDef(function) => {
                self.checker.scopes.enter(
                    Kind::Function,
                    &function.body,
                    Some(&function.parameters),
                );
                self.visit_body(&function.body);
                self.checker.scopes.leave();
                return;
            }
            _ => {}
        }
        walk_stmt(self, value);
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        if let Expr::Call(call) = value {
            let Expr::Attribute(attribute) = call.func.as_ref() else {
                walk_expr(self, value);
                return;
            };
            if let Expr::Call(call) = attribute.value.as_ref() {
                self.checker
                    .check_super_class(call, &self.class, self.origin.clone());
            }
        }
        walk_expr(self, value);
    }
}
impl Checker<'_> {
    fn check_super_class(&mut self, call: &ExprCall, class: &Rc<Class>, origin: Context) {
        if !matches!(call.func.as_ref(),Expr::Name(name) if name.id.as_str()=="super") {
            return;
        }
        let Some(first) = call.arguments.args.first() else {
            return;
        };
        let special = if matches!(first,Expr::Call(call) if matches!(call.func.as_ref(),Expr::Name(name) if name.id.as_str()=="type"))
        {
            Some("type")
        } else if matches!(first,Expr::Attribute(attribute) if attribute.attr.as_str()=="__class__")
            && matches!(call.arguments.args.get(1),Some(Expr::Name(name)) if name.id.as_str()=="self")
        {
            Some("self.__class__")
        } else {
            None
        };
        let name = if let Some(name) = special {
            Some(name.to_string())
        } else {
            let inferred = self.expression_target(first);
            match inferred.map(|value| value.target) {
                Some(Target::Class(inferred)) => {
                    if self
                        .lineage(class.clone(), origin)
                        .iter()
                        .any(|(class, _)| class.identity == inferred.identity)
                    {
                        return;
                    }
                    Some(inferred.name.clone())
                }
                Some(Target::Function(function)) => Some(function.name.clone()),
                Some(Target::Module(module)) => Some(module),
                _ => match first {
                    Expr::Name(name) if self.scopes.declared(name.id.as_str()) => {
                        Some(name.id.to_string())
                    }
                    _ => None,
                },
            }
        };
        if let Some(name) = name {
            let name = super::string_format::python_string_repr(&name);
            self.emit(
                "E1003",
                "bad-super-call",
                &format!("Bad first argument {name} given to super()"),
                call.range(),
            );
        }
    }
    pub(super) fn check_bad_super(
        &mut self,
        function: &StmtFunctionDef,
        method: Option<MethodKind>,
    ) {
        if method.is_none() {
            return;
        }
        let Some((class, origin)) = self.class_stack.last().cloned() else {
            return;
        };
        Calls {
            checker: self,
            class,
            origin,
        }
        .visit_body(&function.body);
    }
}
