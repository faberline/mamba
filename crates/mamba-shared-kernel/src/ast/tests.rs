use super::*;

#[test]
fn test_forward_ref_name_encoding_decoding() {
    let name = "Node";
    let encoded = forward_ref_name(name);
    assert_eq!(encoded, "__mamba_forward_ref__:Node");
    assert_eq!(strip_forward_ref_name(&encoded), Some("Node"));
    assert_eq!(strip_forward_ref_name("Node"), None);
    assert_eq!(strip_forward_ref_name("other_prefix:Node"), None);
}

#[test]
fn test_type_param_plain_construction() {
    let tp = TypeParam::plain("T");
    assert_eq!(tp.name, "T");
    assert_eq!(tp.kind, TypeParamKind::TypeVar);
    assert!(tp.bound.is_none());
    assert!(tp.constraints.is_none());
    assert!(tp.default.is_none());
}

#[test]
fn test_type_param_kinds() {
    assert_eq!(TypeParamKind::TypeVar, TypeParamKind::TypeVar);
    assert_ne!(TypeParamKind::TypeVar, TypeParamKind::TypeVarTuple);
    assert_ne!(TypeParamKind::ParamSpec, TypeParamKind::TypeVar);
}

#[test]
fn test_type_expr_variants() {
    let t_named = TypeExpr::Named("int".to_string());
    let t_opt = TypeExpr::Optional(Box::new(Spanned::new(t_named.clone(), Span::dummy())));
    let t_tuple = TypeExpr::Tuple(vec![Spanned::new(t_named.clone(), Span::dummy())]);

    assert_eq!(t_named, TypeExpr::Named("int".to_string()));
    assert_ne!(t_named, t_opt);
    if let TypeExpr::Tuple(elems) = t_tuple {
        assert_eq!(elems.len(), 1);
        assert_eq!(elems[0].node, TypeExpr::Named("int".to_string()));
    } else {
        panic!("Expected TypeExpr::Tuple");
    }
}

#[test]
fn test_operators_enums() {
    assert_eq!(UnaryOp::Neg, UnaryOp::Neg);
    assert_ne!(UnaryOp::Neg, UnaryOp::Not);
    assert_eq!(BinOp::Add, BinOp::Add);
    assert_ne!(BinOp::Add, BinOp::Sub);
    assert_eq!(AugOp::Add, AugOp::Add);
}
