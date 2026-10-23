use super::*;

/// Desugar all PEP 695 constructs in a module (in place).
pub fn desugar_module(module: &mut Module) {
    let hoist = desugar_block(&mut module.stmts, false, &[], None, None);
    // Module scope absorbs everything; nothing can hoist past it.
    debug_assert!(hoist.before.is_empty() && hoist.after.is_empty());
}
