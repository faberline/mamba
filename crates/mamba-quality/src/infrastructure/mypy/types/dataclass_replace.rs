//! Replacement signatures intersect union fields and expand type-variable bounds.
use super::super::signatures::{Kind, Parameter};
use super::*;
use std::collections::BTreeSet;

impl Context {
    pub(in crate::infrastructure::mypy) fn replace_fields(
        &self,
        ty: Type,
    ) -> Option<Vec<Parameter>> {
        self.replace_fields_inner(ty, &mut BTreeSet::new())
    }

    fn replace_fields_inner(
        &self,
        ty: Type,
        active: &mut BTreeSet<Type>,
    ) -> Option<Vec<Parameter>> {
        if !active.insert(ty) {
            return None;
        }
        let result = match self.node(ty).as_deref() {
            Some(Node::Variable(variable)) => self.replace_fields_inner(variable.bound, active),
            Some(Node::Union(items)) => self.replace_union_fields(items, active),
            _ => self.replace_owner(ty).map(|owner| {
                let solutions = self.class_solutions(ty, owner);
                self.class(owner)
                    .dataclass
                    .as_ref()
                    .unwrap()
                    .attributes
                    .iter()
                    .map(|field| Parameter {
                        name: field.name.clone(),
                        ty: Some(self.substitute(field.ty, &solutions)),
                        kind: Kind::KeywordOnly,
                        required: field.init_var && field.required,
                    })
                    .collect()
            }),
        };
        active.remove(&ty);
        result
    }

    fn replace_union_fields(
        &self,
        items: &[Type],
        active: &mut BTreeSet<Type>,
    ) -> Option<Vec<Parameter>> {
        let mut items = items.iter();
        let mut fields = self.replace_fields_inner(*items.next()?, active)?;
        for &item in items {
            let next = self.replace_fields_inner(item, active)?;
            for field in &mut fields {
                if let Some(other) = next.iter().find(|other| other.name == field.name) {
                    let left = field.ty.unwrap();
                    let right = other.ty.unwrap();
                    field.ty = Some(if left == Type::AnySpecial || right == Type::AnySpecial {
                        self.meet_parameter_types(left, right)
                    } else if left.is_any() {
                        right
                    } else if right.is_any() {
                        left
                    } else {
                        self.meet_parameter_types(left, right)
                    });
                    field.required |= other.required;
                } else {
                    field.ty = Some(Type::Never);
                }
            }
            for mut field in next {
                if !fields.iter().any(|other| other.name == field.name) {
                    field.ty = Some(Type::Never);
                    fields.push(field);
                }
            }
        }
        Some(fields)
    }

    pub(in crate::infrastructure::mypy) fn replace_owner(&self, ty: Type) -> Option<u32> {
        match self.node(ty).as_deref() {
            Some(Node::Variable(variable)) if variable.bound != ty => {
                self.replace_owner(variable.bound)
            }
            Some(Node::Union(items)) => items.first().and_then(|&ty| self.replace_owner(ty)),
            _ if !self.is_class_value(ty) => self
                .class_lineage(self.class_id(ty)?)
                .into_iter()
                .find(|&id| self.class(id).dataclass.is_some()),
            _ => None,
        }
    }

    pub(in crate::infrastructure::mypy) fn replace_bound_valid(&self, ty: Type) -> bool {
        match self.node(ty).as_deref() {
            Some(Node::Variable(variable)) if !variable.values.is_empty() => variable
                .values
                .iter()
                .all(|&ty| self.replace_bound_valid(ty)),
            Some(Node::Variable(variable)) if variable.bound != ty => {
                self.replace_bound_valid(variable.bound)
            }
            Some(Node::Union(items)) => items.iter().all(|&ty| self.replace_bound_valid(ty)),
            _ => {
                matches!(ty, Type::Any | Type::AnySpecial | Type::Never)
                    || self.replace_owner(ty).is_some()
            }
        }
    }
}
