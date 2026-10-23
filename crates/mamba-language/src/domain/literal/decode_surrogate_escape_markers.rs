use super::*;

pub fn decode_surrogate_escape_markers(s: &str) -> Option<Vec<u32>> {
    let chars: Vec<char> = s.chars().collect();
    let mut idx = 0;
    let mut codepoints = Vec::new();
    let mut saw_marker = false;
    while idx < chars.len() {
        let c = chars[idx];
        if c != SURROGATE_ESCAPE_MARKER[0] {
            codepoints.push(c as u32);
            idx += 1;
            continue;
        }

        let marker_end = idx + SURROGATE_ESCAPE_MARKER.len();
        if marker_end > chars.len()
            || chars[idx..marker_end] != SURROGATE_ESCAPE_MARKER
            || marker_end + 4 > chars.len()
        {
            codepoints.push(c as u32);
            idx += 1;
            continue;
        }

        let mut hex = String::with_capacity(4);
        for h in &chars[marker_end..marker_end + 4] {
            hex.push(*h);
        }
        let cp = u32::from_str_radix(&hex, 16).ok()?;
        if !(0xD800..=0xDFFF).contains(&cp) {
            return None;
        }
        saw_marker = true;
        codepoints.push(cp);
        idx = marker_end + 4;
    }
    saw_marker.then_some(codepoints)
}
