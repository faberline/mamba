//! Format access walks static data and declared members without Python execution.
use super::super::resolution::Resolved;
use super::brace::Field;
use super::*;
use mamba_language::application::quality_syntax::python_ast::Number;
use std::rc::Rc;

fn lookup(value: &Value, key: &Key) -> Result<Value, ()> {
    match (value, key) {
        (Value::Sequence(values, "tuple" | "list"), Key::Index(index)) => {
            values.get(*index).cloned().ok_or(())
        }
        (Value::String(text), Key::Index(index)) => text
            .chars()
            .nth(*index)
            .map(|c| Value::String(c.to_string()))
            .ok_or(()),
        (Value::Bytes(bytes), Key::Index(index)) => bytes
            .get(*index)
            .map(|&b| Value::Number(Number::Int(b.into())))
            .ok_or(()),
        (Value::Mapping(items), key) => {
            let mut unknown = false;
            for (candidate, value) in items {
                match (candidate, key) {
                    (Some(Value::String(name)), Key::Name(wanted)) if name == wanted => {
                        return Ok(value.clone())
                    }
                    (Some(Value::Number(Number::Int(index))), Key::Index(wanted))
                        if index.as_usize() == Some(*wanted) =>
                    {
                        return Ok(value.clone())
                    }
                    (Some(Value::Boolean(b)), Key::Index(wanted)) if usize::from(*b) == *wanted => {
                        return Ok(value.clone())
                    }
                    (None | Some(Value::Unknown), _) => unknown = true,
                    _ => {}
                }
            }
            if unknown {
                Ok(Value::Unknown)
            } else {
                Err(())
            }
        }
        (Value::Unknown, _) => Ok(Value::Unknown),
        _ => Err(()),
    }
}

impl Checker<'_> {
    pub(super) fn check_format_access(&mut self, call: &ExprCall, fields: &[Field]) {
        for field in fields {
            let key = match &field.key {
                Key::Name(name) if name.is_empty() => Key::Index(0),
                key => key.clone(),
            };
            let argument = match &key {
                Key::Index(index) => call.arguments.args.get(*index),
                Key::Name(name) => call
                    .arguments
                    .keywords
                    .iter()
                    .find(|keyword| keyword.arg.as_ref().is_some_and(|arg| arg.as_str() == name))
                    .map(|keyword| &keyword.value),
            };
            let Some(argument) = argument else { continue };
            let Some(mut resolved) = self.expression_target(argument) else {
                continue;
            };
            let mut path = match &key {
                Key::Index(index) => index.to_string(),
                Key::Name(name) => name.clone(),
            };
            for (attribute, part) in &field.access {
                path.push_str(&if *attribute {
                    match part {
                        Key::Name(name) => format!(".{name}"),
                        Key::Index(index) => format!(".{index}"),
                    }
                } else {
                    format!("[{}]", representation::key(part))
                });
                let next = if *attribute {
                    self.format_attribute(resolved, part)
                } else {
                    self.format_index(resolved, part)
                };
                match next {
                    Ok(Some(next)) => resolved = next,
                    Ok(None) => break,
                    Err(()) => {
                        if *attribute {
                            self.emit(
                                "W1306",
                                "missing-format-attribute",
                                &format!(
                                    "Missing format attribute {} in format specifier {}",
                                    representation::key(part),
                                    representation::repr(&path)
                                ),
                                call.range(),
                            );
                        } else {
                            self.emit(
                                "W1307",
                                "invalid-format-index",
                                &format!(
                                    "Using invalid lookup key {} in format specifier {}",
                                    representation::key(part),
                                    representation::repr(&path)
                                ),
                                call.range(),
                            );
                        }
                        break;
                    }
                }
            }
        }
    }

    fn format_attribute(&self, resolved: Resolved, key: &Key) -> Result<Option<Resolved>, ()> {
        let Key::Name(name) = key else { return Err(()) };
        let (class, context, instance) = match resolved.target {
            Target::Instance(class, context) => (class, context, true),
            Target::Class(class) => (class, resolved.context, false),
            Target::Value(value) => {
                let value =
                    self.expand_value((*value).clone(), &resolved.context, &mut Default::default());
                let name = match &value {
                    Value::Sequence(_, kind) => *kind,
                    Value::Mapping(_) => "dict",
                    _ => match value.builtin_name() {
                        Some(name) => name,
                        None => return Ok(None),
                    },
                };
                let Some(class) = self.modules.builtin_class(name) else {
                    return Err(());
                };
                (class, Context::Module("builtins".into()), true)
            }
            _ => return Ok(None),
        };
        if let Some(next) = self.class_member(class.clone(), context.clone(), name, instance) {
            Ok(Some(next))
        } else if instance && self.dynamic_member(class, context) {
            Ok(None)
        } else {
            Err(())
        }
    }

    fn format_index(&self, resolved: Resolved, key: &Key) -> Result<Option<Resolved>, ()> {
        match resolved.target {
            Target::Value(value) => {
                let value =
                    self.expand_value((*value).clone(), &resolved.context, &mut Default::default());
                let value = lookup(&value, key)?;
                if matches!(value, Value::Unknown) {
                    return Ok(None);
                }
                Ok(Some(Resolved {
                    target: Target::Value(Rc::new(value)),
                    context: resolved.context,
                }))
            }
            Target::Instance(class, context) => {
                let Some(method) = self.class_member(class, context, "__getitem__", true) else {
                    return Ok(None);
                };
                let Target::Function(signature) = method.target else {
                    return Ok(None);
                };
                if !matches!(signature.category, "method" | "classmethod") {
                    return Ok(None);
                }
                let [parameter] = signature.parameters.as_slice() else {
                    return Err(());
                };
                let value = match key {
                    Key::Name(name) => Value::String(name.clone()),
                    Key::Index(index) => Value::Number(Number::Int((*index as u64).into())),
                };
                let arguments = std::collections::BTreeMap::from([(parameter.name.clone(), value)]);
                let value =
                    self.result_value(signature.result.clone(), &arguments, &method.context);
                if matches!(value, Value::Unknown) {
                    return Ok(None);
                }
                Ok(Some(Resolved {
                    target: Target::Value(Rc::new(value)),
                    context: method.context,
                }))
            }
            Target::Class(class) => {
                if self
                    .class_member(class, resolved.context, "__getitem__", false)
                    .is_some()
                {
                    Ok(None)
                } else {
                    Err(())
                }
            }
            _ => Ok(None),
        }
    }
}
