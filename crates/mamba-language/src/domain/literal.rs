const SURROGATE_ESCAPE_MARKER: [char; 3] = ['\u{E000}', '\u{E001}', '\u{E002}'];

#[cfg(test)]
mod tests;

mod apply_bytes_escapes;
mod apply_escape_sequences;
mod decode_surrogate_escape_markers;
mod push_surrogate_escape_marker;
mod unicode_name_to_char;

pub(crate) use apply_bytes_escapes::apply_bytes_escapes;
pub(crate) use apply_escape_sequences::apply_escape_sequences;
pub use decode_surrogate_escape_markers::decode_surrogate_escape_markers;
use push_surrogate_escape_marker::push_surrogate_escape_marker;
use unicode_name_to_char::unicode_name_to_char;
