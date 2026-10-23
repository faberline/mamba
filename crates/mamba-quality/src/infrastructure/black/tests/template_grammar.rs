//! Frozen Black grammar and source AST checks for template string concatenation.
#[test]
fn pinned_template_grammar_comparisons() {
    super::string_directives::compare(
        include_str!("template_grammar.json"),
        9,
        "MAMBA_QUALITY_BLACK_TEMPLATE_GRAMMAR_EVIDENCE",
        "mamba.quality.black-template-grammar-comparison.v1",
    );
}
