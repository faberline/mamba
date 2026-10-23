//! Frozen original controls retain raw template prefixes and adjacent byte literals.
#[test]
fn pinned_template_mixed_literal_comparisons() {
    super::string_directives::compare(
        include_str!("template_mixed_literals.json"),
        6,
        "MAMBA_QUALITY_BLACK_TEMPLATE_MIXED_LITERAL_EVIDENCE",
        "mamba.quality.black-template-mixed-literal-comparison.v1",
    );
}
