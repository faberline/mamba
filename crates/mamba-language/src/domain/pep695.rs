//! PEP 695 runtime desugaring (#233).
//!
//! Rewrites `def f[T](...)`, `class C[T]: ...`, and `type X = V` so the type
//! parameters and aliases exist as *runtime* objects with CPython semantics:
//!
//! * every `[T]`-syntax parameter becomes a runtime TypeVar instance bound to
//!   a same-named variable in the enclosing (non-class) scope, created by the
//!   `__mb_pep695_typevar__` intrinsic around the def/class statement
//!   (classes before the statement; functions immediately after it so defaults
//!   and decorators still raise CPython's out-of-scope `NameError`);
//! * the function/class gains a `__type_params__` tuple (classes also gain a
//!   matching `__parameters__`) assigned right after the definition;
//! * `type X = V` additionally binds `X` to a TypeAliasType instance built by
//!   the `__mb_pep695_type_alias__` intrinsic whose `__value__` is evaluated
//!   lazily via a zero-arg lambda (enabling recursive aliases). The original
//!   `TypeAlias` statement is kept for compile-time annotation resolution
//!   (it lowers to nothing).
//!
//! Bounds and constraints are wrapped in zero-arg lambdas so they are
//! evaluated lazily on first `__bound__` / `__constraints__` access, matching
//! CPython's deferred-evaluation semantics.
//!
//! Scope honesty: TypeVar bindings for parameters declared on methods or on
//! class-body aliases are hoisted to the nearest enclosing non-class scope
//! (class-body bindings are invisible to method bodies). Generic classes also
//! delete their temporary module-scope type-param bindings after attaching
//! `__type_params__`, and class-body lazy aliases read class params back from
//! that tuple to preserve CPython's out-of-scope `NameError` behavior.

use crate::source::span::{Span, Spanned};
use mamba_shared_kernel::ast::{
    CallArg, ExceptHandler, Expr, Module, Stmt, TypeParam, TypeParamKind,
};
use std::collections::{HashMap, HashSet};

/// Name of the TypeVar-construction intrinsic (see `runtime::pep695`).
pub const TYPEVAR_INTRINSIC: &str = "__mb_pep695_typevar__";
/// Name of the TypeAliasType-construction intrinsic (see `runtime::pep695`).
pub const TYPE_ALIAS_INTRINSIC: &str = "__mb_pep695_type_alias__";

#[derive(Clone)]
struct SavedBinding {
    name: Name,
    saved_name: Name,
    had_name: Name,
}

use mamba_shared_kernel::ast::Name;

/// Statements hoisted out of a class body.
///
/// * `before` — TypeVar bindings that must exist before the class statement
///   executes (class-level params, class-body alias params).
/// * `after` — per-definition TypeVar bindings plus the dotted
///   `Cls.meth.__type_params__ = (...)` assignments that must run after the
///   class exists. Method param bindings are deliberately placed *after* the
///   enclosing class's own `__type_params__` capture so a same-named method
///   param (`class Outer[T]: def meth[T]`) rebinds the shared name without
///   contaminating the class's already-captured tuple (method bodies resolve
///   the name dynamically at call time, so the later binding is the one they
///   see — matching the inner scope shadowing CPython gives them).
struct ClassHoists {
    before: Vec<Spanned<Stmt>>,
    after: Vec<AfterItem>,
}

/// One definition's deferred type-param wiring (see [`ClassHoists::after`]).
struct AfterItem {
    /// `T = __mb_pep695_typevar__(...)` bindings for this definition.
    tv_assigns: Vec<Spanned<Stmt>>,
    /// Attribute path *within* the enclosing class (e.g. `["meth"]`);
    /// each enclosing class prepends its own name while bubbling up.
    path: Vec<Name>,
    /// `(attr, params)` assignments, e.g. `("__type_params__", [T])`.
    attrs: Vec<(String, Vec<Name>)>,
    /// Extra deferred attribute writes whose values are not simple type-param
    /// tuples.
    extra_attrs: Vec<(String, Spanned<Expr>)>,
    span: Span,
}

#[cfg(test)]
mod tests;

mod class_desugar;

mod alias_ident;
mod assign_ident;
mod attr_assign;
mod attr_tuple_assign;
mod class_attr_expr;
mod class_type_param_expr;
mod collect_class_local_names;
mod collect_runtime_type_param_uses_block;
mod collect_runtime_type_param_uses_comprehension;
mod collect_runtime_type_param_uses_expr;
mod collect_runtime_type_param_uses_in_params;
mod collect_runtime_type_param_uses_in_type_params;
mod collect_runtime_type_param_uses_stmt;
mod collect_target_names;
mod del_ident;
mod desugar_block;
mod desugar_module;
mod emit_after;
mod params_tuple;
mod private_binding_name;
mod restore_or_delete_binding;
mod rewrite_class_alias_value;
mod rewrite_class_scoped_type_params;
mod rewrite_comprehension;
mod rewrite_current_type_param_bindings_comprehension;
mod rewrite_current_type_param_bindings_expr;
mod rewrite_current_type_param_bindings_in_params;
mod rewrite_current_type_param_bindings_in_type_params;
mod save_binding;
mod saved_binding;
mod sp;
mod type_param_binding_names;
mod typevar_assign;

use alias_ident::alias_ident;
use assign_ident::assign_ident;
use attr_assign::attr_assign;
use attr_tuple_assign::attr_tuple_assign;
use class_attr_expr::class_attr_expr;
use class_type_param_expr::class_type_param_expr;
use collect_class_local_names::collect_class_local_names;
use collect_runtime_type_param_uses_block::collect_runtime_type_param_uses_block;
use collect_runtime_type_param_uses_comprehension::collect_runtime_type_param_uses_comprehension;
use collect_runtime_type_param_uses_expr::collect_runtime_type_param_uses_expr;
use collect_runtime_type_param_uses_in_params::collect_runtime_type_param_uses_in_params;
use collect_runtime_type_param_uses_in_type_params::collect_runtime_type_param_uses_in_type_params;
use collect_runtime_type_param_uses_stmt::collect_runtime_type_param_uses_stmt;
use collect_target_names::collect_target_names;
use del_ident::del_ident;
use desugar_block::desugar_block;
pub use desugar_module::desugar_module;
use emit_after::emit_after;
use params_tuple::params_tuple;
use private_binding_name::private_binding_name;
use restore_or_delete_binding::restore_or_delete_binding;
use rewrite_class_alias_value::rewrite_class_alias_value;
use rewrite_class_scoped_type_params::rewrite_class_scoped_type_params;
use rewrite_comprehension::rewrite_comprehension;
use rewrite_current_type_param_bindings_comprehension::rewrite_current_type_param_bindings_comprehension;
use rewrite_current_type_param_bindings_expr::rewrite_current_type_param_bindings_expr;
use rewrite_current_type_param_bindings_in_params::rewrite_current_type_param_bindings_in_params;
use rewrite_current_type_param_bindings_in_type_params::rewrite_current_type_param_bindings_in_type_params;
use save_binding::save_binding;
use saved_binding::saved_binding;
use sp::sp;
use type_param_binding_names::type_param_binding_names;
use typevar_assign::typevar_assign;
