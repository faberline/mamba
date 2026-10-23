//! Black 26.10.0 uses its frozen Unicode table and counts other scalars as one.
mod data;

pub fn character(character: char) -> u32 {
    let point = character as u32;
    let index = data::WIDTHS.partition_point(|range| range[1] < point);
    data::WIDTHS
        .get(index)
        .filter(|range| range[0] <= point)
        .map_or(1, |range| range[2])
}

pub fn measure(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        text.chars().map(|value| character(value) as usize).sum()
    }
}

/// Whole normalized Python literal tokens can contain their own line breaks.
/// These breaks do not require new breaks around the token.
pub(crate) fn multiline_literal(text: &str) -> bool {
    let Some(start) = text.find(['\'', '"']) else {
        return false;
    };
    let prefix = &text[..start];
    start <= 2
        && prefix.bytes().all(|value| {
            matches!(
                value,
                b'r' | b'R' | b'u' | b'U' | b'b' | b'B' | b'f' | b'F' | b't' | b'T'
            )
        })
        && text.ends_with(text.as_bytes()[start] as char)
        && text.contains('\n')
}
