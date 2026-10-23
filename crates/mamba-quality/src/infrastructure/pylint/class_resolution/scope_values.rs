//! Source scope values retain original constant, container, and method types.
use super::super::{
    callables::{Kind, Parameter, Returns, Target},
    classes::Class,
    resolution::{Context, Resolved},
    values::Value,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, Stmt};
use std::rc::Rc;

impl Checker<'_> {
    pub(in super::super) fn source_class_scope_value(
        &self,
        class: Rc<Class>,
        origin: Context,
        name: &str,
        instance: bool,
    ) -> Option<Resolved> {
        if !self.source_class_scope_member(&class, name, instance) {
            return None;
        }
        let builtin = Context::Module("builtins".into());
        let value = match name {
            "__name__" => Value::String(class.name.clone()),
            "__qualname__" => Value::String(self.source_class_qualified_name(&class)?),
            "__module__" => Value::String(class.identity.0.clone()),
            "__doc__" => self
                .source_class(&class)
                .and_then(|statement| statement.body.first())
                .and_then(|statement| match statement {
                    Stmt::Expr(statement) => match statement.value.as_ref() {
                        Expr::StringLiteral(value) => {
                            Some(Value::String(value.value.to_str().into()))
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .unwrap_or(Value::Scalar("NoneType")),
            "__dict__" => Value::Mapping(Vec::new()),
            "__annotations__" | "__annotate__" => Value::Unknown,
            "__type_params__" => Value::Sequence(
                vec![
                    Value::Unknown;
                    self.source_class(&class)?
                        .type_params
                        .as_ref()
                        .map_or(0, |parameters| parameters.type_params.len())
                ],
                "tuple",
            ),
            "__mro__" => Value::Sequence(
                vec![Value::Unknown; self.lineage(class, origin.clone()).len() + 1],
                "tuple",
            ),
            "__bases__" => Value::Sequence(vec![Value::Unknown; class.bases.len().max(1)], "tuple"),
            "__class__" => {
                return Some(Resolved {
                    target: Target::Class(if instance {
                        class
                    } else {
                        self.modules.builtin_class("type")?
                    }),
                    context: if instance { origin } else { builtin },
                })
            }
            "__call__" => {
                return Some(Resolved {
                    target: Target::Instance(class, origin.clone()),
                    context: origin,
                })
            }
            "mro" | "__subclasses__" => {
                let model = self.modules.builtin_class("type")?;
                let mut signature = model.methods.get(name)?.signature.bound(1, "method");
                signature.result = if name == "mro" {
                    Value::Sequence(
                        vec![Value::Unknown; self.mro(class, origin.clone()).ok()?.len()],
                        "tuple",
                    )
                } else {
                    Value::Sequence(Vec::new(), "list")
                };
                signature.result_expression = None;
                signature.result_expressions.clear();
                signature.returns = Returns::Value;
                return Some(Resolved {
                    target: Target::Function(Rc::new(signature)),
                    context: builtin,
                });
            }
            "__new__" => {
                let model = self.modules.builtin_class("object")?;
                let mut signature = (*model.methods.get(name)?.signature).clone();
                signature.category = "classmethod";
                signature.returns = Returns::Value;
                if instance {
                    signature.parameters.insert(
                        0,
                        Parameter {
                            name: "self".into(),
                            required: true,
                            kind: Kind::Positional,
                            default: None,
                        },
                    );
                    signature.result_class = Some((class.identity.0.clone(), class.name.clone()));
                }
                return Some(Resolved {
                    target: Target::Function(Rc::new(signature)),
                    context: if instance { origin } else { builtin },
                });
            }
            _ => return None,
        };
        Some(Resolved {
            target: Target::Value(Rc::new(value)),
            context: origin,
        })
    }
}
