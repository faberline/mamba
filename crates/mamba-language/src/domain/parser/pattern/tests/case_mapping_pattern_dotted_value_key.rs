use super::*;

#[test]
pub(super) fn test_mapping_pattern_dotted_value_key() {
    match parse_pattern("{Keys.KEY: value}") {
        Pattern::Mapping { pairs, rest } => {
            assert_eq!(pairs.len(), 1);
            assert!(rest.is_none());
            match &pairs[0].0.node {
                Expr::Attr { object, attr } => {
                    assert_eq!(attr, "KEY");
                    assert!(matches!(&object.node, Expr::Ident(n) if n == "Keys"));
                }
                other => panic!("expected dotted value key, got {other:?}"),
            }
            assert!(matches!(&pairs[0].1.node, Pattern::Binding(n) if n == "value"));
        }
        other => panic!("expected Mapping, got {other:?}"),
    }
}
