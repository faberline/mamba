//! Append checker blocks without changing checker execution or note allocation.

use super::dependencies::Loaded;
use crate::{domain::model::*, infrastructure::project::ProjectIndex};
use std::collections::BTreeMap;

pub(super) fn append(
    report: &mut ToolReport,
    loaded: &Loaded,
    index: &ProjectIndex<'_>,
    blocks: Vec<(usize, Vec<QualityDiagnostic>)>,
) {
    let original: Vec<_> = blocks.iter().map(|(source, _)| *source).collect();
    let mut blocks: BTreeMap<_, _> = blocks.into_iter().collect();
    let mut metadata = None;
    for project in 0..loaded.workspace.projects.len() {
        // Zero or one visible block has the same projection under every file order.
        let visible = blocks
            .iter()
            .filter(|(source, rows)| index.modules[**source].project == project && !rows.is_empty())
            .take(2)
            .count();
        if visible <= 1 {
            for &source in &original {
                if index.modules[source].project == project {
                    if let Some(rows) = blocks.remove(&source) {
                        report.diagnostics.extend(rows);
                    }
                }
            }
            continue;
        }
        let metadata = metadata.get_or_insert_with(|| loaded.output_modules(index));
        let nodes: Vec<_> = metadata
            .iter()
            .filter(|module| module.project == project)
            .collect();
        let ids: BTreeMap<_, _> = nodes
            .iter()
            .enumerate()
            .map(|(id, module)| (module.source, id))
            .collect();
        let names: Vec<_> = nodes.iter().map(|module| module.name.clone()).collect();
        let encounter: Vec<_> = nodes.iter().map(|module| module.encounter).collect();
        let reported: Vec<_> = nodes
            .iter()
            .map(|module| {
                blocks
                    .get(&module.source)
                    .is_some_and(|rows| !rows.is_empty())
            })
            .collect();
        let mut unresolved = None;
        let dependencies: Option<Vec<_>> = nodes
            .iter()
            .map(|module| {
                let imports = module.dependencies.as_ref()?;
                let mut edges = Vec::new();
                for edge in imports {
                    if let Some(id) = edge.target.and_then(|target| ids.get(&target).copied()) {
                        // Metadata None here denotes a resolved direct edge below 30.
                        edges.push((id, edge.priority));
                    } else if edge.is_module != Some(false) && unresolved.is_none() {
                        unresolved = Some(format!("{} imports {}", module.name, edge.name));
                    }
                }
                Some(edges)
            })
            .collect();
        if let Some(dependency) = unresolved {
            report.remaining.push(format!(
                "mypy output graph remains incomplete for {}: unresolved or unrepresented dependency ({dependency})",
                loaded.workspace.projects[project].root.display()
            ));
        }
        let order = dependencies.and_then(|dependencies| {
            super::output_order::file_order(&names, &encounter, &dependencies, &reported)
        });
        if let Some(order) = order {
            for id in order {
                if let Some(rows) = blocks.remove(&nodes[id].source) {
                    report.diagnostics.extend(rows);
                }
            }
        } else {
            report.remaining.push(format!(
                "mypy diagnostic block order remains incomplete for {}: unavailable import metadata or unresolved cycle priorities",
                loaded.workspace.projects[project].root.display()
            ));
            for &source in &original {
                if index.modules[source].project == project {
                    if let Some(rows) = blocks.remove(&source) {
                        report.diagnostics.extend(rows);
                    }
                }
            }
        }
    }
    // Preserve every original block if a malformed graph did not account for it.
    for source in original {
        if let Some(rows) = blocks.remove(&source) {
            if !rows.is_empty() {
                report
                    .remaining
                    .push("mypy diagnostic source is not represented in the output graph".into());
                report.diagnostics.extend(rows);
            }
        }
    }
}
