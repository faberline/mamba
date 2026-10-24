use super::run;
use crate::domain::model::{QualityOperation, QualityRequest};
use std::{collections::BTreeMap, fs, path::Path, time::SystemTime};

const CHECK_SOURCE: &str = "\"\"\"Command fixture.\"\"\"\nVALUE: int = 1\n";
const FIX_SOURCE: &str = "import sys\nimport os\n\nVALUE='one'\n";

type Snapshot = BTreeMap<std::path::PathBuf, (bool, bool, SystemTime, Vec<u8>)>;

fn snapshot(root: &Path) -> Snapshot {
    fn visit(root: &Path, path: &Path, entries: &mut Snapshot) {
        let metadata = fs::symlink_metadata(path).unwrap();
        assert!(!metadata.file_type().is_symlink());
        entries.insert(
            path.strip_prefix(root).unwrap().to_path_buf(),
            (
                metadata.is_dir(),
                metadata.permissions().readonly(),
                metadata.modified().unwrap(),
                if metadata.is_file() {
                    fs::read(path).unwrap()
                } else {
                    Vec::new()
                },
            ),
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), entries);
            }
        }
    }
    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries);
    entries
}

#[test]
fn readonly_requests_keep_the_complete_project_tree_unchanged() {
    // Reuse the existing CLI inputs and targets. Do not add reference cases.
    for version in ["3.12", "3.13", "3.14"] {
        for (operation, source) in [
            (QualityOperation::Check, CHECK_SOURCE),
            (QualityOperation::Fix { diff: true }, FIX_SOURCE),
            (
                QualityOperation::Format {
                    check: false,
                    diff: true,
                },
                FIX_SOURCE,
            ),
            (
                QualityOperation::Format {
                    check: true,
                    diff: false,
                },
                FIX_SOURCE,
            ),
        ] {
            let project = tempfile::tempdir().unwrap();
            let root = fs::canonicalize(project.path()).unwrap();
            fs::write(root.join("main.py"), source).unwrap();
            fs::write(
                root.join("pyproject.toml"),
                format!("[project]\nrequires-python = '>={version}'\n"),
            )
            .unwrap();
            let before = snapshot(&root);
            let report = run(&QualityRequest::new(operation, vec![root.clone()]));
            assert_eq!(report.files.checked, 1, "{version} {operation:?}");
            assert_eq!(report.files.written, 0, "{version} {operation:?}");
            assert_eq!(snapshot(&root), before, "{version} {operation:?}");
        }
    }
}

#[test]
fn writing_fix_keeps_the_existing_complete_report_cache() {
    let project = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(project.path()).unwrap();
    fs::write(root.join("main.py"), FIX_SOURCE).unwrap();
    fs::write(
        root.join("pyproject.toml"),
        "[project]\nrequires-python = '>=3.12'\n",
    )
    .unwrap();
    let report = run(&QualityRequest::new(
        QualityOperation::Fix { diff: false },
        vec![root.clone()],
    ));
    assert_eq!(report.files.checked, 1);
    let cache = root.join(".mamba-quality-cache/v2");
    let reports: Vec<_> = fs::read_dir(cache)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    assert!(!reports.is_empty());
    assert!(reports.iter().any(|path| {
        let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        value["schema"] == "mamba.quality.cache.v2"
    }));
}
