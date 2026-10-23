//! Target-aware model for the pinned stdlib `ast.Constant` declarations.
use super::super::{
    classes::{Abstractness, Field, Method, MethodKind},
    signatures::{Kind, Parameter, Signature},
};
use super::literals::Literal;
use super::{Context, Node, Type};
use mamba_language::application::quality_syntax::PythonTarget;
use std::sync::Arc;

impl Context {
    pub(in crate::infrastructure::mypy) fn ast_constant_class(&self, target: PythonTarget) -> Type {
        let ast_id = self.ast_class(target, "AST");
        let expr_id = self.ast_class(target, "expr");
        let mut expr = (*self.class(expr_id)).clone();
        if expr.bases.is_empty() {
            expr.bases.push(ast_id);
            expr.base_types.push(self.class(ast_id).instance);
        }
        self.add_expr_positions(&mut expr);
        self.replace_class(expr_id, expr);

        let id = self.declare_class(
            &format!("ast@{}.Constant", target_name(target)),
            "Constant",
            0,
        );
        let instance = self.class(id).instance;
        let mut class = (*self.class(id)).clone();
        if class.bases.is_empty() {
            class.bases.push(expr_id);
            class.base_types.push(self.class(expr_id).instance);
        }

        self.replace_class(id, class);
        self.populate_ast_constant(id, target);
        instance
    }

    // The source-backed ast module calls this with its own Constant identity.
    // Keep the established model while support modules still lack full types.
    pub(in crate::infrastructure::mypy) fn populate_ast_constant(
        &self,
        id: u32,
        target: PythonTarget,
    ) {
        let instance = self.class(id).instance;
        let mut class = (*self.class(id)).clone();
        let value = self.ast_constant_value_type();
        let kind = self.union(vec![Type::Str, Type::None]);
        self.add_field(&mut class, "value", value, false);
        self.add_field(&mut class, "kind", kind, false);
        self.add_field(
            &mut class,
            "__match_args__",
            self.string_tuple(&["value", "kind"]),
            true,
        );
        if target != PythonTarget::Py314 {
            self.add_property(&mut class, "n", instance, value);
            self.add_property(&mut class, "s", instance, value);
        }
        self.add_constructor(&mut class, instance, value, kind);
        if target == PythonTarget::Py314 {
            self.add_replace_method(&mut class, instance, value, kind);
        }
        self.replace_class(id, class);
    }

    fn ast_class(&self, target: PythonTarget, name: &str) -> u32 {
        let id = self.declare_class(&format!("ast@{}.{}", target_name(target), name), name, 0);
        if name == "AST" && self.class(id).bases.is_empty() {
            let object = self.builtin_class(Type::Object);
            let mut class = (*self.class(id)).clone();
            class.bases.push(object);
            class.base_types.push(self.class(object).instance);
            let variadic_strings = self.intern(Node::VariadicTuple(Type::Str));
            self.add_field(
                &mut class,
                "__match_args__",
                self.intern(Node::Tuple(Vec::new())),
                true,
            );
            self.add_field(&mut class, "_attributes", variadic_strings, true);
            self.add_field(&mut class, "_fields", variadic_strings, true);
            if target != PythonTarget::Py312 {
                self.add_field(
                    &mut class,
                    "_field_types",
                    self.intern(Node::Container(
                        super::Container::Dict,
                        vec![Type::Str, Type::Any],
                    )),
                    true,
                );
            }
            if target == PythonTarget::Py314 {
                let self_type =
                    self.self_variable("mamba.ast.AST.__replace__", self.class(id).instance);
                let mut signature = Signature::empty("__replace__".into(), self_type);
                signature.owner = Some("AST".into());
                signature.definition_module = Some("ast".into());
                signature.constructor = false;
                signature.generic = vec![self_type].into();
                signature.parameters = vec![Parameter {
                    name: "self".into(),
                    ty: Some(self_type),
                    kind: Kind::PositionalOnly,
                    required: true,
                }]
                .into();
                class.methods.insert(
                    "__replace__".into(),
                    Method {
                        signature: Arc::new(signature),
                        kind: MethodKind::Instance,
                        setter: None,
                        final_method: false,
                        abstractness: Abstractness::Concrete,
                    },
                );
            }
            self.replace_class(id, class);
        }
        id
    }

    fn add_expr_positions(&self, class: &mut super::super::classes::Class) {
        for (name, ty) in [
            ("lineno", Type::Int),
            ("col_offset", Type::Int),
            ("end_lineno", self.union(vec![Type::Int, Type::None])),
            ("end_col_offset", self.union(vec![Type::Int, Type::None])),
        ] {
            self.add_field(class, name, ty, false);
        }
    }

    fn add_field(
        &self,
        class: &mut super::super::classes::Class,
        name: &str,
        ty: Type,
        class_access: bool,
    ) {
        class.fields.insert(
            name.into(),
            Field {
                ty: Some(ty),
                needs_annotation: false,
                class_access,
                readonly: false,
                final_attribute: false,
                class_var: class_access,
                abstract_field: false,
            },
        );
    }

    fn add_property(
        &self,
        class: &mut super::super::classes::Class,
        name: &str,
        instance: Type,
        value: Type,
    ) {
        // The deprecated n/s property exposes the same bool-free read type as its setter.
        let legacy_value = match self.node(value).as_deref() {
            Some(Node::Union(items)) if items.contains(&Type::Int) => self.union(
                items
                    .iter()
                    .copied()
                    .filter(|&item| item != Type::Bool)
                    .collect(),
            ),
            _ => value,
        };
        let mut getter = Signature::empty(name.into(), legacy_value);
        getter.owner = Some("Constant".into());
        getter.definition_module = Some("ast".into());
        getter.constructor = false;
        getter.parameters = vec![Parameter {
            name: "self".into(),
            ty: Some(instance),
            kind: Kind::PositionalOnly,
            required: true,
        }]
        .into();
        // Legacy n/s getter and setter values use this projection; declarations keep the full union.
        let write_value = legacy_value;
        let mut setter = Signature::empty(name.into(), Type::None);
        setter.owner = Some("Constant".into());
        setter.definition_module = Some("ast".into());
        setter.constructor = false;
        setter.parameters = vec![
            Parameter {
                name: "self".into(),
                ty: Some(instance),
                kind: Kind::PositionalOnly,
                required: true,
            },
            Parameter {
                name: "value".into(),
                ty: Some(write_value),
                kind: Kind::PositionalOnly,
                required: true,
            },
        ]
        .into();
        class.methods.insert(
            name.into(),
            Method {
                signature: Arc::new(getter),
                kind: MethodKind::Property,
                setter: Some(Arc::new(setter)),
                final_method: false,
                abstractness: Abstractness::Concrete,
            },
        );
    }

    fn add_constructor(
        &self,
        class: &mut super::super::classes::Class,
        instance: Type,
        value: Type,
        kind_type: Type,
    ) {
        let mut signature = Signature::empty("__init__".into(), Type::None);
        signature.owner = Some("Constant".into());
        signature.definition_module = Some("ast".into());
        signature.constructor = false;
        signature.parameters = vec![
            Parameter {
                name: "self".into(),
                ty: Some(instance),
                kind: Kind::PositionalOnly,
                required: true,
            },
            Parameter {
                name: "value".into(),
                ty: Some(value),
                kind: Kind::PositionalOrKeyword,
                required: true,
            },
            Parameter {
                name: "kind".into(),
                ty: Some(kind_type),
                kind: Kind::PositionalOrKeyword,
                required: false,
            },
            Parameter {
                name: "lineno".into(),
                ty: Some(Type::Int),
                kind: Kind::KeywordOnly,
                required: false,
            },
            Parameter {
                name: "col_offset".into(),
                ty: Some(Type::Int),
                kind: Kind::KeywordOnly,
                required: false,
            },
            Parameter {
                name: "end_lineno".into(),
                ty: Some(self.union(vec![Type::Int, Type::None])),
                kind: Kind::KeywordOnly,
                required: false,
            },
            Parameter {
                name: "end_col_offset".into(),
                ty: Some(self.union(vec![Type::Int, Type::None])),
                kind: Kind::KeywordOnly,
                required: false,
            },
        ]
        .into();
        class.methods.insert(
            "__init__".into(),
            Method {
                signature: Arc::new(signature),
                kind: MethodKind::Instance,
                setter: None,
                final_method: false,
                abstractness: Abstractness::Concrete,
            },
        );
    }

    fn add_replace_method(
        &self,
        class: &mut super::super::classes::Class,
        instance: Type,
        value: Type,
        kind_type: Type,
    ) {
        let self_type = self.self_variable("mamba.ast.Constant.__replace__", instance);
        let mut signature = Signature::empty("__replace__".into(), self_type);
        signature.owner = Some("Constant".into());
        signature.definition_module = Some("ast".into());
        signature.constructor = false;
        signature.generic = vec![self_type].into();
        signature.parameters = vec![
            Parameter {
                name: "self".into(),
                ty: Some(self_type),
                kind: Kind::PositionalOnly,
                required: true,
            },
            Parameter {
                name: "value".into(),
                ty: Some(value),
                kind: Kind::KeywordOnly,
                required: false,
            },
            Parameter {
                name: "kind".into(),
                ty: Some(kind_type),
                kind: Kind::KeywordOnly,
                required: false,
            },
            Parameter {
                name: "lineno".into(),
                ty: Some(Type::Int),
                kind: Kind::KeywordOnly,
                required: false,
            },
            Parameter {
                name: "col_offset".into(),
                ty: Some(Type::Int),
                kind: Kind::KeywordOnly,
                required: false,
            },
            Parameter {
                name: "end_lineno".into(),
                ty: Some(self.union(vec![Type::Int, Type::None])),
                kind: Kind::KeywordOnly,
                required: false,
            },
            Parameter {
                name: "end_col_offset".into(),
                ty: Some(self.union(vec![Type::Int, Type::None])),
                kind: Kind::KeywordOnly,
                required: false,
            },
        ]
        .into();
        class.methods.insert(
            "__replace__".into(),
            Method {
                signature: Arc::new(signature),
                kind: MethodKind::Instance,
                setter: None,
                final_method: false,
                abstractness: Abstractness::Concrete,
            },
        );
    }

    fn self_variable(&self, symbol: &'static str, bound: Type) -> Type {
        // Source class identities include the project namespace. A target-only
        // variable key would merge Self binders across separate projects.
        let position = self.class_id(bound).expect("AST Self has a class bound");
        self.native_variable(symbol, position, "Self".into(), bound)
    }

    pub(super) fn ast_constant_value_type(&self) -> Type {
        let ellipsis = self.declare_class("types.EllipsisType", "EllipsisType", 0);
        self.union(vec![
            Type::Str,
            Type::Bytes,
            Type::Bool,
            Type::Int,
            Type::Float,
            Type::Complex,
            Type::None,
            self.class(ellipsis).instance,
        ])
    }

    fn string_tuple(&self, values: &[&str]) -> Type {
        self.intern(Node::Tuple(
            values
                .iter()
                .map(|value| self.intern(Node::Literal(Literal::Str((*value).into()))))
                .collect(),
        ))
    }
}

fn target_name(target: PythonTarget) -> &'static str {
    match target {
        PythonTarget::Py312 => "3.12",
        PythonTarget::Py313 => "3.13",
        PythonTarget::Py314 => "3.14",
    }
}
