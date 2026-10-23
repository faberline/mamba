use super::*;

pub(super) fn push_surrogate_escape_marker(out: &mut String, codepoint: u32) {
    for marker in SURROGATE_ESCAPE_MARKER {
        out.push(marker);
    }
    out.push_str(&format!("{codepoint:04x}"));
}
