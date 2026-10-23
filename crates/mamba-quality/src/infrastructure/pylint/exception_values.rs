//! Exception inference retains classes, instances, scalar values, and unknown bases.
use super::{callables::Target, classes::Class, resolution::Context, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::Expr;
use std::{collections::BTreeSet, rc::Rc};

pub(super) enum Inferred {
    Class(Rc<Class>, Context, bool),
    Scalar(&'static str),
    Function(String),
    Unknown,
}

impl Checker<'_> {
    pub(super) fn exception_value(&self, value: &Expr) -> Inferred {
        if matches!(value, Expr::Name(name) if name.id.as_str() == "NotImplemented" && !self.scopes.declared(name.id.as_str()))
        {
            return Inferred::Scalar("NotImplementedType");
        }
        if matches!(value, Expr::EllipsisLiteral(_)) {
            return Inferred::Scalar("ellipsis");
        }
        if let Expr::Call(call) = value {
            if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "type" && !self.scopes.declared(name.id.as_str()))
                && call.arguments.args.len() == 1
                && matches!(
                    self.exception_value(&call.arguments.args[0]),
                    Inferred::Unknown
                )
            {
                return Inferred::Unknown;
            }
            if call.arguments.is_empty() {
                if let Expr::Name(name) = call.func.as_ref() {
                    if let Some(resolved) = self.expression_target(&call.func) {
                        if let Target::Class(class) = resolved.target {
                            if class.identity.0 == "builtins" && class.name == name.id.as_str() {
                                let scalar = match class.name.as_str() {
                                    "int" => Some("int"),
                                    "float" => Some("float"),
                                    "complex" => Some("complex"),
                                    "str" => Some("str"),
                                    "bytes" => Some("bytes"),
                                    "bool" => Some("bool"),
                                    _ => None,
                                };
                                if let Some(scalar) = scalar {
                                    return Inferred::Scalar(scalar);
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(resolved) = self.expression_target(value) {
            match resolved.target {
                Target::Class(class) => return Inferred::Class(class, resolved.context, false),
                Target::Instance(class, context) => return Inferred::Class(class, context, true),
                Target::Function(signature) => return Inferred::Function(signature.name.clone()),
                _ => {}
            }
        }
        let name = match self.static_value(value) {
            Value::Scalar(name) => Some(name),
            Value::String(_) => Some("str"),
            Value::Bytes(_) => Some("bytes"),
            Value::Number(number) => Some(match number {
                mamba_language::application::quality_syntax::python_ast::Number::Int(_) => "int",
                mamba_language::application::quality_syntax::python_ast::Number::Float(_) => {
                    "float"
                }
                _ => "complex",
            }),
            Value::Boolean(_) => Some("bool"),
            Value::Sequence(_, name) => Some(name),
            Value::Mapping(_) => Some("dict"),
            _ => None,
        };
        if let Some(name) = name {
            return Inferred::Scalar(name);
        }
        if let Expr::Name(name) = value {
            if !self.scopes.declared(name.id.as_str()) && self.modules.function(name.id.as_str()) {
                return Inferred::Function(name.id.to_string());
            }
        }
        Inferred::Unknown
    }

    pub(super) fn standard_exception(&self, class: Rc<Class>, context: Context) -> bool {
        self.lineage(class, context)
            .iter()
            .any(|(class, _)| class.identity.0 == "builtins" && class.name == "BaseException")
    }

    pub(super) fn known_exception_bases(&self, class: Rc<Class>, context: Context) -> bool {
        self.known_exception_bases_in(class, context, &mut BTreeSet::new())
    }

    fn known_exception_bases_in(
        &self,
        class: Rc<Class>,
        context: Context,
        seen: &mut BTreeSet<(String, u32)>,
    ) -> bool {
        if class.identity.0 == "builtins" {
            return true;
        }
        if !seen.insert(class.identity.clone()) {
            return true;
        }
        class.bases.iter().all(|base| {
            let Some(resolved) = self.declared_base(base, context.clone()) else {
                return false;
            };
            let Target::Class(class) = resolved.target else {
                return false;
            };
            self.known_exception_bases_in(class, resolved.context, seen)
        })
    }
}
