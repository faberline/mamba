use crate::infrastructure::embedded;
use mamba_language::application::quality_syntax::{self, python_ast::Expr, PythonTarget};
use std::{cell::RefCell, collections::BTreeMap};

const MAX_CACHED_MODULES: usize = 64;
const MAX_CACHED_BINDINGS: usize = 8_192;

#[derive(Default)]
pub(super) struct EmbeddedBindings {
    modules: RefCell<BTreeMap<String, BTreeMap<String, Expr>>>,
    #[cfg(test)]
    stats: RefCell<CacheStats>,
}

impl EmbeddedBindings {
    pub(super) fn binding(
        &self,
        module: &str,
        source: &str,
        target: PythonTarget,
        name: &str,
    ) -> Option<Expr> {
        if let Some(bindings) = self.modules.borrow().get(module) {
            #[cfg(test)]
            {
                self.stats.borrow_mut().hits += 1;
            }
            return bindings.get(name).cloned();
        }
        #[cfg(test)]
        {
            self.stats.borrow_mut().misses += 1;
            self.stats.borrow_mut().parse_attempts += 1;
        }
        let document = match quality_syntax::parse(source, target, false) {
            Ok(document) => document,
            Err(_) => {
                #[cfg(test)]
                {
                    self.stats.borrow_mut().parse_failures += 1;
                }
                return None;
            }
        };
        let bindings = super::super::source_bindings::collect(&document.parsed.syntax().body);
        let value = bindings.get(name).cloned();
        let existing_bindings = self
            .modules
            .borrow()
            .values()
            .map(BTreeMap::len)
            .sum::<usize>();
        if self.modules.borrow().len() < MAX_CACHED_MODULES
            && existing_bindings.saturating_add(bindings.len()) <= MAX_CACHED_BINDINGS
        {
            self.modules
                .borrow_mut()
                .insert(module.to_owned(), bindings);
            #[cfg(test)]
            {
                self.stats.borrow_mut().insertions += 1;
            }
        }
        value
    }

    #[cfg(test)]
    pub(super) fn stats(&self) -> CacheStats {
        *self.stats.borrow()
    }

    #[cfg(test)]
    pub(super) fn cached_modules(&self) -> usize {
        self.modules.borrow().len()
    }

    #[cfg(test)]
    pub(super) fn cached_bindings(&self) -> usize {
        self.modules.borrow().values().map(BTreeMap::len).sum()
    }

    #[cfg(test)]
    pub(super) const fn test_module_limit() -> usize {
        MAX_CACHED_MODULES
    }

    #[cfg(test)]
    pub(super) const fn test_binding_limit() -> usize {
        MAX_CACHED_BINDINGS
    }
}

impl super::Modules {
    pub(in crate::infrastructure::pylint) fn embedded_binding(
        &self,
        project: usize,
        module: &str,
        name: &str,
    ) -> Option<Option<Expr>> {
        let source = self.source(project, module)?;
        let embedded_source = embedded::module(module)?;
        if source.len() != embedded_source.len() || source.as_ptr() != embedded_source.as_ptr() {
            return None;
        }
        Some(
            self.embedded_bindings
                .binding(module, source, self.target, name),
        )
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct CacheStats {
    pub(super) hits: usize,
    pub(super) misses: usize,
    pub(super) parse_attempts: usize,
    pub(super) parse_failures: usize,
    pub(super) insertions: usize,
}

#[cfg(test)]
mod tests;
