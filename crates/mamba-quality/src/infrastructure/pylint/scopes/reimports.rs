//! Reimport order follows aliases within each original statement.
use super::{Import, Scopes};
pub(in crate::infrastructure::pylint) struct Reimport {
    pub first: Import,
    pub shadowed: bool,
}

impl Scopes {
    pub(in crate::infrastructure::pylint) fn reimports(&self, import: &Import) -> Vec<Reimport> {
        let current = self.current_index();
        let indices = if current == 0 {
            vec![0]
        } else {
            vec![current, 0]
        };
        indices
            .into_iter()
            .filter_map(|index| {
                let imports = &self.stack[index].imports;
                let own = imports.iter().position(|candidate| {
                    candidate.local == import.local && candidate.range == import.range
                });
                imports
                    .iter()
                    .enumerate()
                    .find(|(offset, first)| {
                        if !self.stack[index]
                            .direct_imports
                            .contains(&first.range.start().to_u32())
                            || first.range == import.range
                            || (index == current && own.is_some_and(|own| *offset >= own))
                            || (index == current && first.range.start() > import.range.start())
                        {
                            return false;
                        }
                        let full = |import: &Import| {
                            import.member.as_ref().map_or_else(
                                || import.module.clone(),
                                |name| format!("{}.{name}", import.module),
                            )
                        };
                        full(first) == full(import)
                            || (first.member.is_some()
                                && first.member == import.member
                                && first.alias.is_none()
                                && import.alias.is_none())
                            || (first.alias.is_none()
                                && import.alias.as_ref().is_some_and(|alias| {
                                    alias == first.member.as_ref().unwrap_or(&first.module)
                                }))
                    })
                    .map(|(_, first)| {
                        let full = |value: &Import| {
                            value.member.as_ref().map_or_else(
                                || value.module.clone(),
                                |name| format!("{}.{name}", value.module),
                            )
                        };
                        Reimport {
                            first: first.clone(),
                            shadowed: full(first) != full(import)
                                && !(first.member.is_some()
                                    && first.member == import.member
                                    && first.alias.is_none()
                                    && import.alias.is_none()),
                        }
                    })
            })
            .collect()
    }
}
