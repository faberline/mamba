use super::*;

#[test]
pub(super) fn test_type_params() {
    let src = "def first[T](items: list[int]) -> int:\n    return 0\n";
    let module = parse(src);
    match &module.stmts[0].node {
        Stmt::FnDef { type_params, .. } => {
            let names: Vec<&str> = type_params.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(names, vec!["T"]);
        }
        _ => panic!("expected FnDef with type params"),
    }
}
