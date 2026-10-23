//! Builtin value metadata retains constant and container source origins.
use super::{
    callables::Target,
    resolution::{Context, Resolved},
    value_constructor::Origin,
    values::Value,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::Expr;
use std::rc::Rc;

impl Checker<'_> {
    pub(super) fn builtin_value_metadata(
        &self,
        expression: &Expr,
        context: Context,
        name: &str,
    ) -> Option<Resolved> {
        let values = self.builtin_metadata_values(expression, context, name)?;
        let strings: Option<Vec<_>> = values
            .iter()
            .map(|value| match &value.target {
                Target::Value(value) => match value.as_ref() {
                    Value::String(value) => Some(value.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let value = match strings {
            Some(strings) if strings.iter().all(|value| value == &strings[0]) => {
                Value::String(strings[0].clone())
            }
            Some(_) => Value::Scalar("str"),
            None => Value::Unknown,
        };
        Some(Resolved {
            target: Target::Value(Rc::new(value)),
            context: Context::Module("builtins".into()),
        })
    }

    pub(super) fn builtin_metadata_values(
        &self,
        expression: &Expr,
        context: Context,
        name: &str,
    ) -> Option<Vec<Resolved>> {
        if !matches!(name, "__qualname__" | "__module__") {
            return None;
        }
        let nodes = self.metadata_nodes(expression, context);
        if nodes.is_empty() {
            return None;
        }
        nodes
            .iter()
            .map(|(expression, origin)| self.builtin_metadata_node(expression, origin, name))
            .collect()
    }

    fn builtin_metadata_node(
        &self,
        expression: &Expr,
        origin: &Context,
        name: &str,
    ) -> Option<Resolved> {
        let (class, module) = if let Expr::Call(call) = expression {
            let resolved = self.expression_target_in(&call.func, origin.clone())?;
            let Target::Class(class) = resolved.target else {
                return None;
            };
            if class.identity.0 != "builtins" {
                return None;
            }
            let module = match self.constructor_value_origin(call, &class.name, origin.clone()) {
                Origin::Builtin => "builtins".into(),
                Origin::Synthetic => "__astroid_synthetic".into(),
                Origin::Source(context) => self.value_origin_module(&context),
                Origin::Unknown => {
                    return Some(Resolved {
                        target: Target::Value(Rc::new(Value::Unknown)),
                        context: Context::Module("builtins".into()),
                    });
                }
            };
            (class.name.clone(), module)
        } else {
            let value = Value::from_expression(expression);
            let (class, module) = match &value {
                Value::Sequence(_, kind) => (*kind, self.value_origin_module(origin)),
                Value::Mapping(_) => ("dict", "builtins".into()),
                value => (value.builtin_name()?, self.value_origin_module(origin)),
            };
            (class.into(), module)
        };
        if name == "__qualname__" {
            let class = self.modules.builtin_class(&class)?;
            return self.builtin_class_scope_value(&class, name, true);
        }
        Some(Resolved {
            target: Target::Value(Rc::new(Value::String(module))),
            context: Context::Module("builtins".into()),
        })
    }

    fn value_origin_module(&self, context: &Context) -> String {
        match context {
            Context::Module(module) => module.clone(),
            Context::Scope(_) => self.module_name.into(),
        }
    }
}
