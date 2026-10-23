//! Initial command-line roots retain lexical names, multiplicity, and order.
//! Shared source snapshots and other tools keep their canonical input set.
mod discovery;
mod names;

use crate::domain::{
    model::{QualityDiagnostic, QualityLocation, QualityTool},
    source::QualityWorkspace,
};
use std::collections::BTreeMap;

const RESOLUTION_HINT: &str = "See https://mypy.readthedocs.io/en/stable/running_mypy.html#mapping-file-paths-to-modules for more info\nCommon resolutions include:\n    a) using `--exclude` to avoid checking one of them,\n    b) adding `__init__.py` somewhere,\n    c) using `--explicit-package-bases` or adjusting `MYPYPATH`";

pub(super) struct Prepared {
    pub(super) workspace: QualityWorkspace,
    pub(super) names: BTreeMap<usize, String>,
}

pub(super) fn prepare(workspace: &QualityWorkspace) -> Result<Prepared, Box<QualityDiagnostic>> {
    // Directly constructed workspaces have no command-line provenance. Keep
    // their existing snapshot behavior rather than inventing request order.
    if workspace.source_requests.is_empty() {
        return Ok(Prepared {
            workspace: workspace.clone(),
            names: BTreeMap::new(),
        });
    }
    let snapshots: BTreeMap<_, _> = workspace
        .sources
        .iter()
        .map(|s| (s.path.clone(), s))
        .collect();
    let mut result = QualityWorkspace {
        projects: workspace.projects.clone(),
        explicit_files: workspace.explicit_files.clone(),
        scan_roots: workspace.scan_roots.clone(),
        source_requests: workspace.source_requests.clone(),
        discovered: workspace.discovered,
        sources: Vec::with_capacity(workspace.sources.len()),
        issues: workspace.issues.clone(),
    };
    let mut modules = BTreeMap::new();
    let mut source_names = BTreeMap::new();
    for request in &workspace.source_requests {
        for requested in discovery::sources(request) {
            let Some(snapshot) = snapshots.get(&requested.source_path) else {
                // Failed source loads already belong to shared workspace issues.
                continue;
            };
            let path = names::normalize(&requested.path);
            let name = names::module(&path, &requested.package_files)?;
            let name = if name.is_empty() {
                "__main__".into()
            } else {
                name
            };
            if let Some(previous) = modules.insert(name.clone(), path.clone()) {
                return Err(Box::new(QualityDiagnostic {
                    tool: QualityTool::Mypy,
                    rule: String::new(),
                    message: format!(
                        "Duplicate module named \"{name}\" (also at \"{}\")",
                        previous.display()
                    ),
                    hint: Some(RESOLUTION_HINT.into()),
                    path,
                    location: QualityLocation::unpositioned(),
                    severity: "error".into(),
                    fix: None,
                }));
            }
            let mut source = (**snapshot).clone();
            source.path = path;
            source.is_stub = source
                .path
                .extension()
                .is_some_and(|extension| extension == "pyi");
            source_names.insert(result.sources.len(), name);
            result.sources.push(source);
        }
    }
    Ok(Prepared {
        workspace: result,
        names: source_names,
    })
}
