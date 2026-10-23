use super::*;

// ── R5: f-string parsing ──

#[test]
pub(super) fn test_fstring_simple_expression() {
    // Verify that f"{x}" produces an FString node with one Expr part
    let src = "x: int = 1\ny: str = f\"{x}\"\n";
    let module = parse(src);
    match &module.stmts[1].node {
        Stmt::VarDecl { value: val, .. } => {
            assert!(
                matches!(&val.node, Expr::FString(_)),
                "expected FString, got {:?}",
                val.node
            );
            if let Expr::FString(parts) = &val.node {
                assert_eq!(parts.len(), 1, "should have one Expr part");
                assert!(matches!(&parts[0], FStringPart::Expr(_, None)));
            }
        }
        other => panic!("expected VarDecl with fstring, got {other:?}"),
    }
}
