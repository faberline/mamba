use super::*;

/// Rewrite every mangled private name in the module in place.
pub fn mangle_module(module: &mut Module) {
    for stmt in &mut module.stmts {
        mangle_stmt(stmt, None);
    }
}
