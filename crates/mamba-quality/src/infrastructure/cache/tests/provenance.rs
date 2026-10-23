//! A cached mypy report must retain the input spelling and package discovery state.
use super::*;
use crate::domain::source::{RequestedSource, SourceRequest};

fn request(path: &str, canonical: &str) -> SourceRequest {
    SourceRequest {
        path: path.into(),
        canonical_path: canonical.into(),
        directory: false,
        sources: vec![RequestedSource {
            path: path.into(),
            source_path: canonical.into(),
            package_files: Vec::new(),
        }],
    }
}

#[test]
fn mypy_cache_keeps_request_order_aliases_and_package_anchors() {
    let profile = FixedProfile::locked();
    let workspace = QualityWorkspace {
        source_requests: vec![
            request("/frozen/a.py", "/frozen/a.py"),
            request("/frozen/b.py", "/frozen/b.py"),
        ],
        ..QualityWorkspace::default()
    };
    let key = |workspace: &QualityWorkspace, tool| {
        fingerprint::key(workspace, &profile, tool, "fixed-test-binary").unwrap()
    };
    let initial = key(&workspace, QualityTool::Mypy);
    let mut cases = Vec::new();
    let mut changed = workspace.clone();
    changed.source_requests.reverse();
    cases.push(("order", changed));
    let mut changed = workspace.clone();
    changed
        .source_requests
        .push(changed.source_requests[0].clone());
    cases.push(("duplicate", changed));
    let mut changed = workspace.clone();
    changed.source_requests[0].path = "/frozen/./a.py".into();
    changed.source_requests[0].sources[0].path = "/frozen/./a.py".into();
    cases.push(("lexical_alias", changed));
    let mut changed = workspace.clone();
    changed.source_requests[0].canonical_path = "/frozen/other/a.py".into();
    changed.source_requests[0].sources[0].source_path = "/frozen/other/a.py".into();
    cases.push(("canonical_mapping", changed));
    let mut changed = workspace.clone();
    changed.source_requests[0].directory = true;
    cases.push(("directory_request", changed));
    let mut changed = workspace.clone();
    changed.source_requests[0].sources[0]
        .package_files
        .push("/frozen/__init__.py".into());
    cases.push(("package_anchor", changed));
    let mut changed = workspace.clone();
    changed.source_requests.clear();
    cases.push(("absent_provenance", changed));
    let mut rows = Vec::new();
    for (name, changed) in cases {
        let mypy = key(&changed, QualityTool::Mypy);
        assert_ne!(initial, mypy, "stale mypy cache for {name}");
        let mut other = Vec::new();
        for tool in [
            QualityTool::Ruff,
            QualityTool::Isort,
            QualityTool::Black,
            QualityTool::Pylint,
        ] {
            let before = key(&workspace, tool);
            let after = key(&changed, tool);
            assert_eq!(before, after, "unrelated tool key changed for {name}");
            other.push(serde_json::json!({"tool": tool, "before": before, "after": after}));
        }
        rows.push(serde_json::json!({"case": name, "mypy_before": initial, "mypy_after": mypy, "other_tools": other}));
    }
    if let Some(path) = std::env::var_os("MAMBA_QUALITY_ROOT_CACHE_EVIDENCE") {
        use std::io::Write;
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        out.write_all(&serde_json::to_vec_pretty(&rows).unwrap())
            .unwrap();
    }
}
