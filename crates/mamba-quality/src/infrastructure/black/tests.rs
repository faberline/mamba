use super::*;
use crate::{domain::ports::ProjectStore, infrastructure::filesystem::Filesystem};
use std::fs;
mod catalog;
mod errors;
mod historical_errors;
mod reference;
mod safety;
mod type_comments;
mod type_comments_data;
mod width_data;

#[test]
fn uses_black_character_widths_on_all_three_reference_runtimes() {
    let cases: serde_json::Value = serde_json::from_str(width_data::CASES).unwrap();
    for case in cases.as_array().unwrap() {
        let actual = ruff_formatter::black_width::measure(case["source"].as_str().unwrap());
        for expected in case["expected"].as_array().unwrap() {
            assert_eq!(actual as u64, expected.as_u64().unwrap(), "{case}");
        }
    }
}

#[test]
fn preserves_format_suppression_and_normalizes_multiline_quotes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.py");
    let text = "# fmt: off\nvalue=  1\n# fmt: on\nlabel='value'\ntext = '''\n unchanged\n  '''\n";
    fs::write(&path, text).unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    let output = format(&workspace.sources[0], &workspace).unwrap();
    assert_eq!(
        output,
        "# fmt: off\nvalue=  1\n# fmt: on\nlabel = \"value\"\ntext = \"\"\"\n unchanged\n  \"\"\"\n"
    );
}

#[test]
fn does_not_claim_full_black_parity_for_a_clean_simple_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.py");
    fs::write(&path, "value = 1\n").unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    let report = check(&workspace);
    assert_eq!(report.completion, CompletionStatus::Incomplete);
    assert_eq!(report.reference_exit_code, None);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn formats_an_eof_statement_and_restores_the_final_newline() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.py");
    fs::write(&path, "VALUE=1").unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    assert_eq!(
        format(&workspace.sources[0], &workspace).unwrap(),
        "VALUE = 1\n"
    );
}
mod interpolation_comments;
mod interpolation_continuation;
mod interpolation_prefixes;
mod literal_skip_controls;
mod mixed_byte_braces;
mod mixed_error_priority;
mod mixed_literal_order;
mod string_directives;
mod syntax_order;
mod template_grammar;
mod template_mixed_literals;

mod comment_controls;

mod byte_controls;

mod type_comment_controls;
