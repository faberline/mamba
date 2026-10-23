//! Callable annotation identities depend only on resolved semantic types.
use super::*;
use mamba_language::application::quality_syntax;
use quality_syntax::python_ast::Stmt;

fn scope(context: &Context) -> Variables {
    let mut scope = Variables::from([
        ("Any".into(), Type::Any),
        (
            "T".into(),
            context.variable("T".into(), Type::Object, vec![], None),
        ),
        ("P".into(), context.parameter_spec("P".into(), None)),
        ("Ts".into(), context.tuple_variable("Ts".into(), None)),
    ]);
    for name in ["Concatenate", "Unpack", "TypeGuard", "TypeIs"] {
        scope.insert(name.into(), context.intern(Node::TypingForm(name.into())));
    }
    scope
}

fn annotation(context: &Context, source: &str, scope: &Variables) -> Type {
    context
        .forward_annotation(source, scope)
        .expect("valid annotation")
}

fn populate(context: &Context, id: u32, source: &str, scope: &Variables) {
    let source = format!("{source}\n");
    let document = quality_syntax::parse(&source, PythonTarget::Py314, false).unwrap();
    assert!(document.is_valid());
    let [Stmt::Expr(statement)] = document.parsed.syntax().body.as_slice() else {
        panic!("one annotation expression required")
    };
    context.populate_alias(id, None, &statement.value, scope, None);
}

#[test]
fn bodyless_callable_alias_does_not_capture_its_previous_target() {
    let context = Context::default();
    let mut scope = scope(&context);
    let id = context.declare_alias("identity_case", "Callback", 1, false, false);
    scope.insert("Callback".into(), context.alias_marker(id));
    populate(&context, id, "Callable[..., Any]", &scope);
    let first = context.alias(id).target.unwrap();
    // 32 is the test's observation window. It is not a product iteration limit.
    for turn in 0..32 {
        scope.insert("Callback".into(), context.alias_marker(id));
        populate(&context, id, "Callable[..., Any]", &scope);
        assert_eq!(context.alias(id).target, Some(first), "refresh {turn}");
    }
}

#[test]
fn unrelated_scope_changes_preserve_generic_callable_identity() {
    let context = Context::default();
    let mut scope = scope(&context);
    for (source, used_names) in [
        ("Callable[[T], T]", vec!["T"]),
        ("Callable[P, T]", vec!["P", "T"]),
        ("Callable[Concatenate[int, P], T]", vec!["P", "T"]),
        ("Callable[[Unpack[Ts]], tuple[Unpack[Ts]]]", vec!["Ts"]),
        ("Callable[[], Callable[[T], T]]", vec!["T"]),
        ("Callable[[object], TypeGuard[T]]", vec!["T"]),
        ("Callable[[object], TypeIs[T]]", vec!["T"]),
        ("Callable[['T'], 'T']", vec!["T"]),
    ] {
        let first = annotation(&context, source, &scope);
        let mut free = Vec::new();
        context.collect_variables(first, &mut free);
        for name in used_names {
            assert!(free.contains(&scope[name]), "binder {name} in {source}");
        }
        for turn in 0..32 {
            scope.insert(
                "unrelated".into(),
                if turn % 2 == 0 { Type::Int } else { Type::Str },
            );
            assert_eq!(
                annotation(&context, source, &scope),
                first,
                "{source}: turn {turn}"
            );
        }
    }
}

#[test]
fn real_parameter_return_narrowing_and_binder_changes_change_identity() {
    let context = Context::default();
    let mut scope = scope(&context);
    for (before, after) in [
        ("Callable[[int], int]", "Callable[[str], int]"),
        ("Callable[[int], int]", "Callable[[int], str]"),
        (
            "Callable[[object], TypeGuard[int]]",
            "Callable[[object], TypeGuard[str]]",
        ),
        (
            "Callable[[object], TypeIs[int]]",
            "Callable[[object], TypeIs[str]]",
        ),
        (
            "Callable[[object], TypeGuard[int]]",
            "Callable[[object], TypeIs[int]]",
        ),
    ] {
        assert_ne!(
            annotation(&context, before, &scope),
            annotation(&context, after, &scope)
        );
    }
    let before = annotation(&context, "Callable[[T], T]", &scope);
    let old = context.node(scope["T"]).unwrap();
    let Node::Variable(variable) = old.as_ref() else {
        panic!("variable required")
    };
    let mut changed = variable.clone();
    changed.bound = Type::Str;
    let updated = context.intern(Node::Variable(changed));
    scope.insert("T".into(), updated);
    let after = annotation(&context, "Callable[[T], T]", &scope);
    assert_ne!(before, after);
    let after_signature = context.callable_signature(after).unwrap();
    assert_eq!(after_signature.parameters[0].ty, Some(updated));
    assert_eq!(after_signature.returns, Some(updated));
}

#[test]
fn generic_callable_substitution_preserves_semantic_types() {
    let context = Context::default();
    let scope = scope(&context);
    let original = annotation(&context, "Callable[[T], T]", &scope);
    let signature = context.callable_signature(original).unwrap();
    let substituted =
        context.substitute_callable(&signature, &BTreeMap::from([(scope["T"], Type::Int)]));
    assert_eq!(substituted.parameters[0].ty, Some(Type::Int));
    assert_eq!(substituted.returns, Some(Type::Int));
    assert_ne!(context.callable_type(Arc::new(substituted)), original);
    let original = annotation(&context, "Callable[P, T]", &scope);
    let signature = context.callable_signature(original).unwrap();
    let captured = context.intern(Node::Parameters(
        vec![Parameter {
            name: String::new(),
            ty: Some(Type::Str),
            kind: Kind::PositionalOnly,
            required: true,
        }],
        false,
    ));
    let substituted = context.substitute_callable(
        &signature,
        &BTreeMap::from([(scope["P"], captured), (scope["T"], Type::Int)]),
    );
    assert_eq!(substituted.parameters.len(), 1);
    assert_eq!(substituted.parameters[0].ty, Some(Type::Str));
    assert_eq!(substituted.returns, Some(Type::Int));
}

#[test]
fn referenced_alias_changes_propagate_and_recursive_signatures_stay_finite() {
    let context = Context::default();
    let mut scope = scope(&context);
    let id = context.declare_alias("identity_case", "Payload", 2, false, false);
    context.set_embedded_alias_target(id, Type::Int);
    scope.insert("Payload".into(), context.alias_marker(id));
    let before = annotation(&context, "Callable[[Payload], Payload]", &scope);
    context.set_embedded_alias_target(id, Type::Str);
    scope.insert("Payload".into(), context.alias_marker(id));
    let after = annotation(&context, "Callable[[Payload], Payload]", &scope);
    assert_ne!(before, after);
    let recursive = context.declare_alias("identity_case", "Recursive", 3, false, false);
    scope.insert("Recursive".into(), context.alias_marker(recursive));
    populate(&context, recursive, "Callable[[], 'Recursive']", &scope);
    let first = context.alias(recursive).target.unwrap();
    let signature = context.callable_signature(first).unwrap();
    assert!(
        matches!(context.node(signature.returns.unwrap()).as_deref(),
        Some(Node::AliasInstance(id, arguments)) if *id == recursive && arguments.is_empty())
    );
    for turn in 0..32 {
        scope.insert("Recursive".into(), context.alias_marker(recursive));
        populate(&context, recursive, "Callable[[], 'Recursive']", &scope);
        assert_eq!(
            context.alias(recursive).target,
            Some(first),
            "refresh {turn}"
        );
    }
}
