use super::*;

#[test]
pub(super) fn test_star_pattern_anonymous_rejected_standalone() {
    // `case *_:` is invalid PEP 634 — star patterns only inside sequences.
    let src = "match x:\n    case *_:\n        pass\n";
    let result = parser::parse(src, fid());
    assert!(
        result.is_err(),
        "standalone star wildcard should be rejected"
    );
}
