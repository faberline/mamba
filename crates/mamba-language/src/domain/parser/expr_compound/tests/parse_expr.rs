use super::*;

pub(super) fn parse_expr(src: &str) -> Expr {
    let full = format!("{src}\n");
    let module = parser::parse(&full, fid()).expect("parse failed");
    match module.stmts.into_iter().next().unwrap().node {
        Stmt::ExprStmt(e) => e.node,
        other => panic!("expected ExprStmt, got {other:?}"),
    }
}
