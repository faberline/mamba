//! Slot inference retains literal nodes for diagnostics and runtime layouts.
use super::{callables::Target, classes::Class, resolution::Context, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::Expr;
use std::{collections::BTreeSet, rc::Rc};

#[derive(Clone)]
pub(super) enum Slots {
    Unknown,
    Invalid,
    String(String),
    Bytes(Vec<u8>),
    Elements(Vec<Expr>, Context),
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Slot {
    Name(String),
    Bytes(Vec<u8>),
}
impl Slot {
    pub(super) fn named(&self, name: &str) -> bool {
        matches!(self, Self::Name(value) if value == name)
    }
}
impl Checker<'_> {
    fn slot_elements(&self, values: &[Expr], context: &Context) -> Vec<Expr> {
        let mut result = Vec::new();
        for value in values {
            if let Expr::Starred(value) = value {
                for (value, origin) in self.slot_nodes(&value.value, context.clone()) {
                    if let Slots::Elements(values, _) = self.slot_value(&value, origin) {
                        result.extend(values);
                    }
                }
            } else {
                result.push(value.clone());
            }
        }
        result
    }
    pub(super) fn slot_value(&self, expression: &Expr, context: Context) -> Slots {
        match expression {
            Expr::Tuple(value) => {
                Slots::Elements(self.slot_elements(&value.elts, &context), context)
            }
            Expr::List(value) => {
                Slots::Elements(self.slot_elements(&value.elts, &context), context)
            }
            Expr::Set(value) => Slots::Elements(self.slot_elements(&value.elts, &context), context),
            Expr::Dict(value) => {
                let mut elements = Vec::new();
                for value in &value.items {
                    if let Some(key) = &value.key {
                        elements.push(key.clone());
                    } else {
                        for (value, origin) in self.slot_nodes(&value.value, context.clone()) {
                            if let Slots::Elements(values, _) = self.slot_value(&value, origin) {
                                elements.extend(values);
                            }
                        }
                    }
                }
                Slots::Elements(elements, context)
            }
            Expr::StringLiteral(value) => Slots::String(value.value.to_str().to_string()),
            Expr::BytesLiteral(value) => Slots::Bytes(value.value.bytes().collect()),
            Expr::Call(_) => match self
                .expression_target_in(expression, context.clone())
                .map(|value| value.target)
            {
                Some(Target::Instance(class, origin))
                    if class.identity.0 != "builtins"
                        && self
                            .class_member(class.clone(), origin.clone(), "__iter__", true)
                            .is_none()
                        && self
                            .class_member(class.clone(), origin.clone(), "__getitem__", true)
                            .is_none() =>
                {
                    Slots::Invalid
                }
                _ => Slots::Unknown,
            },
            Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::Generator(_) => {
                Slots::Unknown
            }
            _ => match self.slot_scalar(expression, context) {
                Value::String(value) => Slots::String(value),
                Value::Bytes(value) => Slots::Bytes(value),
                Value::Unknown => Slots::Unknown,
                _ => Slots::Invalid,
            },
        }
    }
    pub(super) fn slot_name(&self, expression: &Expr, context: Context) -> Option<String> {
        match self.slot_scalar(expression, context) {
            Value::String(value) if !value.is_empty() => Some(value),
            _ => None,
        }
    }
    pub(super) fn own_slots(&self, class: &Class, context: Context) -> Option<Vec<Slot>> {
        if class.slot_values.is_empty() {
            return None;
        }
        let mut names = Vec::new();
        for expression in &class.slot_values {
            for (expression, origin) in self.slot_nodes(expression, context.clone()) {
                match self.slot_value(&expression, origin) {
                    Slots::String(value) if !value.is_empty() => names.push(Slot::Name(value)),
                    Slots::Bytes(value) if !value.is_empty() => names.push(Slot::Bytes(value)),
                    Slots::Elements(values, origin) => {
                        let own: Vec<_> = values
                            .iter()
                            .filter_map(|value| self.slot_name(value, origin.clone()))
                            .collect();
                        if values.is_empty() {
                            return Some(names);
                        }
                        names.extend(own.into_iter().map(Slot::Name));
                    }
                    _ => {}
                }
            }
        }
        (!names.is_empty()).then_some(names)
    }
    pub(super) fn layout_slots(
        &self,
        class: Rc<Class>,
        context: Context,
    ) -> Option<BTreeSet<Slot>> {
        let mut names = BTreeSet::new();
        for (class, context) in self.mro(class, context).ok()? {
            if class.identity.0 == "builtins" && class.name == "object" {
                continue;
            }
            if class.name == "Generic" && class.identity.0 == "typing" {
                continue;
            }
            if class.bases.iter().any(|base| {
                !matches!(
                    self.declared_base(base, context.clone())
                        .map(|value| value.target),
                    Some(Target::Class(_))
                )
            }) {
                return None;
            }
            names.extend(self.own_slots(&class, context)?);
        }
        Some(names)
    }
    pub(super) fn valid_slots(&self, class: &Class, context: Context) -> bool {
        !class.slot_check_values.is_empty()
            && class.slot_check_values.iter().all(|value| {
                self.slot_nodes(value, context.clone())
                    .iter()
                    .all(|(value, context)| {
                        matches!(
                            self.slot_value(value, context.clone()),
                            Slots::Elements(_, _)
                        )
                    })
            })
    }
}
