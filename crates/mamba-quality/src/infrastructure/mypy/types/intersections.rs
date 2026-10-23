//! Nominal predicate intersections retain both bases only when inheritance is valid.
use super::*;
use crate::infrastructure::mypy::{classes::MethodKind, signatures};
use std::collections::BTreeSet;

impl Context {
    pub(super) fn nominal_intersection(
        &self,
        left: Type,
        right: Type,
        module: &str,
    ) -> Option<Type> {
        if self.is_class_value(left) || self.is_class_value(right) {
            return None;
        }
        let left_bases = self.intersection_bases(left)?;
        let right_bases = self.intersection_bases(right)?;
        let bases: Vec<_> = left_bases.iter().chain(&right_bases).copied().collect();
        if bases
            .iter()
            .any(|&ty| self.class(self.class_id(ty).unwrap()).final_class)
        {
            return None;
        }
        if !self.intersection_layout_compatible(&bases) {
            return None;
        }
        for reverse in [false, true] {
            let bases: Vec<_> = if reverse {
                right_bases.iter().chain(&left_bases)
            } else {
                left_bases.iter().chain(&right_bases)
            }
            .copied()
            .collect();
            let name = self.intersection_name(&bases);
            let id = self.declare_class(&format!("<intersection {name}>:{module}"), &name, 0);
            if self
                .classes
                .borrow()
                .predicate_intersections
                .contains_key(&id)
            {
                return Some(self.class(id).instance);
            }
            let mut class = (*self.class(id)).clone();
            class.bases = bases.iter().filter_map(|&ty| self.class_id(ty)).collect();
            class.base_types = bases.clone();
            self.replace_class(id, class);
            if self.class_mro(id).is_err() {
                return None;
            }
            if self.intersection_members_compatible(id) {
                self.classes
                    .borrow_mut()
                    .predicate_intersections
                    .insert(id, bases);
                return Some(self.class(id).instance);
            }
        }
        None
    }

    fn intersection_name(&self, bases: &[Type]) -> String {
        let names: Vec<_> = bases
            .iter()
            .map(|&ty| format!("\"{}\"", self.intersection_type_name(ty)))
            .collect();
        let names = match names.as_slice() {
            [first, second] => format!("{first} and {second}"),
            _ => {
                let (last, rest) = names.split_last().expect("intersection has bases");
                format!("{}, and {last}", rest.join(", "))
            }
        };
        format!("<subclass of {names}>")
    }

    fn intersection_type_name(&self, ty: Type) -> String {
        if self.node(ty).is_none() {
            return match ty {
                Type::Int
                | Type::Float
                | Type::Complex
                | Type::Str
                | Type::Bytes
                | Type::Bool
                | Type::Object => format!("builtins.{}", self.name(ty)),
                _ => self.name(ty),
            };
        }
        match self.node(ty).as_deref().unwrap() {
            Node::AppliedClass {
                id,
                arguments,
                value: false,
            } => format!(
                "{}[{}]",
                self.revealed_class(*id, &[], false),
                self.intersection_type_names(arguments, ", ")
            ),
            Node::Container(kind, arguments) => {
                let module = match kind {
                    Container::List | Container::Set | Container::FrozenSet | Container::Dict => {
                        "builtins"
                    }
                    Container::DictKeys | Container::DictValues | Container::DictItems => {
                        "_collections_abc"
                    }
                    _ => "typing",
                };
                format!(
                    "{module}.{}[{}]",
                    kind.name(),
                    self.intersection_type_names(arguments, ", ")
                )
            }
            Node::Tuple(items) => {
                format!("tuple[{}]", self.intersection_type_names(items, ", "))
            }
            Node::Union(items) => self.intersection_type_names(items, " | "),
            _ => self.revealed_name(ty),
        }
    }

    fn intersection_type_names(&self, items: &[Type], separator: &str) -> String {
        items
            .iter()
            .map(|&ty| self.intersection_type_name(ty))
            .collect::<Vec<_>>()
            .join(separator)
    }

    fn intersection_bases(&self, ty: Type) -> Option<Vec<Type>> {
        let id = self.class_id(ty)?;
        Some(
            self.classes
                .borrow()
                .predicate_intersections
                .get(&id)
                .cloned()
                .unwrap_or_else(|| vec![ty]),
        )
    }

    fn intersection_members_compatible(&self, id: u32) -> bool {
        let class = self.class(id);
        let lineage: Vec<_> = self.class_lineage(id).into_iter().skip(1).collect();
        let names: BTreeSet<_> = lineage
            .iter()
            .flat_map(|&base| {
                let base = self.class(base);
                base.fields
                    .keys()
                    .chain(base.methods.keys())
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();
        for name in names {
            if name == "__slots__"
                || (name.starts_with("__") && !name.ends_with("__"))
                || matches!(
                    name.as_str(),
                    "__init__" | "__new__" | "__init_subclass__" | "__replace__"
                )
            {
                continue;
            }
            let owners: Vec<_> = lineage
                .iter()
                .copied()
                .filter(|&base| {
                    let base = self.class(base);
                    base.fields.contains_key(&name) || base.methods.contains_key(&name)
                })
                .collect();
            let Some((&first, rest)) = owners.split_first() else {
                continue;
            };
            for &second in rest {
                if self.has_base(first, second) {
                    continue;
                }
                let first_class = self.class(first);
                let second_class = self.class(second);
                let receiver_for = |owner| {
                    self.applied_class(
                        owner,
                        self.class_arguments_for(class.instance, owner)
                            .unwrap_or_default(),
                        false,
                    )
                };
                let fields = (
                    self.raw_class_field(receiver_for(first), &name),
                    self.raw_class_field(receiver_for(second), &name),
                );
                if let (Some(first), Some(second)) = fields {
                    let first_type = first.ty.unwrap_or(Type::Any);
                    let second_type = second.ty.unwrap_or(Type::Any);
                    if (first.readonly && !second.readonly)
                        || !self.accepts(second_type, first_type)
                        || (!second.readonly && !self.accepts(first_type, second_type))
                    {
                        return false;
                    }
                } else if let (Some(first_method), Some(second_method)) = (
                    first_class.methods.get(&name),
                    second_class.methods.get(&name),
                ) {
                    let first_method = self.specialize_method(first_method, class.instance, first);
                    let second_method =
                        self.specialize_method(second_method, class.instance, second);
                    if first_method.kind == MethodKind::Property
                        || second_method.kind == MethodKind::Property
                        || !signatures::compatibility::accepts(
                            self,
                            &second_method
                                .signature
                                .bound(usize::from(second_method.kind != MethodKind::Static)),
                            &first_method
                                .signature
                                .bound(usize::from(first_method.kind != MethodKind::Static)),
                            true,
                        )
                    {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }
        true
    }

    fn intersection_layout_compatible(&self, bases: &[Type]) -> bool {
        let disjoint: Vec<_> = bases
            .iter()
            .filter_map(|&ty| self.class_id(ty))
            .filter_map(|id| {
                self.class_lineage(id).into_iter().find(|&id| {
                    let registry = self.classes.borrow();
                    registry.declared_disjoint_bases.contains(&id)
                        || registry.declared_slots.get(&id).is_some_and(|slots| {
                            slots.iter().any(|slot| {
                                !self.class(id).bases.iter().any(|base| {
                                    registry
                                        .declared_slots
                                        .get(base)
                                        .is_some_and(|slots| slots.contains(slot))
                                })
                            })
                        })
                })
            })
            .collect();
        let Some((&first, rest)) = disjoint.split_first() else {
            return true;
        };
        let mut candidate = first;
        for &base in rest {
            if candidate == base || self.has_base(candidate, base) {
                continue;
            }
            if self.has_base(base, candidate) {
                candidate = base;
            } else {
                return false;
            }
        }
        true
    }
}
