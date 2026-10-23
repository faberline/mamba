//! Nominal declarations have stable identities throughout workspace inference.
mod collect;
pub(super) use collect::partial_none::{
    Frame as ConstructorNoneFrame, Plan as ConstructorNonePlan, State as ConstructorNoneState,
};
pub(super) mod dataclass;
mod generics;
mod loop_state;
pub(super) mod mro;
mod properties;
pub(super) mod statements;
pub(super) mod typed_dict;
use super::{
    signatures::Signature,
    types::{Context, Type},
};
use ruff_text_size::{Ranged, TextSize};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MethodKind {
    Instance,
    Static,
    Class,
    Property,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Abstractness {
    Concrete,
    Explicit,
    Implicit,
}

pub(in crate::infrastructure::mypy) fn method_kind(
    function: &mamba_language::application::quality_syntax::python_ast::StmtFunctionDef,
) -> MethodKind {
    collect::method_kind(function)
}

#[derive(Clone)]
pub(super) struct Method {
    pub signature: Arc<Signature>,
    pub kind: MethodKind,
    pub setter: Option<Arc<Signature>>,
    pub final_method: bool,
    pub abstractness: Abstractness,
}

#[derive(Clone)]
pub(super) struct Field {
    pub ty: Option<Type>,
    pub needs_annotation: bool,
    pub class_access: bool,
    pub readonly: bool,
    pub final_attribute: bool,
    pub class_var: bool,
    pub abstract_field: bool,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct CollectionMetadata {
    pub annotated_fields: std::collections::BTreeSet<String>,
    pub first_field_declarations: BTreeMap<String, TextSize>,
    pub constructor_offset: Option<TextSize>,
}

#[derive(Clone)]
pub(in crate::infrastructure::mypy) enum AnyBaseOrigin {
    ResolvedAny {
        range: ruff_text_size::TextRange,
    },
    MissingModuleMember {
        range: ruff_text_size::TextRange,
        module: String,
        member: String,
    },
}

#[derive(Clone)]
pub(super) struct Class {
    pub name: String,
    pub namespace: String,
    pub instance: Type,
    pub object: Type,
    pub bases: Vec<u32>,
    pub base_types: Vec<Type>,
    pub any_base_origins: Vec<AnyBaseOrigin>,
    pub scalar_bases: Vec<Type>,
    pub offset: u32,
    pub fields: BTreeMap<String, Field>,
    pub decorated_fields: std::collections::BTreeSet<String>,
    pub cached_properties: std::collections::BTreeSet<String>,
    pub erased_members: std::collections::BTreeSet<String>,
    pub methods: BTreeMap<String, Method>,
    pub plugin: bool,
    pub pydantic: Option<super::pydantic::Model>,
    pub final_class: bool,
    pub typed_dict: Option<typed_dict::Definition>,
    pub dataclass: Option<dataclass::Definition>,
    pub protocol: bool,
    pub runtime_protocol: bool,
    pub generic: Vec<Type>,
}

#[derive(Default)]
pub(super) struct Registry {
    pub constructor_none: BTreeMap<u32, ConstructorNoneState>,
    pub constructor_none_gaps: std::collections::BTreeSet<String>,
    pub keys: BTreeMap<(String, u32), u32>,
    pub entries: Vec<Arc<Class>>,
    pub mro: BTreeMap<u32, Result<Vec<u32>, mro::Error>>,
    pub typed_dict_checks: std::collections::BTreeSet<(u32, u32)>,
    pub typed_dict_methods: BTreeMap<(u32, String), Arc<Signature>>,
    pub embedded_typed_dict_closed: BTreeMap<u32, bool>,
    pub embedded_class_slots: BTreeMap<u32, std::collections::BTreeSet<String>>,
    pub embedded_implicit_fields: BTreeMap<u32, std::collections::BTreeSet<String>>,
    pub embedded_constructor_reveals: BTreeMap<u32, Arc<Signature>>,
    pub embedded_comparisons: BTreeMap<(Type, String), Arc<Signature>>,
    pub protocol_checks: std::collections::BTreeSet<(Type, Type)>,
    pub protocol_constraint_checks: std::collections::BTreeSet<(Type, Type)>,
    pub class_variances: BTreeMap<(u32, usize), super::types::ParameterVariance>,
    pub dataclass_order_variables: BTreeMap<(u32, String), Type>,
    pub inference_defaults: BTreeMap<Type, Type>,
    pub predicate_intersections: BTreeMap<u32, Vec<Type>>,
    pub declared_disjoint_bases: std::collections::BTreeSet<u32>,
    pub declared_slots: BTreeMap<u32, std::collections::BTreeSet<String>>,
    pub pattern_values: BTreeMap<(u32, String), Type>,
    pub builtin_self_variables: BTreeMap<Type, Type>,
    pub dataclass_replace_function: Option<Arc<Signature>>,
    pub dataclass_fields_function: Option<Arc<Signature>>,
    pub dataclass_converters: BTreeMap<String, Arc<Signature>>,
    pub copy_replace_function: Option<Arc<Signature>>,
    pub pydantic_fields: [Option<Arc<Signature>>; 3],
    pub pydantic_computed_fields: [Option<Arc<Signature>>; 3],
    pub pydantic_models: [Option<[Type; 2]>; 3],
    pub pydantic_root_models: [Option<Type>; 3],
    pub standard_collections: [bool; 3],
    pub collection_functions: BTreeMap<String, Arc<Signature>>,
    pub collection_methods: BTreeMap<(Type, String), Arc<Signature>>,
    pub collection_helpers: [bool; 3],
    pub collection_constructor_protocols: [bool; 3],
    pub aggregation_declarations: [bool; 3],
    pub template_library: bool,
    pub template_functions: BTreeMap<String, Arc<Signature>>,
    pub builtin_value_libraries: [bool; 3],
    // Absent until population finishes; false retains unresolved or unrepresented bases.
    pub populated_class_bases: BTreeMap<u32, bool>,
    pub embedded_source_classes: BTreeMap<(String, usize, String), u32>,
    pub native_source_solutions: BTreeMap<(String, usize), std::sync::Arc<BTreeMap<Type, Type>>>,
    pub builtin_values: BTreeMap<(usize, String), Type>,
    pub builtin_value_constructors: BTreeMap<(usize, Type), Arc<Signature>>,
    pub builtin_value_members: BTreeMap<(usize, Type, String), Arc<Signature>>,
    pub builtin_value_aliases: [BTreeMap<String, u32>; 3],
    pub builtin_value_variables: [super::types::Variables; 3],
    pub embedded_pydantic_models: BTreeMap<u32, super::pydantic::Model>,
    pub dataclass_transforms: dataclass::transforms::Registry,
    pub module_targets: BTreeMap<String, mamba_language::application::quality_syntax::PythonTarget>,
    pub module_stubs: BTreeMap<String, bool>,
}

impl Context {
    pub(in crate::infrastructure::mypy) fn set_module_target(
        &self,
        namespace: String,
        target: mamba_language::application::quality_syntax::PythonTarget,
    ) {
        self.classes
            .borrow_mut()
            .module_targets
            .insert(namespace, target);
    }

    pub(in crate::infrastructure::mypy) fn module_target(
        &self,
        namespace: &str,
    ) -> mamba_language::application::quality_syntax::PythonTarget {
        self.classes
            .borrow()
            .module_targets
            .get(namespace)
            .copied()
            .unwrap_or(mamba_language::application::quality_syntax::PythonTarget::Py312)
    }

    pub(in crate::infrastructure::mypy) fn set_module_stub(
        &self,
        namespace: String,
        is_stub: bool,
    ) {
        self.classes
            .borrow_mut()
            .module_stubs
            .insert(namespace, is_stub);
    }

    pub(in crate::infrastructure::mypy) fn module_is_stub(&self, namespace: &str) -> bool {
        self.classes
            .borrow()
            .module_stubs
            .get(namespace)
            .copied()
            .unwrap_or(false)
    }

    pub(in crate::infrastructure::mypy) fn builtin_class(&self, ty: Type) -> u32 {
        let offset = match ty {
            Type::Object => 0,
            Type::Int => 1,
            Type::Float => 2,
            Type::Complex => 3,
            Type::Str => 4,
            Type::Bytes => 5,
            Type::Bool => 6,
            _ => unreachable!("only builtin scalar classes have builtin identities"),
        };
        let key = ("<builtins>".to_string(), offset);
        if let Some(&id) = self.classes.borrow().keys.get(&key) {
            return id;
        }
        let id = self.declare_class("<builtins>", &self.name(ty), offset);
        let mut class = (*self.class(id)).clone();
        class.scalar_bases.push(ty);
        if ty != Type::Object {
            let parent = self.builtin_class(if ty == Type::Bool {
                Type::Int
            } else {
                Type::Object
            });
            class.bases.push(parent);
            class.base_types.push(self.class(parent).instance);
        }
        self.populate_builtin_members(&mut class, ty);
        self.replace_class(id, class);
        self.classes
            .borrow_mut()
            .populated_class_bases
            .insert(id, true);
        id
    }

    fn populate_class_bases(
        &self,
        class: &mut Class,
        statement: &mamba_language::application::quality_syntax::python_ast::StmtClassDef,
        variables: &super::types::Variables,
        missing_bases: &BTreeMap<u32, AnyBaseOrigin>,
        generic_bases: &[TextSize],
    ) -> bool {
        use mamba_language::application::quality_syntax::python_ast::Expr;
        let mut known = true;
        if let Some(arguments) = &statement.arguments {
            for base in &arguments.args {
                if generic_bases.contains(&base.start())
                    && self.verified_generic_base(base, statement, variables, &class.generic)
                {
                    continue;
                }
                if matches!(base, Expr::Name(name) if name.id.as_str() == "type" && !variables.contains_key("type"))
                {
                    let parent = self
                        .runtime_builtin_class_instance(
                            "type",
                            self.module_target(&class.namespace),
                        )
                        .and_then(|ty| self.class_id(ty))
                        .expect("pinned builtin type class");
                    class.bases.push(parent);
                    class.base_types.push(self.class(parent).instance);
                    continue;
                }
                class.plugin |= collect::last_name(base) == Some("BaseModel");
                if let Some(ty) = self.class_base_annotation(base, variables) {
                    let ty = self
                        .nominal_dict_instance(ty, self.module_target(&class.namespace))
                        .unwrap_or(ty);
                    if let Some(base) = self.class_id(ty) {
                        class.bases.push(base);
                        class.base_types.push(ty);
                    } else if matches!(
                        ty,
                        Type::Int
                            | Type::Float
                            | Type::Complex
                            | Type::Str
                            | Type::Bytes
                            | Type::Bool
                            | Type::Object
                    ) {
                        class.scalar_bases.push(ty);
                        class.bases.push(self.builtin_class(ty));
                        class
                            .base_types
                            .push(self.class(self.builtin_class(ty)).instance);
                    } else {
                        known = false;
                        let name = match base {
                            Expr::Name(name) => Some(name.id.to_string()),
                            Expr::Attribute(attribute) => {
                                if let Expr::Name(root) = attribute.value.as_ref() {
                                    Some(format!("{}.{}", root.id, attribute.attr))
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };
                        if ty.is_any()
                            && name.as_ref().is_some_and(|name| {
                                variables.get(name).is_some_and(|ty| ty.is_any())
                            })
                        {
                            class.any_base_origins.push(AnyBaseOrigin::ResolvedAny {
                                range: base.range(),
                            });
                        }
                    }
                } else {
                    known = false;
                    if let Some(origin) = missing_bases.get(&base.start().to_u32()) {
                        class.any_base_origins.push(origin.clone());
                    }
                }
            }
        }
        if class.bases.is_empty() {
            class.bases.push(self.builtin_class(Type::Object));
            class
                .base_types
                .push(self.class(self.builtin_class(Type::Object)).instance);
        }
        known
    }

    pub(super) fn declare_class(&self, namespace: &str, name: &str, offset: u32) -> u32 {
        let key = (namespace.to_string(), offset);
        if let Some(&id) = self.classes.borrow().keys.get(&key) {
            return id;
        }
        let id = u32::try_from(self.classes.borrow().entries.len()).expect("class ID fits in u32");
        let class = Class {
            name: name.into(),
            namespace: namespace.into(),
            instance: self.intern(super::types::Node::Instance(id)),
            object: self.intern(super::types::Node::Class(id)),
            bases: Vec::new(),
            base_types: Vec::new(),
            any_base_origins: Vec::new(),
            scalar_bases: Vec::new(),
            offset,
            fields: BTreeMap::new(),
            decorated_fields: std::collections::BTreeSet::new(),
            cached_properties: std::collections::BTreeSet::new(),
            erased_members: std::collections::BTreeSet::new(),
            methods: BTreeMap::new(),
            plugin: false,
            pydantic: None,
            final_class: false,
            typed_dict: None,
            dataclass: None,
            protocol: false,
            runtime_protocol: false,
            generic: Vec::new(),
        };
        let mut registry = self.classes.borrow_mut();
        registry.native_source_solutions.clear();
        registry.keys.insert(key, id);
        registry.entries.push(Arc::new(class));
        id
    }

    pub(super) fn class(&self, id: u32) -> Arc<Class> {
        self.classes.borrow().entries[id as usize].clone()
    }

    pub(super) fn replace_class(&self, id: u32, class: Class) {
        let mut registry = self.classes.borrow_mut();
        if registry.entries[id as usize].instance != class.instance
            || registry.entries[id as usize].object != class.object
        {
            registry.native_source_solutions.clear();
        }
        registry.class_variances.clear();
        // Fallback method signatures depend on class identity and its TypedDict role.
        if registry.entries[id as usize].instance != class.instance
            || registry.entries[id as usize].typed_dict.is_some() != class.typed_dict.is_some()
        {
            registry
                .typed_dict_methods
                .retain(|(owner, _), _| *owner != id);
        }
        if registry.entries[id as usize].bases != class.bases {
            registry.mro.clear();
        }
        if registry.entries[id as usize].bases != class.bases
            || registry.entries[id as usize].base_types != class.base_types
            || registry.entries[id as usize].scalar_bases != class.scalar_bases
        {
            registry.populated_class_bases.remove(&id);
        }
        registry.entries[id as usize] = Arc::new(class);
    }
}
