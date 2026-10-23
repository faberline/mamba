//! An unresolved import cycle differs from a declared or failed imported symbol.
use super::{project, Lookup, ModuleDeclarations, ProjectIndex};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn reexports(
    index: &ProjectIndex<'_>,
    declarations: &[Option<ModuleDeclarations<'_>>],
) -> Vec<BTreeMap<String, (String, String)>> {
    index
        .modules
        .iter()
        .zip(declarations)
        .map(|(module, declarations)| {
            declarations
                .as_ref()
                .map(|declarations| {
                    project::imports(declarations.body(), &module.name, module.is_package)
                        .into_iter()
                        .filter_map(|import| {
                            let member = import.member.filter(|member| member != "*")?;
                            Some((import.local, (import.module, member)))
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect()
}

impl Lookup {
    pub(super) fn unresolved_import_cycle(&self, module: &str, member: &str) -> bool {
        let mut current = (module.to_string(), member.to_string());
        let mut seen = BTreeSet::new();
        while let Some(id) = self.module_id(&current.0) {
            if !seen.insert((id, current.1.clone())) {
                return true;
            }
            let Some(next) = self.modules.reexports[id].get(&current.1) else {
                return false;
            };
            current = next.clone();
        }
        false
    }
}
