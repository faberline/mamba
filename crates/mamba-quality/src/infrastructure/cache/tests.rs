use super::*;
mod admission;
mod negative;
mod provenance;
use crate::domain::ports::ProjectStore;
use crate::infrastructure::{filesystem::Filesystem, native::NativeEngine};

#[test]
fn never_reuses_a_diagnostic_after_its_source_changes() {
    let project = tempfile::tempdir().unwrap();
    let path = project.path().join("main.py");
    fs::write(&path, "import os\n").unwrap();
    let engine = CachedEngine::new(NativeEngine);
    let profile = FixedProfile::locked();
    let workspace = Filesystem.load(std::slice::from_ref(&path), true);
    let first = engine.check(&workspace, &profile, QualityTool::Ruff);
    assert_eq!(first.diagnostics[0].rule, "F401");
    let cached = engine.check(&workspace, &profile, QualityTool::Ruff);
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(cached).unwrap()
    );
    fs::write(&path, "import os\nos.getcwd()\n").unwrap();
    let changed = Filesystem.load(&[path], true);
    assert!(engine
        .check(&changed, &profile, QualityTool::Ruff)
        .diagnostics
        .is_empty());
}

#[test]
fn dependency_target_profile_and_executable_changes_invalidate_the_key() {
    let project = tempfile::tempdir().unwrap();
    let path = project.path().join("main.py");
    fs::write(&path, "VALUE = 1\n").unwrap();
    fs::write(project.path().join("dependency.pyi"), "VALUE: int\n").unwrap();
    let mut workspace = Filesystem.load(&[path], true);
    let mut profile = FixedProfile::locked();
    let initial = fingerprint::key(&workspace, &profile, QualityTool::Ruff, "binary-a").unwrap();
    assert_ne!(
        initial,
        fingerprint::key(&workspace, &profile, QualityTool::Ruff, "binary-b").unwrap()
    );
    fs::write(project.path().join("dependency.pyi"), "VALUE: str\n").unwrap();
    let dependency = fingerprint::key(&workspace, &profile, QualityTool::Ruff, "binary-a").unwrap();
    assert_ne!(initial, dependency);
    workspace.projects[0].python_version = crate::domain::source::PythonVersion::Py314;
    let version = fingerprint::key(&workspace, &profile, QualityTool::Ruff, "binary-a").unwrap();
    assert_ne!(dependency, version);
    profile.ruff.rules.pop();
    assert_ne!(
        version,
        fingerprint::key(&workspace, &profile, QualityTool::Ruff, "binary-a").unwrap()
    );
}

#[test]
fn one_writer_owns_an_entry_and_a_corrupt_report_is_a_miss() {
    let project = tempfile::tempdir().unwrap();
    let directory = fs::canonicalize(project.path())
        .unwrap()
        .join(".mamba-quality-cache/v2");
    let first = Entry::claim(&directory, "test-key").unwrap();
    assert!(Entry::claim(&directory, "test-key").is_none());
    fs::write(&first.path, "{truncated").unwrap();
    assert!(first.read(QualityTool::Ruff, 1).is_none());
    drop(first);
    assert!(Entry::claim(&directory, "test-key").is_some());
}

#[test]
fn namespace_directories_and_native_module_data_invalidate_the_key() {
    let project = tempfile::tempdir().unwrap();
    let path = project.path().join("main.py");
    fs::write(&path, "import namespace\n").unwrap();
    let workspace = Filesystem.load(&[path], true);
    let profile = FixedProfile::locked();
    let key = || fingerprint::key(&workspace, &profile, QualityTool::Ruff, "binary").unwrap();
    let initial = key();
    fs::create_dir(project.path().join("namespace")).unwrap();
    let directory = key();
    assert_ne!(initial, directory);
    fs::write(
        project.path().join("module.cpython-312.so"),
        b"first native module",
    )
    .unwrap();
    let module = key();
    assert_ne!(directory, module);
    fs::write(
        project.path().join("module.cpython-312.so"),
        b"changed native module",
    )
    .unwrap();
    assert_ne!(module, key());
}

#[test]
fn explicit_and_scanned_source_origins_invalidate_the_key() {
    let project = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(project.path()).unwrap();
    fs::write(
        root.join("pyproject.toml"),
        "[project]\nrequires-python = '>=3.12'\n",
    )
    .unwrap();
    let path = root.join("main.py");
    fs::write(&path, "VALUE = 1\n").unwrap();
    let explicit = Filesystem.load(&[path], true);
    let scanned = Filesystem.load(&[root], true);
    assert_eq!(explicit.sources[0].path, scanned.sources[0].path);
    assert_eq!(explicit.sources[0].sha256, scanned.sources[0].sha256);
    let profile = FixedProfile::locked();
    assert_ne!(
        fingerprint::key(&explicit, &profile, QualityTool::Pylint, "binary"),
        fingerprint::key(&scanned, &profile, QualityTool::Pylint, "binary")
    );
}
