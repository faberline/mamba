//! Builtin type calls retain the inferred runtime type of their argument.
use super::*;
use mamba_language::application::quality_syntax::python_ast::ExprCall;

impl Scopes {
    pub(super) fn remove_shadowed_builtin_annotations(
        &self,
        body: &[Stmt],
        parameters: &BTreeMap<String, Option<Type>>,
        variables: &mut types::Variables,
    ) {
        let names = parameters
            .keys()
            .map(String::as_str)
            .chain(body.iter().filter_map(|statement| {
                if let Stmt::FunctionDef(function) = statement {
                    Some(function.name.as_str())
                } else {
                    None
                }
            }));
        for name in names {
            if variables
                .get(name)
                .and_then(|&ty| self.context.class_id(ty))
                .is_some_and(|id| self.context.class(id).namespace.starts_with("builtins."))
            {
                variables.remove(name);
            }
        }
    }

    pub(super) fn builtin_value_result(
        &self,
        call: &ExprCall,
        signature: &Signature,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        if signature.name.as_deref() != Some("type")
            || signature.definition_module.as_deref() != Some("builtins")
            || !signature.constructor
            || !call.arguments.keywords.is_empty()
        {
            return None;
        }
        let [value] = call.arguments.args.as_ref() else {
            return None;
        };
        let mut ty = self
            .context
            .infer_with_calls(value, environment, &|value| {
                self.expression_type(value, environment)
            })?;
        if let Some(literal) = self.context.literal_expression(value) {
            if let Some(types::Node::Literal(value)) = self.context.node(literal).as_deref() {
                ty = self
                    .context
                    .intern(types::Node::LiteralValue(value.clone()));
            }
        }
        Some(self.context.intern(types::Node::Container(
            types::Container::TypeObject,
            vec![ty],
        )))
    }
}
