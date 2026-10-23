//! Prefix and literal-comment cases retain the original invalid output bytes.

#[test]
fn pinned_interpolation_prefix_comparisons() {
    super::string_directives::compare(
        include_str!("interpolation_prefixes.json"),
        3,
        "MAMBA_QUALITY_BLACK_INTERPOLATION_PREFIX_EVIDENCE",
        "mamba.quality.black-interpolation-prefixes-comparison.v1",
    );
}
