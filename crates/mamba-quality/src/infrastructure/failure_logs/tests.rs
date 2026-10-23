use super::*;
use std::cell::Cell;

fn resolve_for(
    temporary: &Path,
    projects: &[PathBuf],
    fallback: impl FnOnce() -> io::Result<PathBuf>,
) -> io::Result<Option<PathBuf>> {
    FailureLogs::new(true)
        .destination_for(temporary, projects, Creation::File, |_, _| Ok(()), fallback)
        .map(|path| path.map(Path::to_path_buf))
}

struct Fixture {
    _root: tempfile::TempDir,
    first: PathBuf,
    second: PathBuf,
    outside: PathBuf,
    fallback: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(root.path()).unwrap();
        let first = base.join("project");
        let second = base.join("second");
        let outside = base.join("outside");
        let fallback = base.join("fallback");
        for path in [&first, &second, &outside, &fallback] {
            fs::create_dir(path).unwrap();
        }
        Self {
            _root: root,
            first,
            second,
            outside,
            fallback,
        }
    }

    fn projects(&self) -> Vec<PathBuf> {
        vec![self.first.clone(), self.second.clone()]
    }

    fn resolve(&self, temporary: &Path) -> io::Result<Option<PathBuf>> {
        resolve_for(temporary, &self.projects(), || Ok(self.fallback.clone()))
    }
}

#[test]
fn check_failure_log_destination_injected_paths_require_absolute_inputs() {
    let fixture = Fixture::new();
    for (temporary, roots) in [
        (Path::new("relative-temp"), fixture.projects()),
        (
            fixture.first.as_path(),
            vec![PathBuf::from("relative-project")],
        ),
    ] {
        let error = resolve_for(temporary, &roots, || {
            panic!("Invalid injected inputs must not inspect a fallback")
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}

#[test]
fn check_failure_log_destination_outside_does_not_call_fallback() {
    let fixture = Fixture::new();
    let result = resolve_for(&fixture.outside, &fixture.projects(), || {
        panic!("An outside TMPDIR must not inspect the fallback")
    });
    assert_eq!(result.unwrap(), None);
}

#[test]
fn check_failure_log_destination_missing_outside_keeps_original_builder() {
    let fixture = Fixture::new();
    let missing = fixture.outside.join("missing");
    let result = resolve_for(&missing, &fixture.projects(), || {
        panic!("A missing outside TMPDIR must keep the original builder")
    });
    assert_eq!(result.unwrap(), None);
    assert!(!missing.exists());
}

#[test]
fn check_failure_log_destination_first_project_redirects() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.resolve(&fixture.first).unwrap(),
        Some(fixture.fallback.clone())
    );
}

#[test]
fn check_failure_log_destination_second_project_redirects() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.resolve(&fixture.second).unwrap(),
        Some(fixture.fallback.clone())
    );
}

#[test]
fn check_failure_log_destination_similar_prefix_stays_outside() {
    let fixture = Fixture::new();
    let sibling = fixture.first.with_file_name("project-copy");
    fs::create_dir(&sibling).unwrap();
    let result = resolve_for(&sibling, &fixture.projects(), || {
        panic!("A shared text prefix is not project membership")
    });
    assert_eq!(result.unwrap(), None);
}

#[cfg(unix)]
#[test]
fn check_failure_log_destination_symlink_into_project_redirects() {
    let fixture = Fixture::new();
    let alias = fixture.outside.join("alias");
    std::os::unix::fs::symlink(&fixture.second, &alias).unwrap();
    assert_eq!(
        fixture.resolve(&alias).unwrap(),
        Some(fixture.fallback.clone())
    );
}

#[cfg(unix)]
#[test]
fn check_failure_log_destination_symlink_out_of_project_stays_outside() {
    let fixture = Fixture::new();
    let alias = fixture.first.join("alias");
    std::os::unix::fs::symlink(&fixture.outside, &alias).unwrap();
    let result = resolve_for(&alias, &fixture.projects(), || {
        panic!("Physical outside membership must keep the original builder")
    });
    assert_eq!(result.unwrap(), None);
}

#[test]
fn check_failure_log_destination_missing_in_project_suffix_keeps_original_builder() {
    let fixture = Fixture::new();
    let missing = fixture.first.join("missing").join("nested");
    assert_eq!(fixture.resolve(&missing).unwrap(), None);
    assert!(!fixture.first.join("missing").exists());
}

#[test]
fn check_failure_log_destination_missing_suffix_dot_components_keep_original_error() {
    let fixture = Fixture::new();
    let temporary = fixture.first.join("missing").join("..").join("tmp");
    assert_eq!(fixture.resolve(&temporary).unwrap(), None);
    assert!(!fixture.first.join("missing").exists());
    assert!(!fixture.first.join("tmp").exists());
}

#[test]
fn check_failure_log_destination_fallback_in_either_project_is_error() {
    let fixture = Fixture::new();
    for fallback in fixture.projects() {
        let error =
            resolve_for(&fixture.first, &fixture.projects(), || Ok(fallback.clone())).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("inside an analyzed project"));
    }
}

#[cfg(unix)]
#[test]
fn check_failure_log_destination_fallback_symlink_into_project_is_error() {
    let fixture = Fixture::new();
    let alias = fixture.outside.join("fallback-alias");
    std::os::unix::fs::symlink(&fixture.second, &alias).unwrap();
    let error = resolve_for(&fixture.first, &fixture.projects(), || Ok(alias)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn check_failure_log_destination_missing_fallback_is_error_without_mkdir() {
    let fixture = Fixture::new();
    let missing = fixture.outside.join("missing-fallback");
    let error =
        resolve_for(&fixture.first, &fixture.projects(), || Ok(missing.clone())).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(!missing.exists());
}

#[test]
fn check_failure_log_destination_non_directory_fallback_is_error() {
    let fixture = Fixture::new();
    let file = fixture.outside.join("fallback-file");
    fs::write(&file, b"existing data").unwrap();
    let error = resolve_for(&fixture.first, &fixture.projects(), || Ok(file.clone())).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotADirectory);
    assert_eq!(fs::read(file).unwrap(), b"existing data");
}

#[test]
fn check_failure_log_destination_accepts_caller_created_fallback() {
    let fixture = Fixture::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            fs::metadata(&fixture.fallback).unwrap().uid(),
            fs::metadata(fixture._root.path()).unwrap().uid(),
        );
    }
    let logs = FailureLogs::new(true);
    let selected = logs
        .destination_for(
            &fixture.first,
            &fixture.projects(),
            Creation::File,
            |_, _| Ok(()),
            || Ok(fixture.fallback.clone()),
        )
        .unwrap();
    assert_eq!(selected, Some(fixture.fallback.as_path()));
    let file = create_file(selected, "blk_", ".log").unwrap();
    assert_eq!(file.path().parent(), Some(fixture.fallback.as_path()));
    let name = file.path().file_name().unwrap().to_str().unwrap();
    assert!(name.starts_with("blk_") && name.ends_with(".log"));
    let selected = logs
        .destination_for(
            &fixture.first,
            &fixture.projects(),
            Creation::Directory,
            |_, _| Ok(()),
            || panic!("The fallback is shared across file and directory creation"),
        )
        .unwrap();
    let directory = create_directory(selected, "mamba-quality-pylint-").unwrap();
    assert_eq!(directory.path().parent(), Some(fixture.fallback.as_path()));
    assert!(directory
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("mamba-quality-pylint-"));
}

#[test]
fn check_failure_log_destination_disabled_does_not_resolve_or_cache() {
    let logs = FailureLogs::new(false);
    let result = logs.destination_with(|| panic!("Disabled policy must not resolve paths"));
    assert_eq!(result.unwrap(), None);
    assert!(logs.destination.get().is_none());
}

#[test]
fn check_failure_log_destination_is_lazy_once_per_request() {
    let fixture = Fixture::new();
    let logs = FailureLogs::new(true);
    let calls = Cell::new(0);
    assert!(logs.destination.get().is_none());
    let first = logs
        .destination_with(|| {
            calls.set(calls.get() + 1);
            fixture.resolve(&fixture.first)
        })
        .unwrap();
    let second = logs
        .destination_with(|| panic!("Same request must reuse its decision"))
        .unwrap();
    assert_eq!(first, Some(fixture.fallback.as_path()));
    assert_eq!(second, first);
    assert_eq!(calls.get(), 1);
    let next_request = FailureLogs::new(true);
    assert_eq!(
        next_request
            .destination_with(|| {
                calls.set(calls.get() + 1);
                Ok(None)
            })
            .unwrap(),
        None
    );
    assert_eq!(calls.get(), 2);
}

#[test]
fn check_failure_log_destination_cached_io_error_preserves_os_code() {
    let fixture = Fixture::new();
    let logs = FailureLogs::new(true);
    let first = logs
        .destination_with(|| {
            resolve_for(&fixture.first, &fixture.projects(), || {
                Err(io::Error::from_raw_os_error(13))
            })
        })
        .unwrap_err();
    let second = logs
        .destination_with(|| panic!("A failed decision is cached"))
        .unwrap_err();
    assert_eq!(first.raw_os_error(), Some(13));
    assert_eq!(second.raw_os_error(), first.raw_os_error());
    assert_eq!(second.to_string(), first.to_string());
}

#[test]
fn check_failure_log_destination_ordinary_file_keeps_original_builder() {
    let fixture = Fixture::new();
    let file = fixture.first.join("not-a-directory");
    fs::write(&file, b"unchanged").unwrap();
    assert_eq!(fixture.resolve(&file).unwrap(), None);
    assert_eq!(fs::read(file).unwrap(), b"unchanged");
}

#[cfg(unix)]
#[test]
fn check_failure_log_destination_dangling_symlink_keeps_original_builder() {
    let fixture = Fixture::new();
    let alias = fixture.first.join("dangling");
    std::os::unix::fs::symlink(fixture.outside.join("missing"), &alias).unwrap();
    assert_eq!(fixture.resolve(&alias).unwrap(), None);
    assert!(fs::symlink_metadata(alias)
        .unwrap()
        .file_type()
        .is_symlink());
}
