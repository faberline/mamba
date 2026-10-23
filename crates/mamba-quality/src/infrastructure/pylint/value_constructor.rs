//! Constructor inference retains original builtin fallback and unknown results.
use super::{resolution::Context, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall};

pub(super) enum Origin {
    Builtin,
    Synthetic,
    Source(Context),
    Unknown,
}

impl Checker<'_> {
    pub(super) fn constructor_value_origin(
        &self,
        call: &ExprCall,
        class: &str,
        context: Context,
    ) -> Origin {
        match class {
            "int" | "str" => {
                let arguments = super::arguments::Arguments::from_call(self, call);
                if !arguments.keywords.is_empty() || arguments.invalid {
                    return Origin::Builtin;
                }
                if class == "int" {
                    if let Some(value) = call.arguments.args.first() {
                        if self.expression_target_in(value, context).is_none() {
                            return Origin::Builtin;
                        }
                    }
                }
                Origin::Synthetic
            }
            "bool" => {
                if call.arguments.args.len() > 1 {
                    return Origin::Builtin;
                }
                if let Some(value) = call.arguments.args.first() {
                    let value = self.slot_scalar(value, context);
                    if matches!(value, Value::Unknown) {
                        return Origin::Unknown;
                    }
                }
                Origin::Synthetic
            }
            "list" | "tuple" | "set" | "frozenset" => {
                let [] = call.arguments.args.as_ref() else {
                    let [argument] = call.arguments.args.as_ref() else {
                        return Origin::Builtin;
                    };
                    if matches!(argument, Expr::Starred(_)) {
                        return Origin::Builtin;
                    }
                    if let Some(value) = self.unpacked_value(argument, context.clone()) {
                        if value.elements.is_some() {
                            return Origin::Source(if value.kind == class {
                                value.origin
                            } else {
                                context
                            });
                        }
                    }
                    return if match self.slot_scalar(argument, context.clone()) {
                        Value::String(value) => value.chars().count() <= 100_000_000,
                        Value::Bytes(value) => value.len() <= 100_000_000,
                        _ => false,
                    } {
                        Origin::Source(context)
                    } else {
                        Origin::Builtin
                    };
                };
                Origin::Source(context)
            }
            _ => Origin::Builtin,
        }
    }
}
