//! Source declaration versions are compared by value, separately from lazy caches.
use super::*;
use std::collections::BTreeSet;

pub(in crate::infrastructure::mypy) struct ClassLoopState {
    namespace: String,
    declarations: BTreeMap<u32, Arc<Class>>,
    populated: BTreeMap<u32, bool>,
    disjoint: BTreeSet<u32>,
    slots: BTreeMap<u32, BTreeSet<String>>,
    patterns: BTreeMap<(u32, String), Type>,
    ordering: BTreeMap<(u32, String), Type>,
    ordering_defaults: BTreeMap<Type, Type>,
    transforms: dataclass::transforms::LoopState,
}

impl PartialEq for ClassLoopState {
    fn eq(&self, other: &Self) -> bool {
        self.namespace == other.namespace
            && self.declarations.len() == other.declarations.len()
            && self.declarations.iter().all(|(id, class)| {
                other
                    .declarations
                    .get(id)
                    .is_some_and(|right| same_class(class, right))
            })
            && self.populated == other.populated
            && self.disjoint == other.disjoint
            && self.slots == other.slots
            && self.patterns == other.patterns
            && self.ordering == other.ordering
            && self.ordering_defaults == other.ordering_defaults
            && self.transforms == other.transforms
    }
}
impl Eq for ClassLoopState {}

fn same_model(left: &super::super::pydantic::Model, right: &super::super::pydantic::Model) -> bool {
    left.name == right.name
        && left.frozen == right.frozen
        && left.populate_by_name == right.populate_by_name
        && left.validate_by_name == right.validate_by_name
        && left.validate_by_alias == right.validate_by_alias
        && left.alias_generator == right.alias_generator
        && left.from_attributes == right.from_attributes
        && left.is_root_model == right.is_root_model
        && left.configuration.0 == right.configuration.0
        && left.fields.len() == right.fields.len()
        && left.fields.iter().zip(&right.fields).all(|(left, right)| {
            left.name == right.name
                && left.alias == right.alias
                && left.ty == right.ty
                && left.required == right.required
                && left.frozen == right.frozen
                && left.dynamic_alias == right.dynamic_alias
        })
}

fn same_dataclass(left: &dataclass::Definition, right: &dataclass::Definition) -> bool {
    left.transformed == right.transformed
        && left.field_specifiers == right.field_specifiers
        && left.class_options == right.class_options
        && left.neutral_frozen == right.neutral_frozen
        && left.frozen == right.frozen
        && left.invalid_descriptors == right.invalid_descriptors
        && left.invalid_order == right.invalid_order
        && left.duplicate_kw_only == right.duplicate_kw_only
        && left.slots_conflict == right.slots_conflict
        && left.sentinels == right.sentinels
        && left.generated_slots == right.generated_slots
        && left.order == right.order
        && left.invalid_eq == right.invalid_eq
        && left.order_conflicts == right.order_conflicts
        && left.generated_replace == right.generated_replace
        && left.attributes.len() == right.attributes.len()
        && left
            .attributes
            .iter()
            .zip(&right.attributes)
            .all(|(left, right)| {
                left.name == right.name
                    && left.alias == right.alias
                    && left.ty == right.ty
                    && left.required == right.required
                    && left.in_init == right.in_init
                    && left.keyword_only == right.keyword_only
                    && left.init_var == right.init_var
                    && left.neutral_frozen == right.neutral_frozen
                    && left.range == right.range
            })
}

fn same_optional<T>(
    left: &Option<T>,
    right: &Option<T>,
    equal: impl FnOnce(&T, &T) -> bool,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => equal(left, right),
        _ => false,
    }
}

fn same_class(left: &Class, right: &Class) -> bool {
    left.name == right.name
        && left.namespace == right.namespace
        && left.instance == right.instance
        && left.object == right.object
        && left.bases == right.bases
        && left.base_types == right.base_types
        && left.scalar_bases == right.scalar_bases
        && left.offset == right.offset
        && left.decorated_fields == right.decorated_fields
        && left.cached_properties == right.cached_properties
        && left.erased_members == right.erased_members
        && left.plugin == right.plugin
        && left.final_class == right.final_class
        && left.protocol == right.protocol
        && left.runtime_protocol == right.runtime_protocol
        && left.generic == right.generic
        && left.any_base_origins.len() == right.any_base_origins.len()
        && left
            .any_base_origins
            .iter()
            .zip(&right.any_base_origins)
            .all(|(left, right)| match (left, right) {
                (
                    AnyBaseOrigin::ResolvedAny { range: left },
                    AnyBaseOrigin::ResolvedAny { range: right },
                ) => left == right,
                (
                    AnyBaseOrigin::MissingModuleMember {
                        range: lrange,
                        module: lmodule,
                        member: lmember,
                    },
                    AnyBaseOrigin::MissingModuleMember {
                        range: rrange,
                        module: rmodule,
                        member: rmember,
                    },
                ) => lrange == rrange && lmodule == rmodule && lmember == rmember,
                _ => false,
            })
        && left.fields.len() == right.fields.len()
        && left.fields.iter().all(|(name, left)| {
            right.fields.get(name).is_some_and(|right| {
                left.ty == right.ty
                    && left.needs_annotation == right.needs_annotation
                    && left.class_access == right.class_access
                    && left.readonly == right.readonly
                    && left.final_attribute == right.final_attribute
                    && left.class_var == right.class_var
                    && left.abstract_field == right.abstract_field
            })
        })
        && left.methods.len() == right.methods.len()
        && left.methods.iter().all(|(name, left)| {
            right.methods.get(name).is_some_and(|right| {
                left.signature == right.signature
                    && left.kind == right.kind
                    && left.setter == right.setter
                    && left.final_method == right.final_method
                    && left.abstractness == right.abstractness
            })
        })
        && same_optional(&left.pydantic, &right.pydantic, same_model)
        && same_optional(&left.dataclass, &right.dataclass, same_dataclass)
        && same_optional(&left.typed_dict, &right.typed_dict, |left, right| {
            left.closed == right.closed
                && left.items.len() == right.items.len()
                && left.items.iter().zip(&right.items).all(|(left, right)| {
                    left.name == right.name
                        && left.ty == right.ty
                        && left.required == right.required
                        && left.readonly == right.readonly
                })
        })
}

impl Context {
    pub(in crate::infrastructure::mypy) fn loop_class_state(
        &self,
        namespace: &str,
    ) -> ClassLoopState {
        let registry = self.classes.borrow();
        // Scopes keeps one exact module namespace across lexical entry. Nested
        // source classes use that namespace and their own original AST offset.
        // A module named `a.b` is not selected for the owner module `a`.
        let declarations: BTreeMap<_, _> = registry
            .keys
            .range((namespace.to_owned(), 0)..=(namespace.to_owned(), u32::MAX))
            .map(|(_, &id)| (id, registry.entries[id as usize].clone()))
            .collect();
        let owns = |id: &u32| declarations.contains_key(id);
        let ordering: BTreeMap<_, _> = registry
            .dataclass_order_variables
            .iter()
            .filter(|((id, _), _)| owns(id))
            .map(|(key, &value)| (key.clone(), value))
            .collect();
        ClassLoopState {
            namespace: namespace.to_owned(),
            populated: registry
                .populated_class_bases
                .iter()
                .filter(|(id, _)| owns(id))
                .map(|(&id, &value)| (id, value))
                .collect(),
            disjoint: registry
                .declared_disjoint_bases
                .iter()
                .filter(|id| owns(id))
                .copied()
                .collect(),
            slots: registry
                .declared_slots
                .iter()
                .filter(|(id, _)| owns(id))
                .map(|(&id, value)| (id, value.clone()))
                .collect(),
            patterns: registry
                .pattern_values
                .iter()
                .filter(|((id, _), _)| owns(id))
                .map(|(key, &value)| (key.clone(), value))
                .collect(),
            ordering_defaults: ordering
                .values()
                .filter_map(|variable| {
                    registry
                        .inference_defaults
                        .get(variable)
                        .map(|&value| (*variable, value))
                })
                .collect(),
            transforms: registry
                .dataclass_transforms
                .loop_state(namespace, declarations.keys().copied().collect()),
            ordering,
            declarations,
        }
    }

    pub(in crate::infrastructure::mypy) fn reconcile_loop_class_caches(
        &self,
        previous: &ClassLoopState,
    ) -> (ClassLoopState, bool) {
        let next = self.loop_class_state(&previous.namespace);
        // Existing invalidation remains unchanged. A predicate intersection's
        // map is also its base provenance, not just a disposable success cache.
        // If source bases/members change beneath it, report a missing replay
        // policy rather than erase that semantic information.
        let changed: BTreeSet<_> = previous
            .declarations
            .iter()
            .filter_map(|(&id, old)| {
                next.declarations
                    .get(&id)
                    .is_none_or(|new| !same_class(old, new))
                    .then_some(id)
            })
            .chain(
                next.declarations
                    .keys()
                    .copied()
                    .filter(|id| !previous.declarations.contains_key(id)),
            )
            .collect();
        if !changed.is_empty() {
            let candidates: Vec<_> = self
                .classes
                .borrow()
                .predicate_intersections
                .iter()
                .map(|(&id, bases)| (id, bases.clone()))
                .collect();
            let invalid: Vec<_> = candidates
                .into_iter()
                .filter_map(|(id, bases)| {
                    let mut pending: Vec<_> = bases
                        .into_iter()
                        .filter_map(|ty| self.class_id(ty))
                        .collect();
                    let mut seen = BTreeSet::new();
                    while let Some(base) = pending.pop() {
                        if changed.contains(&base) {
                            return Some(id);
                        }
                        if seen.insert(base) {
                            pending.extend(self.class(base).bases.iter().copied());
                        }
                    }
                    None
                })
                .collect();
            return (next, !invalid.is_empty());
        }
        (next, false)
    }
}
