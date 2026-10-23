//! Operand inference keeps literal shapes and source aliases for protocol checks.
use super::{callables::Target, classes::Class, resolution::Context, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, Number, UnaryOp};
use ruff_text_size::Ranged;
use std::{collections::BTreeSet, rc::Rc};

pub(super) type Active = BTreeSet<(Context, u32, u32)>;
#[derive(Clone)]
pub(super) enum Operand {
    Literal(&'static str),
    Instance(Rc<Class>, Context),
    Class(Rc<Class>, Context),
    Function(String),
    Generator,
    Unknown,
}
impl Operand {
    pub(super) fn name(&self) -> Option<&str> {
        match self {
            Self::Literal(name) => Some(name),
            Self::Instance(class, _) | Self::Class(class, _) => Some(&class.name),
            Self::Function(name) => Some(name),
            Self::Generator => Some("generator"),
            Self::Unknown => None,
        }
    }
}
impl Checker<'_> {
    pub(super) fn operand(&self, expression: &Expr) -> Operand {
        self.operand_in(
            expression,
            Context::Scope(self.scopes.current_index()),
            &mut Active::new(),
        )
    }
    pub(super) fn operand_alternatives(&self, expression: &Expr) -> Vec<Operand> {
        self.operand_nodes(expression, Context::Scope(self.scopes.current_index()))
            .into_iter()
            .map(|(value, origin)| self.operand_leaf_in(&value, origin, &mut Active::new()))
            .collect()
    }
    pub(super) fn operand_in(
        &self,
        expression: &Expr,
        origin: Context,
        active: &mut Active,
    ) -> Operand {
        let values = self
            .operand_nodes(expression, origin)
            .into_iter()
            .map(|(value, origin)| self.operand_leaf_in(&value, origin, active))
            .collect::<Vec<_>>();
        let Some(first) = values.first() else {
            return Operand::Unknown;
        };
        if values
            .iter()
            .all(|value| operand_type(value) == operand_type(first))
        {
            first.clone()
        } else {
            Operand::Unknown
        }
    }
    pub(super) fn operand_leaf_in(
        &self,
        expression: &Expr,
        origin: Context,
        active: &mut Active,
    ) -> Operand {
        let key = (
            origin.clone(),
            expression.start().to_u32(),
            expression.end().to_u32(),
        );
        if !active.insert(key.clone()) {
            return Operand::Unknown;
        }
        let value = self.operand_step(expression, origin, active);
        active.remove(&key);
        value
    }
    fn operand_step(&self, expression: &Expr, origin: Context, active: &mut Active) -> Operand {
        let literal = match expression {
            Expr::List(_) | Expr::ListComp(_) => Some("list"),
            Expr::Tuple(_) => Some("tuple"),
            Expr::Set(_) | Expr::SetComp(_) => Some("set"),
            Expr::Dict(_) | Expr::DictComp(_) => Some("dict"),
            Expr::StringLiteral(_) => Some("str"),
            Expr::BytesLiteral(_) => Some("bytes"),
            Expr::BooleanLiteral(_) => Some("bool"),
            Expr::NoneLiteral(_) => Some("NoneType"),
            Expr::EllipsisLiteral(_) => Some("ellipsis"),
            Expr::NumberLiteral(value) => Some(match value.value {
                Number::Int(_) => "int",
                Number::Float(_) => "float",
                Number::Complex { .. } => "complex",
            }),
            Expr::Slice(_) => Some("slice"),
            _ => None,
        };
        if let Some(name) = literal {
            return Operand::Literal(name);
        }
        match expression {
            Expr::Generator(_) => return Operand::Generator,
            Expr::BinOp(value) => return self.operand_binary(value, origin, active),
            Expr::UnaryOp(value) => {
                if value.op == UnaryOp::Not {
                    return Operand::Literal("bool");
                }
                let operand = self.operand_in(&value.operand, origin, active);
                if matches!(
                    operand,
                    Operand::Literal("int" | "float" | "complex" | "bool")
                ) {
                    return if matches!(operand, Operand::Literal("bool")) {
                        Operand::Literal("int")
                    } else {
                        operand
                    };
                }
                return Operand::Unknown;
            }
            Expr::Name(name) => {
                if let Context::Scope(index) = &origin {
                    if let Some((owner, values)) =
                        self.scopes
                            .values_before(name.id.as_str(), *index, name.start())
                    {
                        if values.is_empty() {
                            return Operand::Unknown;
                        }
                        let operands: Vec<_> = values
                            .iter()
                            .map(|value| self.operand_in(value, Context::Scope(owner), active))
                            .collect();
                        let first = operands.first().unwrap();
                        return if operands
                            .iter()
                            .all(|value| operand_type(value) == operand_type(first))
                        {
                            first.clone()
                        } else {
                            Operand::Unknown
                        };
                    }
                    if self.scopes.parameter_before_assignment(
                        name.id.as_str(),
                        *index,
                        name.start(),
                    ) {
                        return Operand::Unknown;
                    }
                }
                if let Some((value, context)) = self.source_binding(name.id.as_str(), &origin) {
                    if value.range() != expression.range() || context != origin {
                        return self.operand_in(&value, context, active);
                    }
                }
            }
            Expr::Attribute(attribute) => {
                if let Some(resolved) = self.expression_target_in(&attribute.value, origin.clone())
                {
                    if let Target::Module(module) = resolved.target {
                        if let Some((value, context)) =
                            self.source_binding(attribute.attr.as_str(), &Context::Module(module))
                        {
                            return self.operand_in(&value, context, active);
                        }
                    }
                }
            }
            Expr::Call(call) => {
                if let Expr::Attribute(attribute) = call.func.as_ref() {
                    if matches!(attribute.attr.as_str(), "split" | "rsplit")
                        && matches!(
                            self.operand_in(&attribute.value, origin.clone(), active),
                            Operand::Literal("str" | "bytes")
                        )
                    {
                        // The original built-in method has no inferred result node.
                        return Operand::Unknown;
                    }
                }
                if let Some(resolved) = self.expression_target_in(&call.func, origin.clone()) {
                    match resolved.target {
                        Target::Class(class) if class.identity.0 == "builtins" => {
                            let literal = match class.name.as_str() {
                                "list" => Some("list"),
                                "tuple" => Some("tuple"),
                                "set" => Some("set"),
                                "dict" => Some("dict"),
                                "str" => Some("str"),
                                "bytes" => Some("bytes"),
                                "frozenset" => Some("frozenset"),
                                "int" => Some("int"),
                                "float" => Some("float"),
                                "complex" => Some("complex"),
                                "bool" => Some("bool"),
                                _ => None,
                            };
                            if let Some(name) = literal {
                                return Operand::Literal(name);
                            }
                            return Operand::Instance(class, resolved.context);
                        }
                        Target::Function(function) if function.generates => {
                            return Operand::Generator
                        }
                        Target::Function(function) if function.result_expression.is_some() => {
                            return self.operand_in(
                                function.result_expression.as_ref().unwrap(),
                                resolved.context,
                                active,
                            );
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        let Some(resolved) = self.expression_target_in(expression, origin) else {
            return Operand::Unknown;
        };
        match resolved.target {
            Target::Instance(class, context) => Operand::Instance(class, context),
            Target::Class(class) => Operand::Class(class, resolved.context),
            Target::Function(function) => Operand::Function(function.name.clone()),
            Target::Value(value) => {
                let value =
                    self.expand_value((*value).clone(), &resolved.context, &mut Default::default());
                match value {
                    Value::Sequence(_, name) => Operand::Literal(name),
                    Value::Mapping(_) => Operand::Literal("dict"),
                    Value::String(_) => Operand::Literal("str"),
                    Value::Bytes(_) => Operand::Literal("bytes"),
                    Value::Boolean(_) => Operand::Literal("bool"),
                    Value::Scalar(name) => Operand::Literal(name),
                    Value::Number(Number::Int(_)) => Operand::Literal("int"),
                    Value::Number(Number::Float(_)) => Operand::Literal("float"),
                    Value::Number(Number::Complex { .. }) => Operand::Literal("complex"),
                    _ => Operand::Unknown,
                }
            }
            _ => Operand::Unknown,
        }
    }
    pub(super) fn operand_known_bases(
        &self,
        class: Rc<Class>,
        origin: Context,
        active: &mut BTreeSet<(String, u32)>,
    ) -> bool {
        if class.identity.0 == "builtins" {
            return true;
        }
        if !active.insert(class.identity.clone()) {
            return false;
        }
        let result = class.bases.iter().all(|base| {
            self.declared_base(base, origin.clone())
                .is_some_and(|resolved| {
                    if let Target::Class(base) = resolved.target {
                        self.operand_known_bases(base, resolved.context, active)
                    } else {
                        false
                    }
                })
        });
        active.remove(&class.identity);
        result
    }
}

fn operand_type(value: &Operand) -> Option<String> {
    match value {
        Operand::Literal(name) => Some(format!("builtins.{name}")),
        Operand::Instance(class, _) => Some(format!("{}.{}", class.identity.0, class.name)),
        Operand::Class(_, _) => Some("builtins.type".into()),
        Operand::Function(_) => Some("builtins.function".into()),
        Operand::Generator => Some("generator".into()),
        Operand::Unknown => None,
    }
}
