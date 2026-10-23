//! Full parsing and comments-only views must retain the same complete syntax.
use crate::application::quality_syntax::{
    parse, parse_data, parse_data_for_type_inference, PythonTarget, SyntaxDocument,
};
use std::sync::Arc;

const CASES: &[(&str, &str)] = &[
    ("unicode", "標籤 = '水'  # type: ignore[assignment]\n"),
    ("crlf", "# mypy: ignore-errors\r\nvalue = 1\r\n"),
    (
        "comment_backslash",
        "# comment with \\\nvalue = 1  # type: ignore[arg-type] \\\n",
    ),
    ("empty", ""),
    ("trailing_comment", "value = 1  # tail with no newline"),
    ("invalid_syntax", "value = (  # type: ignore[syntax]\n"),
    ("type_default", "type Items[T = int] = list[T]\n"),
    ("variadic_default", "type Items[**P = [int, str]] = int\n"),
    (
        "template_string",
        "message = t\"hello {name!r}\"  # comment\n",
    ),
    ("string_hash", "value = '# not a comment'  # real comment\n"),
    ("continuation", "value = 1 + \\\n    2  # comment\n"),
    ("invalid_character", "value = $  # comment\n"),
];

#[test]
fn comments_view_matches_full_filter_without_losing_native_syntax() {
    for target in [
        PythonTarget::Py312,
        PythonTarget::Py313,
        PythonTarget::Py314,
    ] {
        for is_stub in [false, true] {
            for &(name, source) in CASES {
                let full = parse_data(source, target, is_stub).unwrap();
                let comments = parse_data_for_type_inference(source, target, is_stub).unwrap();
                let expected: Vec<_> = full
                    .tokens
                    .iter()
                    .filter(|token| token.kind == "Comment")
                    .cloned()
                    .collect();
                assert_eq!(
                    comments.tokens, expected,
                    "{name} {target:?} stub={is_stub}"
                );
                assert!(comments.tokens.iter().all(|token| token.kind == "Comment"));
                assert_eq!(full.target, comments.target);
                assert_eq!(full.parsed.syntax(), comments.parsed.syntax(), "{name}");
                assert_eq!(full.parsed.tokens(), comments.parsed.tokens(), "{name}");
                assert_eq!(full.parsed.errors(), comments.parsed.errors(), "{name}");
                assert_eq!(
                    full.parsed.unsupported_syntax_errors(),
                    comments.parsed.unsupported_syntax_errors(),
                    "{name}"
                );
                assert_eq!(full.parsed, comments.parsed, "{name}");
                assert_eq!(full.issues, comments.issues, "{name}");
                let default = parse(source, target, is_stub).unwrap();
                assert_eq!(default.tokens, full.tokens, "default rich view: {name}");
                assert_eq!(default.parsed, full.parsed, "default native syntax: {name}");
                assert_eq!(default.issues, full.issues, "default issues: {name}");
                let rebuilt: String = default
                    .tokens
                    .iter()
                    .map(|token| default.text(token))
                    .collect();
                assert_eq!(rebuilt, source, "lossless default: {name}");
                let inferred = SyntaxDocument::from_data(source, Arc::new(comments));
                assert_eq!(default.is_valid(), inferred.is_valid());
                assert_eq!(
                    default.has_only_type_parameter_default_errors(),
                    inferred.has_only_type_parameter_default_errors()
                );
                assert_eq!(
                    default.has_only_type_inference_version_errors(),
                    inferred.has_only_type_inference_version_errors()
                );
                for token in &inferred.tokens {
                    assert_eq!(default.text(token), inferred.text(token));
                    assert_eq!(
                        default.line_column(token.start),
                        inferred.line_column(token.start)
                    );
                    assert_eq!(
                        default.line_column(token.end),
                        inferred.line_column(token.end)
                    );
                }
            }
        }
    }
}

#[test]
fn comment_ranges_keep_hashes_inside_strings_out_of_the_view() {
    let source = "value = '# not a comment'  # actual comment\r\n";
    for target in [
        PythonTarget::Py312,
        PythonTarget::Py313,
        PythonTarget::Py314,
    ] {
        let data = parse_data_for_type_inference(source, target, false).unwrap();
        let document = SyntaxDocument::from_data(source, Arc::new(data));
        assert_eq!(document.tokens.len(), 1);
        assert_eq!(document.text(&document.tokens[0]), "# actual comment");
        assert!(document.tokens[0].trivia);
    }
}
