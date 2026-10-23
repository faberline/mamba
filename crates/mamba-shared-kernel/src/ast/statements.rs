use super::*;

pub type Name = String;

/// PEP 695 type-parameter kind: `T`, `*Ts`, `**P`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeParamKind {
    TypeVar,
    TypeVarTuple,
    ParamSpec,
}

/// PEP 695 type parameter: `T`, `T: bound`, `T: (c1, c2)`, `*Ts`, `**P`.
///
/// The bound / constraint / default expressions are kept as ordinary
/// expressions so the runtime can evaluate them lazily (CPython semantics).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: Name,
    pub kind: TypeParamKind,
    /// `T: bound` — single (non-tuple) bound expression.
    pub bound: Option<Spanned<Expr>>,
    /// `T: (c1, c2, ...)` — tuple of constraint expressions.
    pub constraints: Option<Vec<Spanned<Expr>>>,
    /// `T = default` — lazily evaluated default expression (PEP 696).
    pub default: Option<Spanned<Expr>>,
}

impl TypeParam {
    /// Plain unbounded type parameter (used by tests / desugared nodes).
    pub fn plain(name: impl Into<Name>) -> Self {
        TypeParam {
            name: name.into(),
            kind: TypeParamKind::TypeVar,
            bound: None,
            constraints: None,
            default: None,
        }
    }
}

/// Top-level module: a sequence of statements.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub stmts: Vec<Spanned<Stmt>>,
}

/// Statement node.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `x: int = 42`
    VarDecl {
        name: Name,
        ty: Spanned<TypeExpr>,
        value: Spanned<Expr>,
    },
    /// `x = expr` (reassignment, no type annotation)
    Assign {
        target: Spanned<Expr>,
        value: Spanned<Expr>,
    },
    /// `x += expr` (augmented assignment, #221)
    AugAssign {
        target: Spanned<Expr>,
        op: AugOp,
        value: Spanned<Expr>,
    },
    /// `def foo(x: int) -> int: ...`
    FnDef {
        decorators: Vec<Spanned<Expr>>,
        name: Name,
        type_params: Vec<TypeParam>,
        params: Vec<Param>,
        return_ty: Option<Spanned<TypeExpr>>,
        body: Vec<Spanned<Stmt>>,
    },
    /// `async def foo(x: int) -> int: ...` (#227)
    AsyncFnDef {
        decorators: Vec<Spanned<Expr>>,
        name: Name,
        type_params: Vec<TypeParam>,
        params: Vec<Param>,
        return_ty: Option<Spanned<TypeExpr>>,
        body: Vec<Spanned<Stmt>>,
    },
    /// `class Foo(Base1, Base2, metaclass=Meta, name="alpha"): ...`
    ClassDef {
        decorators: Vec<Spanned<Expr>>,
        name: Name,
        type_params: Vec<TypeParam>,
        bases: Vec<Spanned<Expr>>,
        /// Keyword arguments in the class declaration (e.g. `metaclass=Meta`, `name="alpha"`).
        keyword_args: Vec<(Name, Spanned<Expr>)>,
        body: Vec<Spanned<Stmt>>,
    },
    /// `enum Shape: Circle(r: float) | Rectangle(w: float, h: float) | Point`
    EnumDef {
        name: Name,
        type_params: Vec<TypeParam>,
        variants: Vec<Variant>,
    },
    /// `if cond: ... elif cond: ... else: ...`
    If {
        condition: Spanned<Expr>,
        body: Vec<Spanned<Stmt>>,
        elif_clauses: Vec<(Spanned<Expr>, Vec<Spanned<Stmt>>)>,
        else_body: Option<Vec<Spanned<Stmt>>>,
    },
    /// `while cond: ... else: ...`
    While {
        condition: Spanned<Expr>,
        body: Vec<Spanned<Stmt>>,
        else_body: Option<Vec<Spanned<Stmt>>>,
    },
    /// `for x in iterable: ... else: ...`
    For {
        targets: Vec<Name>,
        /// PEP 3132: index into `targets` of a starred name (`for a, *b in
        /// seq:`), if any (#1895). `None` for all pre-existing target lists.
        star_index: Option<usize>,
        var_ty: Option<Spanned<TypeExpr>>,
        iter: Spanned<Expr>,
        body: Vec<Spanned<Stmt>>,
        else_body: Option<Vec<Spanned<Stmt>>>,
    },
    /// `async for x in iterable: ... else: ...` (#227)
    AsyncFor {
        targets: Vec<Name>,
        star_index: Option<usize>,
        var_ty: Option<Spanned<TypeExpr>>,
        iter: Spanned<Expr>,
        body: Vec<Spanned<Stmt>>,
        else_body: Option<Vec<Spanned<Stmt>>>,
    },
    /// `match expr: case Pattern: ...`
    Match {
        expr: Spanned<Expr>,
        arms: Vec<MatchArm>,
    },
    /// `return expr`
    Return(Option<Spanned<Expr>>),
    /// `pass`
    Pass,
    /// `break`
    Break,
    /// `continue`
    Continue,
    /// `import module` or `from module import name` or `import module as alias`
    Import {
        module: Vec<Name>,
        /// For `from X import Y as Z`: Some([("Y", Some("Z"))]).
        /// For `import X`: None.
        names: Option<Vec<(Name, Option<Name>)>>,
        /// For `import X as alias`: Some("alias").  None for plain `import X`.
        module_alias: Option<Name>,
    },
    /// Bare type annotation without a value: `x: int` (#1014).
    ///
    /// Used in class bodies and module scope (e.g. ORM field declarations).
    BareAnnotation { name: Name, ty: Spanned<TypeExpr> },
    /// `try: ... except E as e: ... finally: ...` (#225)
    Try {
        body: Vec<Spanned<Stmt>>,
        handlers: Vec<ExceptHandler>,
        else_body: Option<Vec<Spanned<Stmt>>>,
        finally_body: Option<Vec<Spanned<Stmt>>>,
    },
    /// `raise expr` or `raise expr from expr` (#225)
    Raise {
        value: Option<Spanned<Expr>>,
        from: Option<Spanned<Expr>>,
    },
    /// `with expr as name: ...` (#226)
    With {
        items: Vec<WithItem>,
        body: Vec<Spanned<Stmt>>,
    },
    /// `async with expr as name: ...` (#227)
    AsyncWith {
        items: Vec<WithItem>,
        body: Vec<Spanned<Stmt>>,
    },
    /// `assert expr` or `assert expr, msg` (#229)
    Assert {
        test: Spanned<Expr>,
        msg: Option<Spanned<Expr>>,
    },
    /// `del target` (#230)
    Del(Spanned<Expr>),
    /// `global x, y` (#231)
    Global(Vec<Name>),
    /// `nonlocal x, y` (#231)
    Nonlocal(Vec<Name>),
    /// `type X = int | str` (PEP 695, #233)
    ///
    /// The value is kept as a general expression (CPython allows any
    /// expression, e.g. `type Lazy[T] = lambda: T`) and is evaluated lazily
    /// at runtime by the TypeAliasType object built during desugaring.
    TypeAlias {
        name: Name,
        type_params: Vec<TypeParam>,
        value: Spanned<Expr>,
    },
    /// Expression used as statement
    ExprStmt(Spanned<Expr>),
}

/// Augmented assignment operator (#221).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AugOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    LShift,
    RShift,
    MatMul,
}

/// Exception handler in try/except (#225).
/// `is_star` is true for `except*` (PEP 654) handlers.
#[derive(Debug, Clone, PartialEq)]
pub struct ExceptHandler {
    pub exc_type: Option<Spanned<Expr>>,
    pub name: Option<Name>,
    pub body: Vec<Spanned<Stmt>>,
    pub is_star: bool,
    pub span: Span,
}

/// `with expr as name` item (#226).
#[derive(Debug, Clone, PartialEq)]
pub struct WithItem {
    pub context: Spanned<Expr>,
    pub alias: Option<Name>,
}

/// Parameter annotation presence in source syntax.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamAnnotation {
    Omitted,
    Authored(Spanned<TypeExpr>),
}

pub type SourceAnnotation = ParamAnnotation;

/// Function parameter with type annotation presence.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: Name,
    pub ty: Spanned<TypeExpr>,
    pub annotation: ParamAnnotation,
    pub default: Option<Spanned<Expr>>,
    pub kind: ParamKind,
    /// Declared before a `/` separator (PEP 570 positional-only).
    /// Introspection metadata only — call binding is unaffected.
    pub pos_only: bool,
    /// Declared after a bare `*` separator or `*args` (keyword-only).
    /// Introspection metadata only — call binding is unaffected.
    pub kw_only: bool,
    pub span: Span,
}

impl Param {
    pub fn is_authored(&self) -> bool {
        matches!(self.annotation, ParamAnnotation::Authored(_))
    }

    pub fn is_omitted(&self) -> bool {
        matches!(self.annotation, ParamAnnotation::Omitted)
    }

    pub fn authored_type(&self) -> Option<&Spanned<TypeExpr>> {
        match &self.annotation {
            ParamAnnotation::Authored(ref ty) => Some(ty),
            ParamAnnotation::Omitted => None,
        }
    }
}

/// Parameter kind (#218).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    Regular,
    Star,       // *args
    DoubleStar, // **kwargs
}

/// Enum variant definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: Name,
    pub fields: Vec<Param>,
    pub span: Span,
}

/// Match arm: `case Pattern: body`
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Spanned<Pattern>,
    pub guard: Option<Spanned<Expr>>,
    pub body: Vec<Spanned<Stmt>>,
    pub span: Span,
}
