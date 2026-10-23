//! Conditional function bindings retain original before-assignment categories.
use super::{
    branches::{Branch, Path},
    collector::{Binding, Reference},
};
use crate::infrastructure::project::symbols::{DefinitionKind, Symbols};
use std::collections::BTreeMap;

fn covers(paths: &[&[(u32, Branch)]], counts: &BTreeMap<u32, usize>) -> bool {
    if paths.iter().any(|path| path.is_empty()) {
        return true;
    }
    for path in paths {
        let Some(&(id, branch)) = path.first() else {
            continue;
        };
        match branch {
            Branch::If(_) => {
                let Some(&count) = counts.get(&id) else {
                    continue;
                };
                if (0..count).all(|branch| {
                    let children: Vec<_> = paths
                        .iter()
                        .filter(|path| path.first() == Some(&(id, Branch::If(branch))))
                        .map(|path| &path[1..])
                        .collect();
                    !children.is_empty() && covers(&children, counts)
                }) {
                    return true;
                }
            }
            Branch::TryBody | Branch::TryElse | Branch::TryFinally => {
                let children: Vec<_> = paths
                    .iter()
                    .filter(|path| path.first() == Some(&(id, branch)))
                    .map(|path| &path[1..])
                    .collect();
                if covers(&children, counts) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn relative<'a>(definition: &'a Path, usage: &Path) -> &'a [(u32, Branch)] {
    let shared = definition
        .iter()
        .zip(usage)
        .take_while(|(first, second)| first == second)
        .count();
    &definition[shared..]
}

pub(super) fn collect(
    symbols: &Symbols,
    bindings: &BTreeMap<String, Vec<Binding>>,
    references: &[Reference],
    counts: &BTreeMap<u32, usize>,
    issues: &mut BTreeMap<u32, bool>,
    expired: &mut std::collections::BTreeSet<u32>,
) {
    for usage in references {
        if symbols.globals.contains(&usage.name)
            || symbols.nonlocals.contains(&usage.name)
            || !symbols.bindings.get(&usage.name).is_some_and(|value| {
                matches!(
                    value.kind,
                    DefinitionKind::Function | DefinitionKind::Value | DefinitionKind::Import(_)
                )
            })
        {
            continue;
        }
        let Some(bindings) = bindings.get(&usage.name) else {
            continue;
        };
        if bindings.iter().all(|binding| {
            binding.handler_name
                && !binding
                    .branches
                    .iter()
                    .all(|branch| usage.branches.contains(branch))
        }) {
            issues.insert(usage.range.start().to_u32(), false);
            expired.insert(usage.range.start().to_u32());
            continue;
        }
        let annotation_only = bindings.iter().all(|value| value.annotation_only);
        let before: Vec<_> = bindings
            .iter()
            .filter(|value| !value.annotation_only && value.range.start() < usage.range.start())
            .collect();
        if before.is_empty() {
            if annotation_only {
                issues.insert(usage.range.start().to_u32(), false);
            }
            continue;
        }
        let paths: Vec<_> = before
            .iter()
            .map(|value| relative(&value.branches, &usage.branches))
            .collect();
        if covers(&paths, counts) {
            continue;
        }
        let conditional = paths.iter().any(|path| {
            path.iter()
                .any(|(_, branch)| matches!(branch, Branch::If(_)))
        });
        issues.insert(usage.range.start().to_u32(), conditional);
    }
}
