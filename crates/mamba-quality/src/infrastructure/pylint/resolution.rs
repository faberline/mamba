//! References retain their module context through aliases and reexports.
use super::{
    callables::{Signature, Target},
    values::{self, Value},
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::Expr;
use std::{
    collections::{BTreeSet, VecDeque},
    rc::Rc,
};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Context {
    Scope(usize),
    Module(String),
}

pub(super) struct Resolved {
    pub target: Target,
    pub context: Context,
}

impl Checker<'_> {
    pub(super) fn reference_in(&self, names: Vec<String>, context: Context) -> Option<Resolved> {
        let key = (context.clone(), names.clone());
        if !self.resolving.borrow_mut().insert(key.clone()) {
            return None;
        }
        let result = self.reference_inner(names, context);
        self.resolving.borrow_mut().remove(&key);
        result
    }

    fn reference_inner(&self, names: Vec<String>, mut context: Context) -> Option<Resolved> {
        let mut pending: VecDeque<_> = names.into();
        let mut visited = BTreeSet::new();
        loop {
            let name = pending.pop_front()?;
            if !visited.insert((context.clone(), name.clone())) {
                return None;
            }
            let mut target = match &context {
                Context::Module(module) => self
                    .modules
                    .call_target(self.source.project, module, &name)
                    .or_else(|| {
                        self.modules
                            .child(self.source.project, module, &name)
                            .map(Target::Module)
                    })
                    .or_else(|| {
                        (!self.modules.native(module)
                            && self.modules.exists(self.source.project, module)
                            && !self.modules.member(self.source.project, module, &name))
                        .then(|| self.modules.builtin_class(&name))
                        .flatten()
                        .map(Target::Class)
                    })?,
                Context::Scope(index) => {
                    if let Some((index, target)) = self.scopes.call_target_in(&name, *index) {
                        context = Context::Scope(index);
                        target
                    } else {
                        let class = self.modules.builtin_class(&name)?;
                        context = Context::Module("builtins".into());
                        Target::Class(class)
                    }
                }
            };
            loop {
                match target {
                    Target::Reference(names) => {
                        if let Context::Scope(index) = &context {
                            if let Some(Expr::Name(value)) =
                                self.scopes.first_expression(&name, *index)
                            {
                                if let Some((owner, values)) = self.scopes.values_before(
                                    value.id.as_str(),
                                    *index,
                                    value.range.start(),
                                ) {
                                    let [value] = values.as_slice() else {
                                        return None;
                                    };
                                    let resolved =
                                        self.history_target(value, Context::Scope(owner))?;
                                    context = resolved.context;
                                    target = resolved.target;
                                    continue;
                                }
                            }
                        }
                        for name in names.into_iter().rev() {
                            pending.push_front(name);
                        }
                        break;
                    }
                    Target::Construct(call) => {
                        let resolved = self
                            .expression_target_in(&Expr::Call((*call).clone()), context.clone())?;
                        context = resolved.context;
                        target = resolved.target;
                    }
                    Target::Contextual(value, origin) => {
                        context = origin;
                        target = *value;
                    }
                    Target::Imported(import) => {
                        if let Some(member) = import.member {
                            context = Context::Module(import.module);
                            pending.push_front(member);
                        } else {
                            let name = if import.alias.is_none() {
                                import.local
                            } else {
                                import.module
                            };
                            if pending.is_empty() {
                                return Some(Resolved {
                                    target: Target::Module(name),
                                    context,
                                });
                            }
                            context = Context::Module(name);
                        }
                        break;
                    }
                    Target::Module(name) => {
                        if pending.is_empty() {
                            return Some(Resolved {
                                target: Target::Module(name),
                                context,
                            });
                        }
                        context = Context::Module(name);
                        break;
                    }
                    Target::Class(class) if !pending.is_empty() => {
                        let resolved = self.class_member(
                            class,
                            context.clone(),
                            &pending.pop_front()?,
                            false,
                        )?;
                        context = resolved.context;
                        target = resolved.target;
                    }
                    Target::Instance(class, origin) if !pending.is_empty() => {
                        let resolved =
                            self.class_member(class, origin, &pending.pop_front()?, true)?;
                        context = resolved.context;
                        target = resolved.target;
                    }
                    Target::Super(proxy) if !pending.is_empty() => {
                        let resolved = self.super_member(&proxy, &pending.pop_front()?)?;
                        context = resolved.context;
                        target = resolved.target;
                    }
                    target if pending.is_empty() => return Some(Resolved { target, context }),
                    _ => return None,
                }
            }
        }
    }

    pub(super) fn callable_target(&self, expression: &Expr) -> Option<Target> {
        self.expression_target(expression)
            .map(|resolved| resolved.target)
    }

    pub(super) fn expression_target(&self, expression: &Expr) -> Option<Resolved> {
        let context = Context::Scope(self.scopes.current_index());
        self.expression_target_in(expression, context)
    }

    pub(super) fn expression_target_in(
        &self,
        expression: &Expr,
        context: Context,
    ) -> Option<Resolved> {
        if let Expr::Attribute(attribute) = expression {
            if let Some(value) = self.builtin_value_metadata(
                &attribute.value,
                context.clone(),
                attribute.attr.as_str(),
            ) {
                return Some(value);
            }
        }
        if let Expr::Subscript(value) = expression {
            if let Some(resolved) = self.class_container_subscript(value, context.clone()) {
                return Some(resolved);
            }
        }
        if let (Expr::Name(name), Context::Scope(index)) = (expression, &context) {
            if let Some((owner, values)) =
                self.scopes
                    .values_before(name.id.as_str(), *index, name.range.start())
            {
                let [value] = values.as_slice() else {
                    return None;
                };
                return self.history_target(value, Context::Scope(owner));
            }
        }
        match expression {
            Expr::Call(call) => {
                let resolved = self.expression_target_in(&call.func, context.clone())?;
                if matches!(&resolved.target, Target::Class(class) if class.identity.0 == "builtins" && class.name == "type")
                    && call.arguments.args.len() == 1
                    && call.arguments.keywords.is_empty()
                {
                    let value = self.expression_target_in(&call.arguments.args[0], context)?;
                    return match value.target {
                        Target::Instance(class, origin) => Some(Resolved {
                            target: Target::Class(class),
                            context: origin,
                        }),
                        Target::Class(_) => Some(Resolved {
                            target: Target::Class(self.modules.builtin_class("type")?),
                            context: Context::Module("builtins".into()),
                        }),
                        Target::Value(value) => Some(Resolved {
                            target: Target::Class(
                                self.modules.builtin_class(value.builtin_name()?)?,
                            ),
                            context: Context::Module("builtins".into()),
                        }),
                        _ => None,
                    };
                }
                if matches!(&resolved.target, Target::Function(value) if value.name == "__new__")
                    && matches!(&resolved.context, Context::Module(module) if module == "builtins")
                {
                    let value = self.expression_target_in(call.arguments.args.first()?, context)?;
                    if let Target::Class(class) = value.target {
                        return Some(Resolved {
                            target: Target::Instance(class, value.context.clone()),
                            context: value.context,
                        });
                    }
                    return None;
                }
                return match resolved.target {
                    Target::Class(class) => {
                        if class.identity.0 == "builtins" && class.name == "super" {
                            return Some(Resolved {
                                target: Target::Super(self.super_proxy(call, context.clone())?),
                                context,
                            });
                        }
                        Some(Resolved {
                            target: Target::Instance(class, resolved.context.clone()),
                            context: resolved.context,
                        })
                    }
                    Target::Function(signature) if signature.result_class.is_some() => {
                        let (module, name) = signature.result_class.as_ref()?;
                        let Target::Class(class) =
                            self.modules
                                .call_target(self.source.project, module, name)?
                        else {
                            return None;
                        };
                        let origin = Context::Module(module.clone());
                        Some(Resolved {
                            target: Target::Instance(class, origin.clone()),
                            context: origin,
                        })
                    }
                    Target::Function(signature) => Some(Resolved {
                        target: Target::Value(Rc::new(self.call_result(
                            &signature,
                            call,
                            &context,
                            &resolved.context,
                        ))),
                        context,
                    }),
                    _ => None,
                };
            }
            Expr::Attribute(attribute)
                if values::path(expression).is_none()
                    || values::path(expression)
                        .is_some_and(|names| names.iter().any(|name| name == "__class__")) =>
            {
                let resolved = self.expression_target_in(&attribute.value, context)?;
                return self.resolved_attribute(resolved, attribute.attr.as_str());
            }
            _ => {}
        }
        if let Expr::Lambda(lambda) = expression {
            return Some(Resolved {
                target: Target::Function(Rc::new(Signature::from_lambda(lambda))),
                context,
            });
        }
        if let Some(path) = values::path(expression) {
            return self.reference_in(path, context);
        }
        Some(Resolved {
            target: Target::Value(Rc::new(Value::from_expression(expression))),
            context,
        })
    }

    pub(super) fn static_value(&self, expression: &Expr) -> Value {
        if matches!(expression, Expr::Name(_) | Expr::Attribute(_))
            && !matches!(
                Value::from_expression(expression),
                Value::StringMethod(_, _)
            )
        {
            return match self.expression_target(expression) {
                Some(Resolved {
                    target: Target::Value(value),
                    context,
                }) => self.expand_value((*value).clone(), &context, &mut BTreeSet::new()),
                _ => Value::Unknown,
            };
        }
        self.expand_value(
            Value::from_expression(expression),
            &Context::Scope(self.scopes.current_index()),
            &mut BTreeSet::new(),
        )
    }

    pub(super) fn expand_value(
        &self,
        value: Value,
        context: &Context,
        active: &mut BTreeSet<(Context, Vec<String>)>,
    ) -> Value {
        match value {
            Value::Reference(names) => {
                let key = (context.clone(), names.clone());
                if !active.insert(key.clone()) {
                    return Value::Unknown;
                }
                let value = match self.reference_in(names, context.clone()) {
                    Some(Resolved {
                        target: Target::Value(value),
                        context,
                    }) => self.expand_value((*value).clone(), &context, active),
                    _ => Value::Unknown,
                };
                active.remove(&key);
                value
            }
            Value::Sequence(items, kind) => Value::Sequence(
                items
                    .into_iter()
                    .flat_map(|value| match self.expand_value(value, context, active) {
                        Value::Spread(value) => match *value {
                            Value::Sequence(items, _) => items,
                            _ => vec![Value::Unknown],
                        },
                        value => vec![value],
                    })
                    .collect(),
                kind,
            ),
            Value::StringMethod(name, value) => {
                Value::StringMethod(name, Box::new(self.expand_value(*value, context, active)))
            }
            Value::Mapping(items) => Value::Mapping(
                items
                    .into_iter()
                    .flat_map(|(key, value)| {
                        let value = self.expand_value(value, context, active);
                        if let Some(key) = key {
                            vec![(Some(self.expand_value(key, context, active)), value)]
                        } else if let Value::Mapping(items) = value {
                            items
                        } else {
                            vec![(None, Value::Unknown)]
                        }
                    })
                    .collect(),
            ),
            Value::Spread(value) => {
                Value::Spread(Box::new(self.expand_value(*value, context, active)))
            }
            value => value,
        }
    }
}
