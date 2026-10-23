use super::*;

// #1580: `case` is a PEP 634 soft keyword — outside `match` arms it
// must parse as an ordinary identifier (attribute, variable, parameter
// name). CPython tests use `self.case = "empty set"` as a common idiom.
#[test]
pub(super) fn test_case_as_attribute_name() {
    let module = parse(
        "class C:\n\
         \x20   def setUp(self):\n\
         \x20       self.case = \"empty set\"\n\
         \x20       self.match = \"ok\"\n",
    );
    assert_eq!(module.stmts.len(), 1, "class def should parse");
}
