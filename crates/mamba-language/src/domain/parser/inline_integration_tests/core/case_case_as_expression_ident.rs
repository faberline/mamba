use super::*;

// #1580: `case` must also be usable as a bare identifier in expression
// position (e.g. `expected = case in self.cases`).
#[test]
pub(super) fn test_case_as_expression_ident() {
    let module = parse(
        "def f(case):\n\
         \x20   return case\n",
    );
    assert_eq!(
        module.stmts.len(),
        1,
        "function with case parameter should parse"
    );
}
