//! Standard replacement calls retain the instance type and field parameter types.
use super::super::signatures::{Kind, Parameter};
use super::*;
use mamba_language::application::quality_syntax::python_ast::ExprCall;

impl Scopes {
    pub(in crate::infrastructure::mypy) fn native_dataclass_function(
        &self,
        expression: &Expr,
    ) -> Option<Arc<Signature>> {
        if !matches!(expression, Expr::Attribute(_))
            || self
                .module_member(expression)
                .is_some_and(|export| export.callable.is_some())
        {
            return None;
        }
        let form = self.context.typing_form(expression, &self.variables())?;
        self.context.dataclass_function(&form)
    }

    pub(in crate::infrastructure::mypy) fn dataclass_replace_origin(
        &self,
        call: &ExprCall,
        environment: &BTreeMap<String, Type>,
    ) -> Option<String> {
        let actual = self.context.infer_with_calls(
            call.arguments.args.first()?,
            environment,
            &|expression| self.expression_type(expression, environment),
        )?;
        let owner = self.context.replace_owner(actual)?;
        let class = self.context.class(owner);
        (class.namespace != self.namespace).then(|| {
            class
                .namespace
                .split_once(':')
                .map_or(class.namespace.as_str(), |(_, module)| module)
                .to_owned()
        })
    }

    pub(in crate::infrastructure::mypy) fn is_dataclass_replace(&self, call: &ExprCall) -> bool {
        self.context
            .typing_form(&call.func, &self.variables())
            .as_deref()
            == Some("replace")
            && self
                .callable_expression(&call.func)
                .is_none_or(|signature| *signature == *self.context.dataclass_replace_function())
    }

    pub(in crate::infrastructure::mypy) fn dataclass_replace_signature(
        &self,
        call: &ExprCall,
        environment: &BTreeMap<String, Type>,
    ) -> Option<(Signature, bool)> {
        if !self.is_dataclass_replace(call) {
            return None;
        }
        let actual = if let Some(object) = call.arguments.args.first() {
            self.context
                .infer_with_calls(object, environment, &|expression| {
                    self.expression_type(expression, environment)
                })?
        } else {
            Type::Never
        };
        let mut signature = Signature::empty("replace".into(), actual);
        signature.constructor = false;
        if self.context.is_class_value(actual) {
            signature.returns = self
                .context
                .class_constructor(actual)
                .map(|constructor| self.context.callable_type(constructor));
        }
        let mut parameters = vec![Parameter {
            name: "obj".into(),
            ty: Some(actual),
            kind: Kind::PositionalOnly,
            required: true,
        }];
        let invalid = !self.context.replace_bound_valid(actual);
        if let Some(fields) = self.context.replace_fields(actual) {
            signature.owner = Some(self.context.name(actual));
            parameters.extend(fields);
        } else {
            parameters.push(Parameter {
                name: "changes".into(),
                ty: Some(Type::Any),
                kind: Kind::VariadicKeyword,
                required: false,
            });
        }
        signature.parameters = parameters.into();
        Some((signature, invalid))
    }
}
