//! Class sequence indexing retains each class and its declaration context.
use super::super::{
    callables::Target,
    classes::Class,
    operand_inference::Active,
    resolution::{Context, Resolved},
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprSubscript};
use ruff_text_size::Ranged;
use std::rc::Rc;

type Classes = Vec<(Rc<Class>, Context)>;

impl Checker<'_> {
    pub(in super::super) fn class_container_subscript(
        &self,
        value: &ExprSubscript,
        context: Context,
    ) -> Option<Resolved> {
        let classes = self.class_container(&value.value, context.clone(), &mut Active::new())?;
        let index = self.unpacking_integer(&value.slice, context)?;
        let index = if index < 0 {
            i64::try_from(classes.len()).ok()?.checked_add(index)?
        } else {
            index
        };
        let (class, context) = classes.get(usize::try_from(index).ok()?)?;
        Some(Resolved {
            target: Target::Class(class.clone()),
            context: context.clone(),
        })
    }

    fn class_container(
        &self,
        expression: &Expr,
        context: Context,
        active: &mut Active,
    ) -> Option<Classes> {
        let key = (
            context.clone(),
            expression.start().to_u32(),
            expression.end().to_u32(),
        );
        if !active.insert(key.clone()) {
            return None;
        }
        let result = self.class_container_inner(expression, context, active);
        active.remove(&key);
        result
    }

    fn class_container_inner(
        &self,
        expression: &Expr,
        context: Context,
        active: &mut Active,
    ) -> Option<Classes> {
        if let Expr::Name(name) = expression {
            if let Context::Scope(index) = &context {
                if let Some((owner, values)) =
                    self.scopes
                        .values_before(name.id.as_str(), *index, name.start())
                {
                    let mut alternatives = values
                        .iter()
                        .map(|value| self.class_container(value, Context::Scope(owner), active));
                    let first = alternatives.next()??;
                    for other in alternatives {
                        let other = other?;
                        if first.len() != other.len()
                            || first.iter().zip(&other).any(|(left, right)| {
                                left.0.identity != right.0.identity || left.1 != right.1
                            })
                        {
                            return None;
                        }
                    }
                    return Some(first);
                }
            }
            let (expression, context) = self.source_binding(name.id.as_str(), &context)?;
            return self.class_container(&expression, context, active);
        }
        let (attribute, called) = match expression {
            Expr::Attribute(attribute) => (attribute, false),
            Expr::Call(call)
                if call.arguments.args.is_empty() && call.arguments.keywords.is_empty() =>
            {
                let Expr::Attribute(attribute) = call.func.as_ref() else {
                    return None;
                };
                (attribute, true)
            }
            _ => return None,
        };
        let name = attribute.attr.as_str();
        if !matches!(
            (name, called),
            ("__bases__" | "__mro__", false) | ("mro", true)
        ) {
            return None;
        }
        let owner = self.expression_target_in(&attribute.value, context)?;
        let Target::Class(class) = owner.target else {
            return None;
        };
        if class.identity.0 == "builtins" {
            if called && class.name == "type" {
                return None;
            }
            return self.builtin_class_container(&class, name);
        }
        if self
            .lineage(class.clone(), owner.context.clone())
            .into_iter()
            .any(|(class, origin)| {
                self.local_class_member(&class, origin, name, false)
                    .is_some()
            })
        {
            return None;
        }
        if name != "__bases__" {
            return self.mro(class, owner.context).ok();
        }
        if class.bases.is_empty() {
            return Some(vec![(
                self.modules.builtin_class("object")?,
                Context::Module("builtins".into()),
            )]);
        }
        Some(
            class
                .bases
                .iter()
                .filter_map(|value| self.inferred_base(value, owner.context.clone()))
                .collect(),
        )
    }
}
