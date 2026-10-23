//! Global and nonlocal declaration metadata preserve Pylint lookup decisions.
use super::{Definition, DefinitionKind, Kind, Scopes};
use ruff_text_size::TextRange;

impl Scopes {
    pub(in crate::infrastructure::pylint) fn outer(&self, name: &str) -> Option<&Definition> {
        let current = self.stack.last().unwrap();
        if current.redirects.contains_key(name)
            && !current.symbols.bindings.get(name).is_some_and(|value| {
                matches!(value.kind, DefinitionKind::Parameter { .. })
                    || current
                        .global_positions
                        .get(name)
                        .is_some_and(|global| value.range.start() < global.start())
            })
        {
            return None;
        }
        self.global(name)
    }

    pub(in crate::infrastructure::pylint) fn global(&self, name: &str) -> Option<&Definition> {
        self.module_definitions
            .contains(name)
            .then(|| self.stack.first().unwrap().symbols.bindings.get(name))
            .flatten()
    }

    pub(in crate::infrastructure::pylint) fn global_import(&self, name: &str) -> bool {
        self.stack[0]
            .symbols
            .bindings
            .get(name)
            .is_some_and(|value| matches!(value.kind, DefinitionKind::Import(_)))
    }

    pub(in crate::infrastructure::pylint) fn last_assignment(
        &self,
        name: &str,
    ) -> Option<TextRange> {
        self.stack.last().unwrap().assignments.get(name).copied()
    }

    pub(in crate::infrastructure::pylint) fn has_nonlocal_binding(&self, name: &str) -> bool {
        self.stack
            .iter()
            .take(self.current_index())
            .any(|scope| scope.kind != Kind::Module && scope.symbols.bindings.contains_key(name))
    }
}
