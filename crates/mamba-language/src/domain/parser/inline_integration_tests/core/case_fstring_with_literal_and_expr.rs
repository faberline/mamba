use super::*;

#[test]
pub(super) fn test_fstring_with_literal_and_expr() {
    // f"hello {name}" produces [Literal("hello "), Expr(name)]
    let src = "name: str = \"world\"\ns: str = f\"hello {name}\"\n";
    let module = parse(src);
    match &module.stmts[1].node {
        Stmt::VarDecl { value: val, .. } => {
            if let Expr::FString(parts) = &val.node {
                assert_eq!(parts.len(), 2);
                assert!(matches!(&parts[0], FStringPart::Literal(s) if s == "hello "));
                assert!(matches!(&parts[1], FStringPart::Expr(_, None)));
            } else {
                panic!("expected FString, got {:?}", val.node);
            }
        }
        other => panic!("expected VarDecl, got {other:?}"),
    }
}
