//! Import usage retains all traversed dotted imports, including nested statements.
use super::{DefinitionKind, Import, Kind, Scopes};
impl Scopes {
    pub(in crate::infrastructure::pylint) fn record_import(&mut self, import: &Import) {
        let scope = self.stack.last_mut().unwrap();
        if !scope.imports.iter().any(|value| {
            value.module == import.module
                && value.member == import.member
                && value.range == import.range
        }) {
            scope.imports.push(import.clone());
        }
    }
    pub(in crate::infrastructure::pylint) fn leave(&mut self) -> Vec<Import> {
        let scope = self.stack.pop().expect("lexical scope");
        if scope.kind == Kind::Class {
            return Vec::new();
        }
        let mut unused: Vec<_> = scope
            .symbols
            .bindings
            .into_iter()
            .filter_map(|(name, definition)| match definition.kind {
                DefinitionKind::Import(import)
                    if !scope.used.contains(&name) && import.module != "__future__" =>
                {
                    Some(import)
                }
                _ => None,
            })
            .collect();
        for import in &mut unused {
            if import.member.as_deref() == Some("*") {
                continue;
            }
            if let Some(first) = scope.imports.iter().find(|first| {
                first.range == import.range
                    && first.module == import.module
                    && first.member == import.member
            }) {
                *import = first.clone();
            }
        }
        for import in &scope.imports {
            if (import.member.is_none()
                || (scope.kind == Kind::Module && import.member.as_deref() != Some("*")))
                && (scope.kind != Kind::Function
                    || !scope.imports.iter().any(|first| {
                        first.range.start() < import.range.start() && first.local == import.local
                    }))
                && !scope.used.contains(&import.local)
                && import.module != "__future__"
                && !unused.iter().any(|value| {
                    value.module == import.module
                        && value.member == import.member
                        && value.range == import.range
                })
            {
                unused.push(import.clone());
            }
        }
        unused.sort_by_key(|import| {
            (
                import.range.start(),
                scope.imports.iter().position(|candidate| {
                    candidate.local == import.local && candidate.range == import.range
                }),
            )
        });
        unused
    }
}
