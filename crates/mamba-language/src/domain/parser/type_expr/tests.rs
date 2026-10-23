use crate::parser;
use crate::source::span::FileId;
use mamba_shared_kernel::ast::*;

fn fid() -> FileId {
    FileId(0)
}
/// Parse `x: <type> = 0` and return the TypeExpr.
fn parse_type(ty_src: &str) -> TypeExpr {
    let src = format!("x: {ty_src} = 0\n");
    let module = parser::parse(&src, fid()).expect("parse failed");
    match module.stmts.into_iter().next().unwrap().node {
        Stmt::VarDecl { ty, .. } => ty.node,
        other => panic!("expected VarDecl, got {other:?}"),
    }
}

// --- Named types ---

#[test]
fn test_named_int() {
    match parse_type("int") {
        TypeExpr::Named(n) => assert_eq!(n, "int"),
        other => panic!("expected Named('int'), got {other:?}"),
    }
}

#[test]
fn test_named_float() {
    match parse_type("float") {
        TypeExpr::Named(n) => assert_eq!(n, "float"),
        other => panic!("expected Named('float'), got {other:?}"),
    }
}

#[test]
fn test_named_bool() {
    match parse_type("bool") {
        TypeExpr::Named(n) => assert_eq!(n, "bool"),
        other => panic!("expected Named('bool'), got {other:?}"),
    }
}

#[test]
fn test_named_str() {
    match parse_type("str") {
        TypeExpr::Named(n) => assert_eq!(n, "str"),
        other => panic!("expected Named('str'), got {other:?}"),
    }
}

#[test]
fn test_named_user_type() {
    match parse_type("MyClass") {
        TypeExpr::Named(n) => assert_eq!(n, "MyClass"),
        other => panic!("expected Named('MyClass'), got {other:?}"),
    }
}

#[test]
fn test_named_none() {
    match parse_type("None") {
        TypeExpr::Named(n) => assert_eq!(n, "None"),
        other => panic!("expected Named('None'), got {other:?}"),
    }
}

// --- Generic types ---

#[test]
fn test_generic_list_int() {
    match parse_type("list[int]") {
        TypeExpr::Generic { name, args } => {
            assert_eq!(name, "list");
            assert_eq!(args.len(), 1);
            assert!(matches!(&args[0].node, TypeExpr::Named(n) if n == "int"));
        }
        other => panic!("expected Generic, got {other:?}"),
    }
}

#[test]
fn test_generic_dict() {
    match parse_type("dict[str, int]") {
        TypeExpr::Generic { name, args } => {
            assert_eq!(name, "dict");
            assert_eq!(args.len(), 2);
            assert!(matches!(&args[0].node, TypeExpr::Named(n) if n == "str"));
            assert!(matches!(&args[1].node, TypeExpr::Named(n) if n == "int"));
        }
        other => panic!("expected Generic, got {other:?}"),
    }
}

#[test]
fn test_generic_user_type() {
    match parse_type("Optional[int]") {
        TypeExpr::Generic { name, args } => {
            assert_eq!(name, "Optional");
            assert_eq!(args.len(), 1);
        }
        other => panic!("expected Generic, got {other:?}"),
    }
}

// --- Tuple type ---

#[test]
fn test_tuple_type() {
    match parse_type("tuple[int, str]") {
        TypeExpr::Tuple(args) => {
            assert_eq!(args.len(), 2);
            assert!(matches!(&args[0].node, TypeExpr::Named(n) if n == "int"));
            assert!(matches!(&args[1].node, TypeExpr::Named(n) if n == "str"));
        }
        other => panic!("expected Tuple, got {other:?}"),
    }
}

#[test]
fn test_tuple_type_starred_arg_with_trailing_comma() {
    match parse_type("tuple[*Ts,]") {
        TypeExpr::Tuple(args) => {
            assert_eq!(args.len(), 1);
            assert!(matches!(
                &args[0].node,
                TypeExpr::Unpack(inner) if matches!(&inner.node, TypeExpr::Named(n) if n == "Ts")
            ));
        }
        other => panic!("expected Tuple, got {other:?}"),
    }
}

#[test]
fn test_tuple_type_preserves_type_var_tuple_unpack_order() {
    match parse_type("tuple[Head, *Ts, Tail]") {
        TypeExpr::Tuple(args) => {
            assert_eq!(args.len(), 3);
            assert!(matches!(&args[0].node, TypeExpr::Named(n) if n == "Head"));
            assert!(matches!(
                &args[1].node,
                TypeExpr::Unpack(inner) if matches!(&inner.node, TypeExpr::Named(n) if n == "Ts")
            ));
            assert!(matches!(&args[2].node, TypeExpr::Named(n) if n == "Tail"));
        }
        other => panic!("expected Tuple, got {other:?}"),
    }
}

// --- Optional ---

#[test]
fn test_optional_shorthand() {
    match parse_type("int?") {
        TypeExpr::Optional(inner) => {
            assert!(matches!(&inner.node, TypeExpr::Named(n) if n == "int"));
        }
        other => panic!("expected Optional, got {other:?}"),
    }
}

// --- Union ---

#[test]
fn test_union_two() {
    match parse_type("int | str") {
        TypeExpr::Union(types) => {
            assert_eq!(types.len(), 2);
            assert!(matches!(&types[0].node, TypeExpr::Named(n) if n == "int"));
            assert!(matches!(&types[1].node, TypeExpr::Named(n) if n == "str"));
        }
        other => panic!("expected Union, got {other:?}"),
    }
}

#[test]
fn test_union_three() {
    match parse_type("int | str | float") {
        TypeExpr::Union(types) => {
            assert_eq!(types.len(), 3);
        }
        other => panic!("expected Union with 3, got {other:?}"),
    }
}

// --- Function type ---

#[test]
fn test_fn_type() {
    match parse_type("(int, str) -> bool") {
        TypeExpr::Fn { params, ret } => {
            assert_eq!(params.len(), 2);
            assert!(matches!(&ret.node, TypeExpr::Named(n) if n == "bool"));
        }
        other => panic!("expected Fn, got {other:?}"),
    }
}

#[test]
fn test_fn_type_no_params() {
    match parse_type("() -> int") {
        TypeExpr::Fn { params, ret } => {
            assert!(params.is_empty());
            assert!(matches!(&ret.node, TypeExpr::Named(n) if n == "int"));
        }
        other => panic!("expected Fn, got {other:?}"),
    }
}

// --- Parenthesized type ---

#[test]
fn test_parenthesized_type() {
    // `(int)` should unwrap to just `int`
    match parse_type("(int)") {
        TypeExpr::Named(n) => assert_eq!(n, "int"),
        other => panic!("expected Named through paren, got {other:?}"),
    }
}

// --- Bare container without brackets ---

#[test]
fn test_list_without_brackets() {
    // `list` without brackets is just Named("list")
    match parse_type("list") {
        TypeExpr::Named(n) => assert_eq!(n, "list"),
        other => panic!("expected Named('list'), got {other:?}"),
    }
}

// --- Nested generics ---

#[test]
fn test_nested_generic() {
    match parse_type("list[dict[str, int]]") {
        TypeExpr::Generic { name, args } => {
            assert_eq!(name, "list");
            assert_eq!(args.len(), 1);
            assert!(matches!(&args[0].node, TypeExpr::Generic { .. }));
        }
        other => panic!("expected nested Generic, got {other:?}"),
    }
}

#[test]
fn test_generic_type_arg_with_trailing_comma() {
    match parse_type("Box[int,]") {
        TypeExpr::Generic { name, args } => {
            assert_eq!(name, "Box");
            assert_eq!(args.len(), 1);
            assert!(matches!(&args[0].node, TypeExpr::Named(n) if n == "int"));
        }
        other => panic!("expected Generic, got {other:?}"),
    }
}

#[test]
fn test_forward_ref_call_annotation() {
    match parse_type("ForwardRef('CC')") {
        TypeExpr::Named(n) => assert_eq!(strip_forward_ref_name(&n), Some("CC")),
        other => panic!("expected forward-ref Named, got {other:?}"),
    }
    match parse_type("typing.ForwardRef('int', module='mod')") {
        TypeExpr::Named(n) => assert_eq!(strip_forward_ref_name(&n), Some("int")),
        other => panic!("expected forward-ref Named, got {other:?}"),
    }
    match parse_type("list[list[ForwardRef('CC')]]") {
        TypeExpr::Generic { name, args } => {
            assert_eq!(name, "list");
            assert!(matches!(&args[0].node, TypeExpr::Generic { .. }));
        }
        other => panic!("expected nested Generic, got {other:?}"),
    }
}

// --- Error cases ---

#[test]
fn test_literal_annotation_resolves_to_any() {
    // PEP 3107/526: an annotation is an arbitrary expression, never validated as
    // a type (CPython only stores it in __annotations__). A literal value carries
    // no type constraint, so `x: 42` must PARSE, not fail. Simple numeric/bool
    // literals keep their textual form (`Named("42")`) so introspection can
    // render them (`str(inspect.signature(f))` → `-> 42`); the checker resolves
    // digit-leading Named types to Any. Non-literal expressions resolve to Any.
    assert_eq!(parse_type("42"), TypeExpr::Named("42".to_string()));
    assert_eq!(parse_type("{1: 2}"), TypeExpr::Named("Any".to_string()));
    assert_eq!(parse_type("[int]"), TypeExpr::Named("Any".to_string()));
}

#[test]
fn test_type_error_on_non_expression_token() {
    // A token that cannot even start an expression (a stray `,`) still errors.
    let src = "x: , = 0\n";
    assert!(parser::parse(src, fid()).is_err());
}

// --- Optional after union ---

#[test]
fn test_optional_after_union() {
    // `int | str?` should make Optional around the union
    match parse_type("int | str?") {
        // The `?` applies after union is fully parsed
        TypeExpr::Optional(inner) => {
            assert!(matches!(&inner.node, TypeExpr::Union(_)));
        }
        other => panic!("expected Optional(Union), got {other:?}"),
    }
}
