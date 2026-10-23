//! Builtin metadata retains the isolated original constant and container shapes.
use super::super::{
    callables::{Returns, Target},
    classes::Class,
    resolution::{Context, Resolved},
    values::Value,
    Checker,
};
use std::rc::Rc;

impl Checker<'_> {
    pub(in super::super) fn builtin_class_scope_value(
        &self,
        class: &Class,
        name: &str,
        instance: bool,
    ) -> Option<Resolved> {
        if class.identity.0 != "builtins"
            || (instance && !matches!(name, "__qualname__" | "__module__"))
        {
            return None;
        }
        let model = super::builtin_scope::class(&class.name, self.modules.target())?;
        if name == "mro" {
            let prototype = self.modules.builtin_class("type")?;
            let method = &prototype.methods.get(name)?.signature;
            let mut signature = if class.name == "type" {
                (**method).clone()
            } else {
                method.bound(1, "method")
            };
            signature.result = if class.name == "type" {
                Value::Unknown
            } else {
                Value::Sequence(vec![Value::Unknown; model.mro_names.len()], "tuple")
            };
            signature.result_expression = None;
            signature.result_expressions.clear();
            signature.result_class = None;
            signature.returns = Returns::Value;
            return Some(Resolved {
                target: Target::Function(Rc::new(signature)),
                context: Context::Module("builtins".into()),
            });
        }
        let attribute = model.attributes.get(name)?;
        if !attribute.available {
            return None;
        }
        let [node] = attribute.values.as_slice() else {
            return None;
        };
        let value = match node.get("kind")?.as_str()? {
            "Const" => match &node["value"] {
                serde_json::Value::String(value) => Value::String(value.clone()),
                serde_json::Value::Null => Value::Scalar("NoneType"),
                serde_json::Value::Bool(value) => Value::Boolean(*value),
                _ => Value::Unknown,
            },
            "Tuple" => Value::Sequence(
                vec![Value::Unknown; node["elements"].as_array()?.len()],
                "tuple",
            ),
            "List" => Value::Sequence(
                vec![Value::Unknown; node["elements"].as_array()?.len()],
                "list",
            ),
            "Dict" => Value::Mapping(Vec::new()),
            _ => Value::Unknown,
        };
        Some(Resolved {
            target: Target::Value(Rc::new(value)),
            context: Context::Module("builtins".into()),
        })
    }
}
