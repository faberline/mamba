//! Collection mutations follow the original recursive for-body scan.
use super::{resolution::Context, scopes::Kind, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt, StmtFor,
};
use ruff_text_size::{Ranged, TextRange};

#[derive(PartialEq, Eq)]
struct Collection {
    kind: &'static str,
    origin: Context,
    range: TextRange,
}
struct Scan<'a, 'b, 's> {
    checker: &'b mut Checker<'s>,
    loop_: &'a StmtFor,
    origin: Context,
}
impl<'a> Visitor<'a> for Scan<'a, '_, '_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        self.checker
            .check_collection_mutation(statement, self.loop_, self.origin.clone());
        match statement {
            Stmt::FunctionDef(function) => {
                self.checker.scopes.enter(
                    Kind::Function,
                    &function.body,
                    Some(&function.parameters),
                );
                self.visit_body(&function.body);
                self.checker.scopes.leave();
            }
            Stmt::ClassDef(class) => {
                self.checker.scopes.enter(Kind::Class, &class.body, None);
                self.visit_body(&class.body);
                self.checker.scopes.leave();
            }
            _ => walk_stmt(self, statement),
        }
    }
}
fn receiver_name(expression: &Expr) -> Option<&str> {
    match expression {
        Expr::Name(name) => Some(name.id.as_str()),
        Expr::Attribute(attribute) => Some(attribute.attr.as_str()),
        _ => None,
    }
}
impl Checker<'_> {
    fn mutation_collection(&self, expression: &Expr, origin: Context) -> Option<Collection> {
        let nodes = self.operand_nodes(expression, origin);
        let [(value, context)] = nodes.as_slice() else {
            return None;
        };
        let kind = match value {
            Expr::List(_) | Expr::ListComp(_) => "list",
            Expr::Dict(_) | Expr::DictComp(_) => "dict",
            Expr::Set(_) | Expr::SetComp(_) => "set",
            Expr::Call(_) => {
                let unpacked = self.unpacked_value(value, context.clone())?;
                if unpacked.view.is_some() || !matches!(unpacked.kind, "list" | "dict" | "set") {
                    return None;
                }
                unpacked.kind
            }
            _ => return None,
        };
        Some(Collection {
            kind,
            origin: context.clone(),
            range: value.range(),
        })
    }
    pub(super) fn check_modified_iteration(&mut self, loop_: &StmtFor) {
        if loop_.is_async {
            return;
        }
        let origin = Context::Scope(self.scopes.current_index());
        Scan {
            checker: self,
            loop_,
            origin,
        }
        .visit_body(&loop_.body);
    }
    fn check_collection_mutation(&mut self, statement: &Stmt, loop_: &StmtFor, origin: Context) {
        let Some(collection) = self.mutation_collection(&loop_.iter, origin) else {
            return;
        };
        if let Stmt::Delete(deletion) = statement {
            let targets = super::local_rules::target_names(&loop_.target);
            if deletion.targets.iter().any(|target| {
                matches!(target, Expr::Name(name) if targets.iter().any(|target| target == name.id.as_str()))
            }) {
                self.emit_collection_mutation(collection.kind, &loop_.iter, statement.range());
            }
            return;
        }
        let Some(name) = receiver_name(&loop_.iter) else {
            return;
        };
        let origin = Context::Scope(self.scopes.current_index());
        match statement {
            Stmt::Expr(expression) => {
                let Expr::Call(call) = expression.value.as_ref() else {
                    return;
                };
                let Expr::Attribute(attribute) = call.func.as_ref() else {
                    return;
                };
                let Expr::Name(receiver) = attribute.value.as_ref() else {
                    return;
                };
                let modifies = match collection.kind {
                    "list" => matches!(attribute.attr.as_str(), "append" | "remove"),
                    "set" => matches!(
                        attribute.attr.as_str(),
                        "add" | "clear" | "discard" | "pop" | "remove"
                    ),
                    _ => false,
                };
                if modifies
                    && receiver.id.as_str() == name
                    && self.mutation_collection(&attribute.value, origin).as_ref()
                        == Some(&collection)
                {
                    self.emit_collection_mutation(collection.kind, &loop_.iter, statement.range());
                }
            }
            Stmt::Assign(assignment) if collection.kind == "dict" => {
                let Some(Expr::Subscript(subscript)) = assignment.targets.first() else {
                    return;
                };
                let Expr::Name(receiver) = subscript.value.as_ref() else {
                    return;
                };
                if matches!(loop_.iter.as_ref(), Expr::Name(iter) if iter.id.as_str() == receiver.id.as_str())
                    && matches!((loop_.target.as_ref(), subscript.slice.as_ref()), (Expr::Name(target), Expr::Name(index)) if target.id == index.id)
                {
                    return;
                }
                if receiver.id.as_str() == name
                    && self.mutation_collection(&subscript.value, origin).as_ref()
                        == Some(&collection)
                {
                    self.emit_collection_mutation(collection.kind, &loop_.iter, statement.range());
                }
            }
            _ => {}
        }
    }
    fn emit_collection_mutation(&mut self, kind: &str, expression: &Expr, range: TextRange) {
        let (rule, symbol, suffix) = match kind {
            "list" => (
                "W4701",
                "modified-iterating-list",
                "consider iterating through a copy of it instead.",
            ),
            "dict" => (
                "E4702",
                "modified-iterating-dict",
                "iterate through a copy of it instead.",
            ),
            _ => (
                "E4703",
                "modified-iterating-set",
                "iterate through a copy of it instead.",
            ),
        };
        let name = receiver_name(expression).unwrap_or(kind);
        self.emit(
            rule,
            symbol,
            &format!("Iterated {kind} '{name}' is being modified inside for loop body, {suffix}"),
            range,
        );
    }
}
