//! Corrupt and unrelated cache envelopes must never be accepted.
use super::*;

fn report() -> ToolReport {
    ToolReport {
        tool: QualityTool::Ruff,
        reference_version: QualityTool::Ruff.reference_version().into(),
        completion: CompletionStatus::Complete,
        reference_exit_code: Some(0),
        checked_files: 1,
        diagnostics: Vec::new(),
        remaining: Vec::new(),
    }
}

#[test]
fn wrong_identity_tool_version_count_and_completion_are_cache_misses() {
    let directory = tempfile::tempdir().unwrap();
    let path = fs::canonicalize(directory.path()).unwrap();
    let entry = Entry::claim(&path, "expected-key").unwrap();
    let valid = Envelope {
        schema: CACHE_SCHEMA.into(),
        key: "expected-key".into(),
        report: report(),
    };
    let original = serde_json::to_value(valid).unwrap();
    for (field, value) in [
        ("schema", serde_json::json!("mamba.quality.cache.old")),
        ("key", serde_json::json!("other-key")),
        ("tool", serde_json::json!("black")),
        ("reference_version", serde_json::json!("0.0.0")),
        ("checked_files", serde_json::json!(2)),
        ("completion", serde_json::json!("incomplete")),
        ("remaining", serde_json::json!(["unfinished"])),
    ] {
        let mut changed = original.clone();
        if matches!(field, "schema" | "key") {
            changed[field] = value;
        } else {
            changed["report"][field] = value;
        }
        fs::write(&entry.path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(entry.read(QualityTool::Ruff, 1).is_none(), "{field}");
    }
    entry.write(&report()).unwrap();
    assert!(entry.read(QualityTool::Ruff, 1).is_some());
}

#[cfg(unix)]
#[test]
fn cache_symlinks_do_not_write_external_data() {
    use std::os::unix::fs::symlink;
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let project_path = fs::canonicalize(project.path()).unwrap();
    let outside_path = fs::canonicalize(outside.path()).unwrap();
    let marker = outside_path.join("marker");
    fs::write(&marker, b"external data").unwrap();
    let cache = project_path.join(".mamba-quality-cache");
    symlink(&outside_path, &cache).unwrap();
    assert!(Entry::claim(&cache.join("v2"), "key").is_none());
    assert_eq!(fs::read(&marker).unwrap(), b"external data");
    assert_eq!(fs::read_dir(&outside_path).unwrap().count(), 1);
    fs::remove_file(&cache).unwrap();
    let cache = project_path.join("cache");
    fs::create_dir(&cache).unwrap();
    symlink(&marker, cache.join("key.lock")).unwrap();
    assert!(Entry::claim(&cache, "key").is_none());
    assert_eq!(fs::read(&marker).unwrap(), b"external data");
}

#[cfg(unix)]
#[test]
fn report_path_symlink_is_a_miss_and_external_bytes_survive() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let cache = fs::canonicalize(directory.path()).unwrap();
    let marker = outside.path().join("marker");
    fs::write(&marker, b"external bytes").unwrap();
    let entry = Entry::claim(&cache, "expected-key").unwrap();
    symlink(&marker, &entry.path).unwrap();
    assert!(entry.read(QualityTool::Ruff, 1).is_none());
    entry.write(&report()).unwrap();
    assert_eq!(fs::read(marker).unwrap(), b"external bytes");
    assert!(fs::symlink_metadata(&entry.path)
        .unwrap()
        .file_type()
        .is_file());
    assert!(entry.read(QualityTool::Ruff, 1).is_some());
}
