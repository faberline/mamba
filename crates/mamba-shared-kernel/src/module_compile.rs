//! Contracts for module compilation without a compiler dependency.

use std::collections::HashMap;
use std::path::Path;

/// Owns executable storage. Dropping the lease releases its code.
/// A lease stays on its execution thread and has no Send or Sync bound.
pub trait CodeLease {}

impl<T: 'static> CodeLease for T {}

/// Transfers a namespace without copying its values or allocating a second map.
pub struct CompiledModule<Value> {
    pub attributes: HashMap<String, Value>,
    pub code: Box<dyn CodeLease>,
}

/// The host performs one independent compilation and execution per call.
pub type ModuleCompiler<Value> = fn(&Path, &str) -> Option<CompiledModule<Value>>;
