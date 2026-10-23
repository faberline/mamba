use super::*;

pub(super) fn parse_stmt(src: &str) -> Stmt {
    let module = parser::parse(src, fid()).expect("parse failed");
    module.stmts.into_iter().next().unwrap().node
}
