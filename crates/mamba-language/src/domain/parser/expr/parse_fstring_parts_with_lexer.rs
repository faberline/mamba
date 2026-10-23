use super::*;

/// Parse f-string content into literal/expression parts.  `is_raw` carries the
/// `r` prefix (`rf'...'` / `fr'...'`): when set, backslash escapes in the
/// literal runs are kept verbatim.
pub(super) fn parse_fstring_parts_with_lexer(
    content: &str,
    is_raw: bool,
    raw_lexer: super::super::RawLexer,
) -> Result<Vec<FStringPart>, String> {
    let mut parts = Vec::new();
    // Accumulate the literal run as raw bytes so multi-byte UTF-8 characters
    // survive intact; decode to a `String` (lossily, though `content` is valid
    // UTF-8 so this never substitutes) only when flushing the run.
    let mut lit: Vec<u8> = Vec::new();
    let flush_lit = |lit: &mut Vec<u8>| String::from_utf8_lossy(&std::mem::take(lit)).into_owned();
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !is_raw && bytes[i] == b'\\' {
            // Non-raw f-string backslash escape. The braces of a `\N{NAME}`
            // named-unicode escape are part of the escape and must NOT be
            // treated as replacement-field delimiters (PEP-701 + CPython 3.12).
            // Copy the escape bytes verbatim into the literal run; the actual
            // escape resolution happens later in `apply_escape_sequences`.
            if i + 2 < bytes.len() && bytes[i + 1] == b'N' && bytes[i + 2] == b'{' {
                // Consume `\N{...}` through the closing `}` (or end of input).
                lit.push(b'\\');
                lit.push(b'N');
                lit.push(b'{');
                i += 3;
                while i < bytes.len() {
                    let b = bytes[i];
                    lit.push(b);
                    i += 1;
                    if b == b'}' {
                        break;
                    }
                }
            } else if i + 1 < bytes.len() && bytes[i + 1] != b'{' && bytes[i + 1] != b'}' {
                // Ordinary escape (`\n`, `\\`, `\xHH`, ...): consume the
                // backslash and the following byte as a pair so that an escaped
                // backslash (`\\`) does not leave a stray backslash in front of
                // a real replacement field. A backslash before `{`/`}` is not a
                // valid escape, so the brace is left for field handling below.
                lit.push(b'\\');
                lit.push(bytes[i + 1]);
                i += 2;
            } else {
                lit.push(b'\\');
                i += 1;
            }
        } else if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            lit.push(b'{');
            i += 2;
        } else if bytes[i] == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'}' {
            lit.push(b'}');
            i += 2;
        } else if bytes[i] == b'{' {
            if !lit.is_empty() {
                parts.push(FStringPart::Literal(finish_fstring_literal(
                    flush_lit(&mut lit),
                    is_raw,
                )));
            }
            i += 1;
            let start = i;
            let mut depth = 1i32;
            // Use a stack to track nested strings so that `{` and `}` inside
            // string literals don't confuse the brace counter (PEP-701).
            let mut str_stack: Vec<u8> = Vec::new();
            while i < bytes.len() && depth > 0 {
                let b = bytes[i];
                if str_stack.is_empty() {
                    // Outside any nested string
                    if b == b'{' {
                        depth += 1;
                        i += 1;
                    } else if b == b'}' {
                        depth -= 1;
                        if depth > 0 {
                            i += 1;
                        }
                    } else if b == b'\'' || b == b'"' {
                        str_stack.push(b);
                        i += 1;
                    } else if b == b'\\' {
                        // Skip escaped character
                        i += if i + 1 < bytes.len() { 2 } else { 1 };
                    } else {
                        i += 1;
                    }
                } else {
                    let quote = *str_stack.last().unwrap();
                    if b == b'\\' {
                        // Skip escaped character inside string
                        i += if i + 1 < bytes.len() { 2 } else { 1 };
                    } else if b == quote {
                        str_stack.pop();
                        i += 1;
                    } else {
                        // Any other byte — including the OTHER quote kind —
                        // is plain string content (`f"""{"eric's"}"""`).
                        i += 1;
                    }
                }
            }
            if depth > 0 {
                return Err("f-string: expecting '}'".to_string());
            }
            let raw = &content[start..i];
            i += 1; // skip closing }
                    // Split on first top-level `:` to separate expression from format spec.
                    // We must not split inside brackets/parens/strings (e.g., `d['key']`).
            let (expr_with_conv, format_spec) = split_expr_and_spec(raw);
            // Peel off `!r`, `!s`, or `!a` conversion suffix at top level.
            let (expr_str0, conversion) = split_expr_and_conversion(expr_with_conv);
            // `=` self-documenting form (PEP 498 / 3.8 debug specifier):
            // `{expr=}`, `{expr = }`, `{expr=!r}`, `{expr=:spec}`. The text up
            // to and including the `=` (plus surrounding whitespace) is echoed
            // verbatim, then the value follows — repr by default unless an
            // explicit conversion or spec is present.
            let trimmed = expr_str0.trim_end();
            let is_debug_eq = trimmed.ends_with('=')
                && !trimmed.ends_with("==")
                && !trimmed.ends_with("!=")
                && !trimmed.ends_with("<=")
                && !trimmed.ends_with(">=")
                && !trimmed.ends_with(":=");
            let (expr_str, debug_echo) = if is_debug_eq {
                (
                    trimmed[..trimmed.len() - 1].trim_end(),
                    Some(expr_str0.to_string()),
                )
            } else {
                (expr_str0, None)
            };
            if expr_str.trim().is_empty() {
                return Err("f-string: valid expression required before '}'".to_string());
            }
            // Detect nested f-strings: if the entire expression is `f"..."` or
            // `f'...'`, recursively invoke parse_fstring_parts on the inner
            // content for more direct handling (PEP-701 nested f-strings).
            let mut expr_node = if let Some(inner) = strip_fstring_literal(expr_str) {
                // The nested literal is `f"..."`/`f'...'` (strip_fstring_literal
                // only matches the bare `f` prefix), so it is never raw.
                let inner_parts = parse_fstring_parts_with_lexer(inner, false, raw_lexer)?;
                Expr::FString(inner_parts)
            } else {
                parse_fstring_expr(expr_str, raw_lexer)
            };
            if matches!(expr_node, Expr::Starred(_)) || expr_str.trim_start().starts_with('*') {
                return Err("f-string: cannot use starred expression here".to_string());
            }
            let span = Span::dummy();
            if let Some(echo) = debug_echo {
                parts.push(FStringPart::Literal(echo));
            }
            // Apply conversion by wrapping the expression in a call to the
            // corresponding builtin. `!s` is a no-op because the default
            // conversion already calls str(). `!r` and `!a` both go through
            // repr() — mamba doesn't distinguish ASCII yet. The `=` debug
            // form defaults to repr when no conversion or spec is given.
            let effective_conv = match conversion {
                Some(c) => Some(c),
                None if is_debug_eq && format_spec.is_none() => Some('r'),
                None => None,
            };
            if let Some(conv) = effective_conv {
                if conv == 'r' || conv == 'a' {
                    // `!r` -> repr(); `!a` -> ascii() (backslash-escapes non-ASCII).
                    let fname = if conv == 'a' { "ascii" } else { "repr" };
                    let inner = Spanned::new(expr_node, span);
                    expr_node = Expr::Call {
                        func: Box::new(Spanned::new(Expr::Ident(fname.to_string()), span)),
                        args: vec![CallArg::Positional(inner)],
                    };
                }
            }
            // Structure the spec: a static spec is one Literal part; a spec
            // containing replacement fields parses recursively so the nested
            // expressions evaluate at runtime ({value:{width}}).
            let spec_parts = match format_spec {
                None => None,
                Some(sp) if !sp.contains('{') => Some(vec![FStringPart::Literal(sp)]),
                Some(sp) => Some(parse_fstring_parts_with_lexer(&sp, is_raw, raw_lexer)?),
            };
            parts.push(FStringPart::Expr(Spanned::new(expr_node, span), spec_parts));
        } else {
            // Copy the raw byte into the literal run; multi-byte UTF-8 sequences
            // are preserved (decoded as a unit when the run is flushed) instead
            // of being mangled by a per-byte `as char` (Latin-1) cast.
            lit.push(bytes[i]);
            i += 1;
        }
    }
    if !lit.is_empty() {
        parts.push(FStringPart::Literal(finish_fstring_literal(
            flush_lit(&mut lit),
            is_raw,
        )));
    }
    Ok(parts)
}
