//! Original Black error order and supported defaults remain exact across targets.

#[test]
fn pinned_syntax_order_and_type_defaults() {
    super::string_directives::compare(
        include_str!("syntax_order.json"),
        32,
        "MAMBA_QUALITY_BLACK_SYNTAX_ORDER_EVIDENCE",
        "mamba.quality.black-syntax-order-comparison.v1",
    );
}
