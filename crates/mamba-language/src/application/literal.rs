pub fn decode_surrogate_escape_markers(s: &str) -> Option<Vec<u32>> {
    crate::domain::literal::decode_surrogate_escape_markers(s)
}
