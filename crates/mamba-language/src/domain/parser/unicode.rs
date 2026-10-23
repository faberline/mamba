use crate::source::span::FileId;
use mamba_shared_kernel::token::{Token, TokenKind};

pub(super) fn prepare_parser_tokens(
    source: &str,
    file_id: FileId,
    raw_lexer: super::RawLexer,
) -> Vec<Token> {
    let raw_tokens = raw_lexer(source, file_id);
    let raw_tokens = repair_unicode_ident_tokens(raw_tokens, source);
    let mut processor = crate::domain::indent::IndentProcessor::new();
    processor.process(raw_tokens)
}

pub(super) fn tokens_need_unicode_ident_repair(tokens: &[Token], source: &str) -> bool {
    source.char_indices().any(|(offset, ch)| {
        is_unicode_ident_start(ch)
            && !tokens
                .iter()
                .any(|token| token.start as usize <= offset && offset < token.end as usize)
    })
}

fn repair_unicode_ident_tokens(mut tokens: Vec<Token>, source: &str) -> Vec<Token> {
    let mut repaired = Vec::with_capacity(tokens.len());
    let mut cursor = 0usize;

    for token in tokens.drain(..) {
        let token_start = token.start as usize;
        if token_start > cursor {
            push_unicode_ident_tokens_from_gap(&mut repaired, source, cursor, token_start);
        }
        cursor = cursor.max(token.end as usize);
        repaired.push(token);
    }

    if cursor < source.len() {
        push_unicode_ident_tokens_from_gap(&mut repaired, source, cursor, source.len());
    }

    merge_unicode_ident_runs(repaired, source)
}

fn push_unicode_ident_tokens_from_gap(
    out: &mut Vec<Token>,
    source: &str,
    gap_start: usize,
    gap_end: usize,
) {
    let gap = &source[gap_start..gap_end];
    let mut iter = gap.char_indices().peekable();

    while let Some((offset, ch)) = iter.next() {
        if !is_unicode_ident_start(ch) {
            continue;
        }

        let ident_start = gap_start + offset;
        let mut ident_end = ident_start + ch.len_utf8();

        while let Some((next_offset, next_ch)) = iter.peek().copied() {
            if !is_unicode_ident_continue(next_ch) {
                break;
            }
            ident_end = gap_start + next_offset + next_ch.len_utf8();
            iter.next();
        }

        out.push(Token::new(
            TokenKind::Ident,
            ident_start as u32,
            ident_end as u32,
        ));
    }
}

fn is_unicode_ident_start(ch: char) -> bool {
    !ch.is_ascii() && ch.is_alphabetic()
}

fn is_unicode_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

fn merge_unicode_ident_runs(tokens: Vec<Token>, source: &str) -> Vec<Token> {
    let mut merged = Vec::with_capacity(tokens.len());
    let mut i = 0usize;

    while i < tokens.len() {
        let start = tokens[i].start;
        let mut end = tokens[i].end;
        let mut j = i + 1;
        let mut best: Option<(usize, u32)> = None;

        while j < tokens.len() && tokens[j].start == end {
            let candidate_end = tokens[j].end;
            let candidate = &source[start as usize..candidate_end as usize];
            if !is_identifier_text(candidate) {
                break;
            }
            if !candidate.is_ascii() {
                best = Some((j + 1, candidate_end));
            }
            end = candidate_end;
            j += 1;
        }

        if let Some((next_i, ident_end)) = best {
            merged.push(Token::new(TokenKind::Ident, start, ident_end));
            i = next_i;
        } else {
            merged.push(tokens[i].clone());
            i += 1;
        }
    }

    merged
}

fn is_identifier_text(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_alphabetic()) && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}
