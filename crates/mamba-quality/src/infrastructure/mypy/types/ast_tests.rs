use super::literals::Literal;
use super::{Context, Node, Type};
use crate::infrastructure::mypy::signatures::Kind;
use mamba_language::application::quality_syntax::PythonTarget;

fn tuple_text(context: &Context, ty: Type, expected: &[&str]) {
    let tuple_node = context.node(ty).expect("tuple node");
    let Node::Tuple(items) = tuple_node.as_ref() else {
        panic!("expected fixed string tuple");
    };
    let actual: Vec<String> = items
        .iter()
        .map(|item| {
            let item_node = context.node(*item).expect("tuple item node");
            match item_node.as_ref() {
                Node::Literal(Literal::Str(value)) => value.clone(),
                other => panic!("expected string literal, got {other:?}"),
            }
        })
        .collect();
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>()
    );
}

#[test]
fn constant_api_shape_is_target_aware_and_reusable() {
    let context = Context::default();
    let targets = [
        PythonTarget::Py312,
        PythonTarget::Py313,
        PythonTarget::Py314,
    ];
    let instances: Vec<_> = targets
        .iter()
        .map(|target| context.ast_constant_class(*target))
        .collect();
    assert_ne!(instances[0], instances[1]);
    assert_ne!(instances[1], instances[2]);
    for (target, instance) in targets.into_iter().zip(instances) {
        assert_eq!(context.ast_constant_class(target), instance);
        assert_eq!(
            context
                .class_field(instance, "value")
                .and_then(|field| field.ty),
            Some(context.ast_constant_value_type())
        );
        assert_eq!(
            context
                .class_field(instance, "kind")
                .and_then(|field| field.ty),
            Some(context.union(vec![Type::Str, Type::None]))
        );
        for (name, ty) in [
            ("lineno", Some(Type::Int)),
            ("col_offset", Some(Type::Int)),
            (
                "end_lineno",
                Some(context.union(vec![Type::Int, Type::None])),
            ),
            (
                "end_col_offset",
                Some(context.union(vec![Type::Int, Type::None])),
            ),
        ] {
            assert_eq!(
                context
                    .class_field(instance, name)
                    .and_then(|field| field.ty),
                ty
            );
        }
        let class = context.class_value_type(instance);
        let initializer = context
            .class_constructor_for(class, target)
            .expect("Constant constructor");
        let parameters = initializer.parameters.as_ref();
        assert_eq!(
            parameters
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "value",
                "kind",
                "lineno",
                "col_offset",
                "end_lineno",
                "end_col_offset"
            ]
        );
        assert_eq!(parameters[0].kind, Kind::PositionalOrKeyword);
        assert!(parameters[0].required);
        assert!(!parameters[1].required);
        for parameter in &parameters[2..] {
            assert_eq!(parameter.kind, Kind::KeywordOnly);
            assert!(!parameter.required);
        }
        for name in ["_fields", "_attributes"] {
            let ty = context
                .class_field(class, name)
                .and_then(|field| field.ty)
                .expect("inherited AST tuple member");
            let field_node = context.node(ty).expect("inherited tuple node");
            assert!(matches!(
                field_node.as_ref(),
                Node::VariadicTuple(Type::Str)
            ));
        }
        let match_args = context
            .class_field(class, "__match_args__")
            .and_then(|field| field.ty)
            .expect("Constant match args");
        tuple_text(&context, match_args, &["value", "kind"]);
        assert_eq!(
            context.class_field(instance, "n").is_some(),
            target != PythonTarget::Py314
        );
        assert_eq!(
            context.class_field(instance, "s").is_some(),
            target != PythonTarget::Py314
        );
        // Original n/s write diagnostics use this normalized union, while
        // Constant.value and constructor declarations retain bool explicitly.
        let ellipsis = context.declare_class("types.EllipsisType", "EllipsisType", 0);
        let value_type = context.union(vec![
            Type::Str,
            Type::Bytes,
            Type::Int,
            Type::Float,
            Type::Complex,
            Type::None,
            context.class(ellipsis).instance,
        ]);
        assert_eq!(
            context.class_write_type(instance, "n"),
            (target != PythonTarget::Py314).then_some(value_type)
        );
        assert_eq!(
            context.class_write_type(instance, "s"),
            (target != PythonTarget::Py314).then_some(value_type)
        );
        assert_eq!(
            context.class_field(class, "_field_types").is_some(),
            target != PythonTarget::Py312
        );
        let replace = context.class_method(instance, "__replace__");
        assert_eq!(replace.is_some(), target == PythonTarget::Py314);
        if let Some(replace) = replace {
            assert_eq!(
                replace
                    .parameters
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>(),
                vec![
                    "value",
                    "kind",
                    "lineno",
                    "col_offset",
                    "end_lineno",
                    "end_col_offset"
                ]
            );
            assert!(replace
                .parameters
                .iter()
                .all(|parameter| parameter.kind == Kind::KeywordOnly && !parameter.required));
            assert_eq!(replace.returns, Some(instance));
        }
    }
}

#[test]
fn ellipsis_is_part_of_constant_value_union() {
    let context = Context::default();
    let value = context.ast_constant_value_type();
    let value_node = context.node(value).expect("Constant value union");
    let Node::Union(items) = value_node.as_ref() else {
        panic!("expected Constant value union");
    };
    for scalar in [
        Type::Str,
        Type::Bytes,
        Type::Bool,
        Type::Int,
        Type::Float,
        Type::Complex,
        Type::None,
    ] {
        assert!(items.contains(&scalar), "missing scalar member {scalar:?}");
    }
    let has_ellipsis = items.iter().any(|item| match *item {
        Type::Node(_) => context.node(*item).is_some_and(|member| {
            matches!(member.as_ref(), Node::Instance(id) if context.class(*id).name == "EllipsisType")
        }),
        _ => false,
    });
    assert!(has_ellipsis, "missing EllipsisType union member");
}

#[test]
fn replace_returns_the_concrete_subclass() {
    let context = Context::default();
    let constant = context.ast_constant_class(PythonTarget::Py314);
    let constant_id = context.class_id(constant).expect("Constant class ID");
    let child_id = context.declare_class("ast.test.Child", "Child", 0);
    let mut child = (*context.class(child_id)).clone();
    child.bases.push(constant_id);
    child.base_types.push(constant);
    context.replace_class(child_id, child);
    let child_instance = context.class(child_id).instance;
    let replace = context
        .class_method(child_instance, "__replace__")
        .expect("inherited replace method");
    assert_eq!(replace.returns, Some(child_instance));
}
