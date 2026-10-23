//! Frozen interpolation cases compare messages and the original invalid output bytes.

#[test]
fn pinned_interpolation_comment_comparisons() {
    super::string_directives::compare(
        include_str!("interpolation_comments.json"),
        26,
        "MAMBA_QUALITY_BLACK_INTERPOLATION_COMMENT_EVIDENCE",
        "mamba.quality.black-interpolation-comments-comparison.v1",
    );
}
