use super::*;

// --- Built-in type class patterns (#827) ---

#[test]
pub(super) fn test_builtin_int_class_pattern() {
    let src = "match x:\n    case int(n):\n        pass\n";
    let module = parser::parse(src, fid()).expect("parse failed");
    match module.stmts.into_iter().next().unwrap().node {
        Stmt::Match { arms, .. } => match &arms[0].pattern.node {
            Pattern::ClassPattern { cls, patterns } => {
                assert_eq!(cls, &vec!["int".to_string()]);
                assert_eq!(patterns.len(), 1);
                assert_eq!(patterns[0].0, None);
                assert!(matches!(&patterns[0].1.node, Pattern::Binding(n) if n == "n"));
            }
            other => panic!("expected ClassPattern, got {other:?}"),
        },
        other => panic!("expected Match, got {other:?}"),
    }
}
