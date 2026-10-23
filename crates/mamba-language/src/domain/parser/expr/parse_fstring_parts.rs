use super::*;

#[cfg(test)]
pub(super) fn parse_fstring_parts(content: &str, is_raw: bool) -> Result<Vec<FStringPart>, String> {
    parse_fstring_parts_with_lexer(content, is_raw, crate::app::RAW_LEXER)
}
