use super::*;

#[test]
fn resolves_pep440_minima_and_exclusions() {
    for (requirement, expected) in [
        (None, PythonVersion::Py312),
        (Some(">=3.12.20,<4"), PythonVersion::Py312),
        (Some(">3.12"), PythonVersion::Py312),
        (Some("~=3.13.1"), PythonVersion::Py313),
        (Some("==3.14.*"), PythonVersion::Py314),
        (Some(">=3.12,!=3.12.*"), PythonVersion::Py313),
        (Some(">=3.11,!=3.11.*"), PythonVersion::Py312),
        (Some(">=3.12.999999,!=3.12.999999"), PythonVersion::Py312),
    ] {
        assert_eq!(
            target_version(requirement).unwrap(),
            expected,
            "{requirement:?}"
        );
    }
    for requirement in [">=3.8", ">=3.15", ">=3.13,<3.12", "not-a-version"] {
        assert!(target_version(Some(requirement)).is_err(), "{requirement}");
    }
}

#[test]
fn reads_actual_environment_data_without_changing_the_project_target() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("pyproject.toml"),
        "[project]\nrequires-python = '>=3.12'\n",
    )
    .unwrap();
    let packages = directory.path().join(".venv/lib/python3.14/site-packages");
    fs::create_dir_all(&packages).unwrap();
    let initial = project(directory.path()).unwrap();
    assert_eq!(initial.python_version, PythonVersion::Py312);
    assert!(initial
        .dependencies
        .iter()
        .any(|source| source.path.as_ref() == Some(&packages)));
    fs::create_dir_all(directory.path().join(".venv/lib/python3.12/site-packages")).unwrap();
    fs::write(
        directory.path().join(".venv/pyvenv.cfg"),
        "version = 3.14.0\n",
    )
    .unwrap();
    let configured = project(directory.path()).unwrap();
    assert_eq!(configured.python_version, PythonVersion::Py312);
    let installed: Vec<_> = configured
        .dependencies
        .iter()
        .filter(|source| source.kind == "installed-package-data")
        .collect();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].path.as_ref(), Some(&packages));
    assert!(configured
        .dependencies
        .iter()
        .any(|source| source.kind == "environment-metadata" && source.sha256.is_some()));
}
