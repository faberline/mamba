use super::{numbers::*, strings::*};
use crate::domain::literal::apply_escape_sequences;
use logos::Logos;

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t]+")]
#[logos(skip r"\\\r?\n[ \t]*")]
pub(super) enum LogosKind {
    // --- Keywords: Control Flow ---
    #[token("def")]
    Def,
    #[token("return")]
    Return,
    #[token("if")]
    If,
    #[token("elif")]
    Elif,
    #[token("else")]
    Else,
    #[token("while")]
    While,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("class")]
    Class,
    #[token("enum")]
    Enum,
    #[token("match")]
    Match,
    #[token("case")]
    Case,
    #[token("import")]
    Import,
    #[token("from")]
    From,
    #[token("as")]
    As,
    #[token("and")]
    And,
    #[token("or")]
    Or,
    #[token("not")]
    Not,
    #[token("True")]
    True,
    #[token("False")]
    False,
    #[token("None")]
    None_,
    #[token("pass")]
    Pass,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("self")]
    Self_,
    // --- Keywords: Exception Handling (#206) ---
    #[token("try")]
    Try,
    #[token("except")]
    Except,
    #[token("finally")]
    Finally,
    #[token("raise")]
    Raise,
    #[token("with")]
    With,
    // --- Keywords: Async (#207) ---
    #[token("async")]
    Async,
    #[token("await")]
    Await,
    #[token("yield")]
    Yield,
    // --- Keywords: Other (#208, #212) ---
    #[token("lambda")]
    Lambda,
    #[token("del")]
    Del,
    #[token("assert")]
    Assert,
    #[token("global")]
    Global,
    #[token("nonlocal")]
    Nonlocal,
    #[token("is")]
    Is,
    #[token("type")]
    Type,
    // --- Type keywords ---
    #[token("int")]
    IntType,
    #[token("float")]
    FloatType,
    #[token("bool")]
    BoolType,
    #[token("str")]
    StrType,
    #[token("list")]
    ListType,
    #[token("dict")]
    DictType,
    #[token("tuple")]
    TupleType,
    // --- Literals ---
    // Complex literals — the exponent form `([eE][+-]?[0-9]+)?` mirrors
    // the sibling Float regexes so `3.5e-20j` / `.5e-2J` / `2E5j` all
    // tokenize as Complex (#1676). Without the exponent, the Float
    // regex consumed the numeric body and `j` fell through to Ident.
    // Digit runs allow PEP 515 underscores (`2_0j`, `1_000.5`, `1_0e3`); the
    // `_` separators are stripped before `parse::<f64>()`, which rejects them.
    #[regex(r"[0-9][0-9_]*\.[0-9_]*([eE][+-]?[0-9][0-9_]*)?[jJ]", |lex| parse_complex_literal(lex.slice()), priority = 4)]
    #[regex(r"\.[0-9][0-9_]*([eE][+-]?[0-9][0-9_]*)?[jJ]", |lex| parse_complex_literal(lex.slice()), priority = 4)]
    #[regex(r"[0-9][0-9_]*([eE][+-]?[0-9][0-9_]*)?[jJ]", |lex| parse_complex_literal(lex.slice()), priority = 4)]
    Complex(f64),
    #[regex(r"[0-9][0-9_]*\.[0-9_]*([eE][+-]?[0-9][0-9_]*)?", |lex| parse_decimal_float_literal(lex.slice()), priority = 4)]
    #[regex(r"\.[0-9][0-9_]*([eE][+-]?[0-9][0-9_]*)?", |lex| parse_decimal_float_literal(lex.slice()), priority = 4)]
    #[regex(r"[0-9][0-9_]*[eE][+-]?[0-9][0-9_]*", |lex| parse_decimal_float_literal(lex.slice()), priority = 4)]
    Float(f64),
    #[regex(r"0[xX]_?[0-9a-fA-F][0-9a-fA-F_]*", |lex| {
        parse_prefixed_int_literal(lex.slice(), 16, is_hex_digit_byte)
    }, priority = 3)]
    #[regex(r"0[oO]_?[0-7][0-7_]*", |lex| {
        parse_prefixed_int_literal(lex.slice(), 8, is_oct_digit_byte)
    }, priority = 3)]
    #[regex(r"0[bB]_?[01][01_]*", |lex| {
        parse_prefixed_int_literal(lex.slice(), 2, is_bin_digit_byte)
    }, priority = 3)]
    #[regex(r"[0-9][0-9_]*", |lex| parse_decimal_int_literal(lex.slice()), priority = 2)]
    Int(i64),
    #[regex(r"0[xX][A-Za-z0-9_]*", priority = 1)]
    #[regex(r"0[oO][A-Za-z0-9_]*", priority = 1)]
    #[regex(r"0[bB][A-Za-z0-9_]*", priority = 1)]
    #[regex(r"0_[A-Za-z][A-Za-z0-9_]*", priority = 3)]
    #[regex(
        r"[0-9][0-9_]*\.[0-9_]*([eE][+-]?[0-9][0-9_]*)?[jJ][A-Za-z0-9_]+",
        priority = 2
    )]
    #[regex(
        r"\.[0-9][0-9_]*([eE][+-]?[0-9][0-9_]*)?[jJ][A-Za-z0-9_]+",
        priority = 2
    )]
    #[regex(r"[0-9][0-9_]*([eE][+-]?[0-9][0-9_]*)?[jJ][A-Za-z0-9_]+", priority = 2)]
    #[regex(r"[0-9][0-9_]*\.[0-9_]*[eE][+-]?[A-Za-z0-9_]*", priority = 1)]
    #[regex(r"\.[0-9][0-9_]*[eE][+-]?[A-Za-z0-9_]*", priority = 1)]
    #[regex(r"[0-9][0-9_]*[eE][+-]?[A-Za-z0-9_]*", priority = 1)]
    InvalidNumber,
    // Regular strings
    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice(); Some(apply_escape_sequences(&s[1..s.len()-1]))
    })]
    #[regex(r#"'([^'\\]|\\.)*'"#, |lex| {
        let s = lex.slice(); Some(apply_escape_sequences(&s[1..s.len()-1]))
    })]
    // Python 2-compat `u`/`U` unicode-string prefix (#1920) — a no-op prefix
    // in CPython 3, so it shares `Str`'s payload and escape processing;
    // the callback only differs in skipping the 2-byte prefix+quote instead
    // of the 1-byte bare quote.
    #[regex(r#"u"([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice(); Some(apply_escape_sequences(&s[2..s.len()-1]))
    })]
    #[regex(r#"u'([^'\\]|\\.)*'"#, |lex| {
        let s = lex.slice(); Some(apply_escape_sequences(&s[2..s.len()-1]))
    })]
    #[regex(r#"U"([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice(); Some(apply_escape_sequences(&s[2..s.len()-1]))
    })]
    #[regex(r#"U'([^'\\]|\\.)*'"#, |lex| {
        let s = lex.slice(); Some(apply_escape_sequences(&s[2..s.len()-1]))
    })]
    Str(String),
    // Triple-quoted strings (#211)
    #[token("\"\"\"", lex_regular_triple_dquote, priority = 10)]
    #[token("'''", lex_regular_triple_squote, priority = 10)]
    TripleStr(String),
    // F-strings with PEP 701 support (#py312):
    //   callback-based lexers handle multiline expressions, quote reuse, and
    //   backslashes in expressions.  Priority 5 beats the Ident regex for `f`.
    //   Triple-quoted openers carry priority 11 so the 4-byte `f"""` beats
    //   both the 2-byte `f"` opener and the bare `"""` triple-string token.
    #[token("f\"\"\"", lex_fstr_triple_dquote, priority = 11)]
    #[token("f'''", lex_fstr_triple_squote, priority = 11)]
    #[token("f\"", lex_fstr_dquote, priority = 5)]
    #[token("f'", lex_fstr_squote, priority = 5)]
    FStr(String),
    // Raw f-strings (PEP 498): `rf"..."` / `fr"..."` in either prefix order and
    // either letter case.  The body is scanned exactly like a normal f-string
    // (the same callbacks) — the only difference from `FStr` is that backslash
    // escapes in the literal runs are kept verbatim by the parser.  Priority 6
    // beats both the `Ident` regex (`rf`/`fr` are not names here) and the bare
    // `r"`/`f"` openers, so the 3-char prefix wins the longest match.
    #[token("rf\"", lex_fstr_dquote, priority = 6)]
    #[token("rf'", lex_fstr_squote, priority = 6)]
    #[token("rF\"", lex_fstr_dquote, priority = 6)]
    #[token("rF'", lex_fstr_squote, priority = 6)]
    #[token("Rf\"", lex_fstr_dquote, priority = 6)]
    #[token("Rf'", lex_fstr_squote, priority = 6)]
    #[token("RF\"", lex_fstr_dquote, priority = 6)]
    #[token("RF'", lex_fstr_squote, priority = 6)]
    #[token("fr\"", lex_fstr_dquote, priority = 6)]
    #[token("fr'", lex_fstr_squote, priority = 6)]
    #[token("fR\"", lex_fstr_dquote, priority = 6)]
    #[token("fR'", lex_fstr_squote, priority = 6)]
    #[token("Fr\"", lex_fstr_dquote, priority = 6)]
    #[token("Fr'", lex_fstr_squote, priority = 6)]
    #[token("FR\"", lex_fstr_dquote, priority = 6)]
    #[token("FR'", lex_fstr_squote, priority = 6)]
    RawFStr(String),
    // Raw strings (#212) — no escape processing
    // Raw triple-quoted forms (#1678) registered with priority = 10 so the
    // 4-byte `r"""` / `r'''` opening token beats the 3-byte `r""` / `r''`
    // longest-match of the single-quoted raw regex on input that begins
    // with three quotes.
    #[token("r\"\"\"", lex_raw_triple_dquote, priority = 10)]
    #[token("r'''", lex_raw_triple_squote, priority = 10)]
    // Python raw-string rule: a backslash does NOT introduce an escape (it is
    // kept verbatim in the value), but a backslash-quote pair (`\"` / `\'`)
    // still does not terminate the literal.  The body regex therefore mirrors
    // the regular-string shape (`([^"\\]|\\.)*`) so `\"`/`\'` are consumed as
    // a single non-terminating unit — the callback then slices the content
    // WITHOUT running `apply_escape_sequences`, so the backslash survives.
    #[regex(r#"r"([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice(); Some(s[2..s.len()-1].to_string())
    })]
    #[regex(r#"r'([^'\\]|\\.)*'"#, |lex| {
        let s = lex.slice(); Some(s[2..s.len()-1].to_string())
    })]
    RawStr(String),
    // Byte strings (#212)
    // Bytes triple-quoted forms (#1682). Same shape as the raw-triple-quoted
    // fix in #1678: the 4-byte opener `b"""` / `b'''` beats the 3-byte
    // longest match of the single-quoted byte regex via priority = 10.
    // Re-uses `lex_triple_dquote` / `lex_triple_squote`; downstream bytes
    // processing handles escape sequences from the raw content.
    #[token("b\"\"\"", lex_triple_dquote, priority = 10)]
    #[token("b'''", lex_triple_squote, priority = 10)]
    #[regex(r#"b"([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice(); Some(s[2..s.len()-1].to_string())
    })]
    #[regex(r#"b'([^'\\]|\\.)*'"#, |lex| {
        let s = lex.slice(); Some(s[2..s.len()-1].to_string())
    })]
    // Raw-bytes literal prefixes `br'...'` / `rb'...'` (#1604).
    // Content-wise raw bytes is bytes-with-escapes-kept-literal; the existing
    // b'' regex already passes escapes through unchanged, so the slice just
    // skips the 3-char prefix instead of 2.
    #[regex(r#"br"([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice(); Some(s[3..s.len()-1].to_string())
    })]
    #[regex(r#"br'([^'\\]|\\.)*'"#, |lex| {
        let s = lex.slice(); Some(s[3..s.len()-1].to_string())
    })]
    #[regex(r#"rb"([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice(); Some(s[3..s.len()-1].to_string())
    })]
    #[regex(r#"rb'([^'\\]|\\.)*'"#, |lex| {
        let s = lex.slice(); Some(s[3..s.len()-1].to_string())
    })]
    ByteStr(String),
    // Ellipsis (#212)
    #[token("...")]
    Ellipsis,
    // --- Identifiers ---
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", priority = 1)]
    Ident,
    // --- Operators ---
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("//")]
    DoubleSlash,
    #[token("%")]
    Percent,
    #[token("**")]
    DoubleStar,
    #[token("=")]
    Eq,
    #[token("==")]
    EqEq,
    #[token("!=")]
    NotEq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("<=")]
    LtEq,
    #[token(">=")]
    GtEq,
    #[token("->")]
    Arrow,
    #[token("|")]
    Pipe,
    #[token("?")]
    Question,
    // Augmented assignment (#205)
    #[token("+=")]
    PlusEq,
    #[token("-=")]
    MinusEq,
    #[token("*=")]
    StarEq,
    #[token("/=")]
    SlashEq,
    #[token("//=")]
    DoubleSlashEq,
    #[token("%=")]
    PercentEq,
    #[token("**=")]
    DoubleStarEq,
    // Bitwise operators (#209)
    #[token("&")]
    Amp,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
    #[token("<<")]
    LShift,
    #[token(">>")]
    RShift,
    #[token("&=")]
    AmpEq,
    #[token("|=")]
    PipeEq,
    #[token("^=")]
    CaretEq,
    #[token("<<=")]
    LShiftEq,
    #[token(">>=")]
    RShiftEq,
    // Walrus and @ operator (#210)
    #[token(":=")]
    ColonEq,
    #[token("@")]
    At,
    #[token("@=")]
    AtEq,
    // --- Delimiters ---
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token(":")]
    Colon,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    #[token(";")]
    Semicolon,
    // --- Comments ---
    #[regex(r"#[^\n]*")]
    Comment,
    // --- Newline (significant in Python-like syntax) ---
    #[token("\n")]
    Newline,
}
