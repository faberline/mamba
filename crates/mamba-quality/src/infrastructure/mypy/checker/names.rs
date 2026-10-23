//! Unresolved value names recover as Any without introducing a lexical binding.
use super::*;
use quality_syntax::python_ast::ExprName;

impl Checker<'_> {
    pub(super) fn check_name(&mut self, name: &ExprName) {
        if name.ctx.is_load()
            && self
                .scopes
                .binding_location(name.id.as_str())
                .is_some_and(|location| self.untyped_pattern_bindings.contains(&location))
            && !self.environment().contains_key(name.id.as_str())
        {
            self.emit(
                "has-type",
                format!("Cannot determine type of \"{}\"", name.id),
                name,
            );
        }
        if (name.ctx.is_load() || name.ctx.is_del()) && !self.scopes.known_name(name.id.as_str()) {
            let (message, hint) = self.scopes.undefined_name_message(name.id.as_str());
            self.emit_with_hint("name-defined", message, name, hint);
        }
    }

    pub(super) fn check_deleted_argument(&mut self, actual: types::Type, value: &Expr) -> bool {
        let source = self
            .context
            .node(actual)
            .and_then(|node| match node.as_ref() {
                types::Node::Deleted(source) => Some(source.clone()),
                _ => None,
            });
        let Some(source) = source else {
            return false;
        };
        self.emit(
            "misc",
            format!("Trying to read deleted variable \"{source}\""),
            value,
        );
        true
    }

    pub(super) fn check_any_constructor(
        &mut self,
        signature: &signatures::Signature,
        call: &quality_syntax::python_ast::ExprCall,
    ) {
        if signature.returns.is_some_and(|ty| {
            self.context
                .class_id(ty)
                .is_some_and(|id| self.context.class(id).namespace == "typing.Any")
        }) {
            self.emit(
                "misc",
                "Any(...) is no longer supported. Use cast(Any, ...) instead".into(),
                call,
            );
        }
    }

    pub(super) fn check_deleted_overload_arguments(
        &mut self,
        call: &quality_syntax::python_ast::ExprCall,
    ) {
        for value in call
            .arguments
            .args
            .iter()
            .chain(call.arguments.keywords.iter().map(|keyword| &keyword.value))
        {
            if let Some(actual) = self.infer(value) {
                self.check_deleted_argument(actual, value);
            }
        }
    }

    pub(super) fn check_module_global_annotation(
        &mut self,
        assignment: &quality_syntax::python_ast::StmtAnnAssign,
    ) -> bool {
        if !self.scopes.redefined_module_global(&assignment.target) {
            return false;
        }
        let Expr::Name(name) = assignment.target.as_ref() else {
            return false;
        };
        self.emit(
            "no-redef",
            format!(
                "Name \"{}\" already defined (possibly by an import)",
                name.id
            ),
            name,
        );
        true
    }

    pub(super) fn delete_name(&mut self, expression: &Expr) {
        match expression {
            Expr::Name(name) => {
                self.scopes.clear_annotation_form(name.id.as_str());
                self.flow.deleted.insert(name.id.to_string());
            }
            Expr::Tuple(tuple) => {
                for item in &tuple.elts {
                    self.delete_name(item);
                }
            }
            Expr::List(list) => {
                for item in &list.elts {
                    self.delete_name(item);
                }
            }
            _ => {}
        }
    }
}
