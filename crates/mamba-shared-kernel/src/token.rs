//! Token data shared by syntax and execution services.
mod display;

/// A valid integer literal outside the signed 64-bit range.
pub const BIG_INT_LITERAL_SENTINEL: i64 = i64::MIN;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Def,
    Return,
    If,
    Elif,
    Else,
    While,
    For,
    In,
    Class,
    Enum,
    Match,
    Case,
    Import,
    From,
    As,
    And,
    Or,
    Not,
    True,
    False,
    None_,
    Pass,
    Break,
    Continue,
    Self_,
    Try,
    Except,
    Finally,
    Raise,
    With,
    Async,
    Await,
    Yield,
    Lambda,
    Del,
    Assert,
    Global,
    Nonlocal,
    Is,
    Type,
    IntType,
    FloatType,
    BoolType,
    StrType,
    ListType,
    DictType,
    TupleType,
    Complex(f64),
    Float(f64),
    Int(i64),
    InvalidNumber,
    Str(String),
    TripleStr(String),
    FStr(String),
    RawFStr(String),
    RawStr(String),
    ByteStr(String),
    Ellipsis,
    Ident,
    Plus,
    Minus,
    Star,
    Slash,
    DoubleSlash,
    Percent,
    DoubleStar,
    Eq,
    EqEq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Arrow,
    Pipe,
    Question,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    DoubleSlashEq,
    PercentEq,
    DoubleStarEq,
    Amp,
    Caret,
    Tilde,
    LShift,
    RShift,
    AmpEq,
    PipeEq,
    CaretEq,
    LShiftEq,
    RShiftEq,
    ColonEq,
    At,
    AtEq,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Colon,
    Comma,
    Dot,
    Semicolon,
    Comment,
    Newline,
    Indent,
    Dedent,
    Eof,
}

impl TokenKind {
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            Self::Def
                | Self::Return
                | Self::If
                | Self::Elif
                | Self::Else
                | Self::While
                | Self::For
                | Self::In
                | Self::Class
                | Self::Enum
                | Self::Match
                | Self::Case
                | Self::Import
                | Self::From
                | Self::As
                | Self::And
                | Self::Or
                | Self::Not
                | Self::True
                | Self::False
                | Self::None_
                | Self::Pass
                | Self::Break
                | Self::Continue
                | Self::Self_
                | Self::Try
                | Self::Except
                | Self::Finally
                | Self::Raise
                | Self::With
                | Self::Async
                | Self::Await
                | Self::Yield
                | Self::Lambda
                | Self::Del
                | Self::Assert
                | Self::Global
                | Self::Nonlocal
                | Self::Is
                | Self::Type
        )
    }
}

/// A token with its kind and byte offset range.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub start: u32,
    pub end: u32,
}

impl Token {
    pub fn new(kind: TokenKind, start: u32, end: u32) -> Self {
        Self { kind, start, end }
    }

    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start as usize..self.end as usize]
    }
}
