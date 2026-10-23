//! Mutable defaults use original symbols and Pylint value inference.
use super::{callables::Target, defaults, display, resolution::Context, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtFunctionDef};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

impl Checker<'_> {
    fn mutable_default(
        &self,
        expression: &Expr,
        context: Context,
        active: &mut BTreeSet<(Context, u32, u32)>,
    ) -> Option<(String, String)> {
        let key = (
            context.clone(),
            expression.start().to_u32(),
            expression.end().to_u32(),
        );
        if !active.insert(key.clone()) {
            return None;
        }
        let result = (|| {
            let direct = match expression {
                Expr::List(_) => Some("list"),
                Expr::Set(_) => Some("set"),
                Expr::Dict(_) => Some("dict"),
                _ => None,
            };
            if let Some(name) = direct {
                return Some((format!("builtins.{name}"), name.into()));
            }
            if let Expr::Name(name) = expression {
                if let Some((value, origin)) = self.source_binding(name.id.as_str(), &context) {
                    return self.mutable_default(&value, origin, active);
                }
            }
            if let Expr::Attribute(value) = expression {
                if let Some(resolved) = self.expression_target_in(&value.value, context.clone()) {
                    if let Target::Module(module) = resolved.target {
                        if let Some((value, origin)) =
                            self.source_binding(value.attr.as_str(), &Context::Module(module))
                        {
                            return self.mutable_default(&value, origin, active);
                        }
                    }
                }
            }
            if let Expr::Call(call) = expression {
                if let Expr::Name(name) = call.func.as_ref() {
                    let qname = format!("builtins.{}", name.id);
                    if defaults::fixed().mutable_default(&qname).is_some() {
                        return Some((qname, name.id.to_string()));
                    }
                }
                if let Some(resolved) = self.expression_target_in(&call.func, context.clone()) {
                    if let Target::Function(function) = resolved.target {
                        let mut values = function.result_expressions.iter();
                        let first =
                            self.mutable_default(values.next()?, resolved.context.clone(), active)?;
                        for value in values {
                            if self
                                .mutable_default(value, resolved.context.clone(), active)
                                .as_ref()
                                != Some(&first)
                            {
                                return None;
                            }
                        }
                        return Some(first);
                    }
                }
            }
            let resolved = self.expression_target_in(expression, context)?;
            match resolved.target {
                Target::Value(target) => match self.expand_value(
                    (*target).clone(),
                    &resolved.context,
                    &mut Default::default(),
                ) {
                    Value::Sequence(_, "list") => Some(("builtins.list".into(), "list".into())),
                    Value::Sequence(_, "set") => Some(("builtins.set".into(), "set".into())),
                    Value::Mapping(_) => Some(("builtins.dict".into(), "dict".into())),
                    _ => None,
                },
                Target::Instance(class, origin) => {
                    let name = class.name.clone();
                    self.lineage(class, origin)
                        .into_iter()
                        .find_map(|(class, _)| {
                            let qname = format!("{}.{}", class.identity.0, class.name);
                            defaults::fixed()
                                .mutable_default(&qname)
                                .map(|_| (qname, name.clone()))
                        })
                }
                _ => None,
            }
        })();
        active.remove(&key);
        result
    }
    pub(super) fn check_mutable_defaults(&mut self, function: &StmtFunctionDef) {
        for parameter in function.parameters.iter() {
            let Some(value) = parameter.default() else {
                continue;
            };
            let Some((qname, name)) = self.mutable_default(
                value,
                Context::Scope(self.scopes.current_index()),
                &mut BTreeSet::new(),
            ) else {
                continue;
            };
            let literal = matches!(value, Expr::List(_) | Expr::Dict(_) | Expr::Set(_));
            let text = if literal {
                defaults::fixed().mutable_default(&qname).unwrap().into()
            } else if matches!(value, Expr::Call(_)) {
                format!("{name}()")
            } else {
                format!(
                    "{} ({qname})",
                    display::expression(value, &self.source.text)
                )
            };
            self.emit(
                "W0102",
                "dangerous-default-value",
                &format!("Dangerous default value {text} as argument"),
                self.declaration_range(function.name.range()),
            );
        }
    }
}
