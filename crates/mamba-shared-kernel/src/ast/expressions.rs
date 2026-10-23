use super::*;

/// Match pattern.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// `_`
    Wildcard,
    /// Binding: `x`
    Binding(Name),
    /// Enum constructor: `Shape.Circle(r)`
    Constructor { path: Vec<Name>, fields: Vec<Name> },
    /// Literal pattern
    Literal(Expr),
    /// OR pattern: `p1 | p2` (#237)
    Or(Vec<Spanned<Pattern>>),
    /// Sequence pattern: `[a, b, *rest]` (#235)
    Sequence(Vec<Spanned<Pattern>>),
    /// Mapping pattern: `{"key": value, **rest}` (#236, #827)
    /// `rest` captures unmatched key-value pairs when `**rest` is present.
    Mapping {
        pairs: Vec<(Spanned<Expr>, Spanned<Pattern>)>,
        rest: Option<Name>,
    },
    /// Class pattern: `ClassName(x=1, y=2)` (#238)
    ClassPattern {
        cls: Vec<Name>,
        patterns: Vec<(Option<Name>, Spanned<Pattern>)>,
    },
    /// Star pattern in sequence: `*rest`
    Star(Option<Name>),
    /// AS pattern: `<pattern> as <name>` (#827)
    /// Matches the inner pattern and binds the matched value to `name`.
    As {
        pattern: Box<Spanned<Pattern>>,
        name: Name,
    },
}

/// Expression node.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Integer literal
    IntLit(i64),
    /// Integer literal larger than the compiler's i64 literal path.
    BigIntLit(String),
    /// Float literal
    FloatLit(f64),
    /// Complex literal (imaginary part, e.g. 2j)
    ComplexLit(f64),
    /// String literal
    StrLit(String),
    /// Bytes literal: `b"hello"`
    BytesLit(Vec<u8>),
    /// Boolean literal
    BoolLit(bool),
    /// None literal
    NoneLit,
    /// Ellipsis literal `...` (#212)
    Ellipsis,
    /// Identifier
    Ident(Name),
    /// Binary operation: `a + b`
    BinOp {
        op: BinOp,
        lhs: Box<Spanned<Expr>>,
        rhs: Box<Spanned<Expr>>,
    },
    /// Unary operation: `-x`, `not x`, `~x`
    UnaryOp {
        op: UnaryOp,
        operand: Box<Spanned<Expr>>,
    },
    /// Function call: `f(args)` with keyword args support
    Call {
        func: Box<Spanned<Expr>>,
        args: Vec<CallArg>,
    },
    /// Attribute access: `obj.attr`
    Attr {
        object: Box<Spanned<Expr>>,
        attr: Name,
    },
    /// Index access: `obj[idx]`
    Index {
        object: Box<Spanned<Expr>>,
        index: Box<Spanned<Expr>>,
    },
    /// Slice: `obj[start:stop:step]` (#219)
    Slice {
        start: Option<Box<Spanned<Expr>>>,
        stop: Option<Box<Spanned<Expr>>>,
        step: Option<Box<Spanned<Expr>>>,
    },
    /// List literal: `[1, 2, 3]`
    ListLit(Vec<Spanned<Expr>>),
    /// Dict literal: `{k: v, ...}` or dict-unpack `{**d, ...}` (PEP 448 / #1014).
    ///
    /// Each entry is `(Option<key>, value)`.  When the key is `None` the value
    /// expression is unpacked via `**` (i.e. `{**defaults, **overrides}`).
    DictLit(Vec<(Option<Spanned<Expr>>, Spanned<Expr>)>),
    /// Set literal: `{1, 2, 3}` (#224)
    SetLit(Vec<Spanned<Expr>>),
    /// Tuple literal: `(1, 2)`
    TupleLit(Vec<Spanned<Expr>>),
    /// Ternary: `a if cond else b` (#213)
    IfExpr {
        body: Box<Spanned<Expr>>,
        condition: Box<Spanned<Expr>>,
        else_body: Box<Spanned<Expr>>,
    },
    /// Lambda: `lambda x: int, y: int -> int: x + y` (#214)
    Lambda {
        params: Vec<Param>,
        body: Box<Spanned<Expr>>,
    },
    /// List comprehension: `[expr for x in iter if cond]` (#215)
    ListComp {
        element: Box<Spanned<Expr>>,
        generators: Vec<Comprehension>,
    },
    /// Set comprehension: `{expr for x in iter}` (#215)
    SetComp {
        element: Box<Spanned<Expr>>,
        generators: Vec<Comprehension>,
    },
    /// Dict comprehension: `{k: v for k, v in items}` (#215)
    DictComp {
        key: Box<Spanned<Expr>>,
        value: Box<Spanned<Expr>>,
        generators: Vec<Comprehension>,
    },
    /// Generator expression: `(expr for x in iter)` (#216)
    GeneratorExpr {
        element: Box<Spanned<Expr>>,
        generators: Vec<Comprehension>,
    },
    /// F-string: `f"hello {name}"` (#217)
    FString(Vec<FStringPart>),
    /// Yield: `yield expr` (#222)
    Yield(Option<Box<Spanned<Expr>>>),
    /// Yield from: `yield from expr` (#222)
    YieldFrom(Box<Spanned<Expr>>),
    /// Await: `await expr` (#223)
    Await(Box<Spanned<Expr>>),
    /// Walrus: `x := expr` (#210)
    Walrus {
        target: Name,
        value: Box<Spanned<Expr>>,
    },
    /// Chained comparison: `a < b < c` → `(a < b) and (b < c)` with `b` evaluated once.
    ChainedCompare {
        operands: Vec<Spanned<Expr>>,
        ops: Vec<BinOp>,
    },
    /// Star expression: `*expr` (in function calls, assignments)
    Starred(Box<Spanned<Expr>>),
    /// Tuple unpacking target (for assignments, #232)
    UnpackTarget(Vec<Spanned<Expr>>),
}

/// Call argument — positional or keyword.
#[derive(Debug, Clone, PartialEq)]
pub enum CallArg {
    Positional(Spanned<Expr>),
    Keyword { name: Name, value: Spanned<Expr> },
    StarArg(Spanned<Expr>),
    DoubleStarArg(Spanned<Expr>),
}

/// Comprehension clause: `for x in iter if cond` (#215).
///
/// `targets` holds the loop variable name(s).  For a simple `for x in …`
/// clause there is exactly one element.  For a tuple target (`for k, v in …`)
/// each name is stored separately.
#[derive(Debug, Clone, PartialEq)]
pub struct Comprehension {
    pub targets: Vec<Name>,
    pub unpack_target: bool,
    pub target_reads_before_bind: Vec<Name>,
    pub iter: Spanned<Expr>,
    pub conditions: Vec<Spanned<Expr>>,
    pub is_async: bool,
}

/// F-string part (#217).
#[derive(Debug, Clone, PartialEq)]
pub enum FStringPart {
    Literal(String),
    /// Expression with optional format spec. The spec is itself a part list
    /// so nested replacement fields (`{value:{width}}`) evaluate at runtime;
    /// a static spec is a single `Literal` part (e.g. `{x:.2f}` →
    /// `Some(vec![Literal(".2f")])`).
    Expr(Spanned<Expr>, Option<Vec<FStringPart>>),
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    MatMul,
    // Comparison
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    // Logical
    And,
    Or,
    // Bitwise (#209)
    BitAnd,
    BitOr,
    BitXor,
    LShift,
    RShift,
    // Identity / Membership (#212)
    Is,
    IsNot,
    In,
    NotIn,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Pos, // + (unary positive)
    Neg,
    Not,
    BitNot, // ~ (#209)
}
