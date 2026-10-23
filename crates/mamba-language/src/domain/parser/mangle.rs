//! Private name mangling (CPython `_Py_Mangle`, PEP-classic).
//!
//! Inside a class body, an identifier of the form `__spam` (at least two
//! leading underscores, not ending in two underscores) is textually rewritten
//! to `_<classname>__spam`, where the class name has its own leading
//! underscores stripped. Mangling applies to every such identifier within the
//! *textually enclosing* class — including nested functions/lambdas — but a
//! nested class re-roots mangling under its own name.
//!
//! Run as a post-parse AST pass so the type checker and the HIR lowering both
//! observe the mangled names (attributes, locals, parameters, captures).

use super::ast::*;
use crate::source::span::Spanned;

mod mangle_comprehensions;
mod mangle_expr;
mod mangle_fstring;
mod mangle_in_place;
mod mangle_module;
mod mangle_param;
mod mangle_pattern;
mod mangle_stmt;
mod mangle_stmts;
mod mangle_type_param;
mod mangled;

use mangle_comprehensions::mangle_comprehensions;
use mangle_expr::mangle_expr;
use mangle_fstring::mangle_fstring;
use mangle_in_place::mangle_in_place;
pub use mangle_module::mangle_module;
use mangle_param::mangle_param;
use mangle_pattern::mangle_pattern;
use mangle_stmt::mangle_stmt;
use mangle_stmts::mangle_stmts;
use mangle_type_param::mangle_type_param;
use mangled::mangled;
