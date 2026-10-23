use super::*;
use std::collections::BTreeSet;
mod configuration;
mod fields;
mod from_attributes;
pub(super) mod qualifiers;

impl Checker<'_> {
    pub(super) fn called_model(&self, function: &Expr) -> Option<pydantic::Model> {
        let ty = self.infer(function)?;
        if !self.context.is_class_value(ty) {
            return None;
        }
        self.context
            .class(self.context.class_id(ty)?)
            .pydantic
            .clone()
    }

    pub(super) fn check_model_fields(&mut self, class: &quality_syntax::python_ast::StmtClassDef) {
        let Some(model) = self.models.get(class.name.as_str()).cloned() else {
            return;
        };
        self.check_model_configuration(class);
        self.check_model_qualifiers(class);
        self.check_model_field_overrides(class, &model);
        for statement in super::super::classes::statements::collect(&class.body, self.python_target)
        {
            if model.is_root_model {
                if let Stmt::AnnAssign(assignment) = statement {
                    if let Expr::Name(name) = assignment.target.as_ref() {
                        if name.id.as_str() != "root"
                            && model
                                .fields
                                .iter()
                                .any(|field| field.name == name.id.as_str())
                        {
                            self.emit(
                                "pydantic-field",
                                "Only `root` is allowed as a field of a `RootModel`".into(),
                                statement,
                            );
                        }
                    }
                }
            }
            match statement {
                Stmt::AnnAssign(assignment) if !model.accepts_names() => {
                    if let Expr::Name(name) = assignment.target.as_ref() {
                        if model
                            .fields
                            .iter()
                            .any(|field| field.name == name.id.as_str() && field.dynamic_alias)
                        {
                            self.emit(
                                "pydantic-alias",
                                "Required dynamic aliases disallowed".into(),
                                statement,
                            );
                        }
                    }
                }
                Stmt::Assign(assignment)
                    if assignment.targets.iter().any(|target| {
                        matches!(target, Expr::Name(name)
                        if !name.id.as_str().starts_with('_') && name.id.as_str() != "model_config"
                            && !self.inherited_model_class_var(name.id.as_str()))
                    }) =>
                {
                    self.emit(
                        "pydantic-field",
                        "Untyped fields disallowed".into(),
                        statement,
                    );
                }
                _ => {}
            }
        }
    }

    pub(super) fn check_model_call(&mut self, expression: &Expr) {
        let Expr::Call(call) = expression else {
            return;
        };
        let Some(model) = self.called_model(&call.func) else {
            return;
        };
        if self
            .infer(&call.func)
            .is_some_and(|ty| self.context.pydantic_declared_initializer(ty))
        {
            if let Some(signature) = self
                .scopes
                .callable_expression_in(&call.func, &self.environment())
            {
                self.bind_call(call, &signature);
            }
            return;
        }
        let expected_return = self
            .expected_value
            .filter(|(range, _)| *range == call.range())
            .map(|(_, ty)| ty)
            .or_else(|| {
                self.expected_calls
                    .get(&(call.start(), call.end()))
                    .copied()
            });
        let instance = self
            .scopes
            .call_return_context(call, &self.environment(), expected_return);
        if model.is_root_model {
            if let Some(signature) = self
                .scopes
                .callable_expression_in(&call.func, &self.environment())
            {
                self.bind_call(call, &signature);
            }
            return;
        }
        let arguments = &call.arguments;
        let mut provided = BTreeSet::new();
        let mut unexpected = false;
        if !arguments.args.is_empty() {
            self.emit(
                "call-arg",
                format!("Too many positional arguments for \"{}\"", model.name),
                expression,
            );
            unexpected = true;
        }
        for keyword in &arguments.keywords {
            let Some(argument) = &keyword.arg else {
                continue;
            };
            provided.insert(argument.as_str());
            if !model.allows_extra()
                && model
                    .init_fields()
                    .all(|field| field.argument_name(!model.uses_aliases()) != argument.as_str())
            {
                self.emit(
                    "call-arg",
                    format!(
                        "Unexpected keyword argument \"{}\" for \"{}\"",
                        argument, model.name
                    ),
                    expression,
                );
                unexpected = true;
            }
        }
        if !unexpected
            && arguments
                .keywords
                .iter()
                .all(|keyword| keyword.arg.is_some())
        {
            let mut missing = BTreeSet::new();
            let mut bound_names = BTreeSet::new();
            for field in model.init_fields() {
                let name = field.argument_name(!model.uses_aliases());
                let bound = provided.contains(name) && bound_names.insert(name);
                if field.required && !model.optional_init_fields() && !bound && missing.insert(name)
                {
                    self.emit(
                        "call-arg",
                        format!("Missing named argument \"{}\" for \"{}\"", name, model.name),
                        expression,
                    );
                }
            }
        }
        for keyword in &arguments.keywords {
            let Some(argument) = &keyword.arg else {
                continue;
            };
            let Some(field) = model
                .init_fields()
                .find(|field| field.argument_name(!model.uses_aliases()) == argument.as_str())
            else {
                continue;
            };
            let expected = instance
                .and_then(|ty| self.context.raw_class_field(ty, &field.name))
                .and_then(|field| field.ty)
                .or(field.ty);
            if let (Some(expected), Some(actual)) = (
                expected,
                expected.and_then(|expected| self.infer_expected(&keyword.value, expected)),
            ) {
                if !self.context.accepts(expected, actual) {
                    self.emit("arg-type", format!("Argument \"{}\" to \"{}\" has incompatible type \"{}\"; expected \"{}\"",
                        argument, model.name, self.context.name(actual), self.context.name(expected)), &keyword.value);
                }
            }
        }
    }
}
