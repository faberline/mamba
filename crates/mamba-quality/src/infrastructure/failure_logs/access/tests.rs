use super::super::{create_directory, create_file, Creation, FailureLogs};
use std::{
    cell::Cell,
    fs, io,
    path::{Path, PathBuf},
};

struct Fixture {
    _root: tempfile::TempDir,
    project: PathBuf,
    fallback: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(root.path()).unwrap();
        let project = base.join("project");
        let fallback = base.join("fallback");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&fallback).unwrap();
        Self {
            _root: root,
            project,
            fallback,
        }
    }

    fn projects(&self) -> Vec<PathBuf> {
        vec![self.project.clone()]
    }
}

#[test]
fn check_failure_log_destination_outside_skips_native_guard_and_fallback() {
    let fixture = Fixture::new();
    let logs = FailureLogs::new(true);
    let path = logs
        .destination_for(
            &fixture.fallback,
            &fixture.projects(),
            Creation::File,
            |_, _| panic!("Outside TMPDIR must not use a new access check"),
            || panic!("Outside TMPDIR must not select a fallback"),
        )
        .unwrap();
    assert_eq!(path, None);
    assert!(logs.destination.get().is_none());
}

#[test]
fn check_failure_log_destination_disabled_skips_paths_guard_and_fallback() {
    let logs = FailureLogs::new(false);
    let path = logs
        .destination_for(
            Path::new("relative"),
            &[PathBuf::from("relative-project")],
            Creation::Directory,
            |_, _| panic!("Disabled policy must not check access"),
            || panic!("Disabled policy must not select a fallback"),
        )
        .unwrap();
    assert_eq!(path, None);
}

#[test]
fn check_failure_log_destination_native_denied_or_unknown_keeps_old_builder() {
    let fixture = Fixture::new();
    for creation in [Creation::File, Creation::Directory] {
        for error in [
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::Unsupported,
            io::ErrorKind::Other,
        ] {
            let logs = FailureLogs::new(true);
            let path = logs
                .destination_for(
                    &fixture.project,
                    &fixture.projects(),
                    creation,
                    |path, kind| {
                        assert_eq!(path, fixture.project.as_path());
                        assert_eq!(kind, creation);
                        Err(io::Error::from(error))
                    },
                    || panic!("Denied or unknown access must not inspect fallback"),
                )
                .unwrap();
            assert_eq!(path, None);
            assert!(logs.destination.get().is_none());
        }
    }
}

#[test]
fn check_failure_log_destination_access_is_rechecked_and_fallback_shared() {
    let fixture = Fixture::new();
    let logs = FailureLogs::new(true);
    let guards = Cell::new(0);
    let fallbacks = Cell::new(0);
    for (creation, allowed) in [
        (Creation::File, true),
        (Creation::Directory, false),
        (Creation::Directory, true),
    ] {
        let path = logs
            .destination_for(
                &fixture.project,
                &fixture.projects(),
                creation,
                |_, kind| {
                    guards.set(guards.get() + 1);
                    assert_eq!(kind, creation);
                    if allowed {
                        Ok(())
                    } else {
                        Err(io::ErrorKind::PermissionDenied.into())
                    }
                },
                || {
                    fallbacks.set(fallbacks.get() + 1);
                    Ok(fixture.fallback.clone())
                },
            )
            .unwrap();
        assert_eq!(path, allowed.then_some(fixture.fallback.as_path()));
    }
    assert_eq!(guards.get(), 3);
    assert_eq!(fallbacks.get(), 1);
}

#[test]
fn check_failure_log_destination_allowed_file_and_directory_use_external_builders() {
    let fixture = Fixture::new();
    let logs = FailureLogs::new(true);
    let file_path = logs
        .destination_for(
            &fixture.project,
            &fixture.projects(),
            Creation::File,
            |_, _| Ok(()),
            || Ok(fixture.fallback.clone()),
        )
        .unwrap();
    let file = create_file(file_path, "blk_", ".log").unwrap();
    assert_eq!(file.path().parent(), Some(fixture.fallback.as_path()));
    let dir_path = logs
        .destination_for(
            &fixture.project,
            &fixture.projects(),
            Creation::Directory,
            |_, _| Ok(()),
            || panic!("Reuse the same fallback"),
        )
        .unwrap();
    let directory = create_directory(dir_path, "mamba-quality-pylint-").unwrap();
    assert_eq!(directory.path().parent(), Some(fixture.fallback.as_path()));
    assert_eq!(fs::read_dir(&fixture.project).unwrap().count(), 0);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn check_failure_log_destination_native_access_accepts_caller_owned_0755() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    fs::set_permissions(&fixture.project, fs::Permissions::from_mode(0o755)).unwrap();
    for creation in [Creation::File, Creation::Directory] {
        super::check(&fixture.project, creation).expect("Native effective write and search access");
    }
}

#[test]
fn check_failure_log_destination_missing_original_skips_guard_and_fallback() {
    let fixture = Fixture::new();
    let logs = FailureLogs::new(true);
    let missing = fixture.project.join("missing");
    assert_eq!(
        logs.destination_for(
            &missing,
            &fixture.projects(),
            Creation::File,
            |_, _| panic!("Do not query access for a missing original directory"),
            || panic!("Do not use a fallback for a missing original directory"),
        )
        .unwrap(),
        None
    );
    assert!(!missing.exists());
}

#[cfg(target_os = "macos")]
struct DenyAcl {
    path: PathBuf,
}

#[cfg(target_os = "macos")]
impl DenyAcl {
    fn install(fixture: &Fixture, right: &str) -> Self {
        let path = fs::canonicalize(&fixture.project).unwrap();
        let root = fs::canonicalize(fixture._root.path()).unwrap();
        assert!(path.starts_with(&root) && path != root);
        let acl = Self { path };
        // The fixture is private. Clear inherited entries before adding one deny.
        acl.chmod(&["-N"]);
        acl.chmod(&["+a", &format!("everyone deny {right}")]);
        acl
    }

    fn chmod(&self, arguments: &[&str]) {
        let output = std::process::Command::new("/bin/chmod")
            .args(arguments)
            .arg(&self.path)
            .output()
            .expect("Run fixture-only ACL chmod");
        assert!(
            output.status.success(),
            "fixture ACL chmod failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(target_os = "macos")]
impl Drop for DenyAcl {
    fn drop(&mut self) {
        let output = std::process::Command::new("/bin/chmod")
            .arg("-N")
            .arg(&self.path)
            .output();
        let restored = output.as_ref().is_ok_and(|output| output.status.success());
        if std::thread::panicking() {
            if !restored {
                eprintln!("Could not clear private fixture ACL: {output:?}");
            }
        } else {
            assert!(restored, "Could not clear private fixture ACL: {output:?}");
        }
    }
}

#[cfg(target_os = "macos")]
fn assert_acl_creation(right: &str, file_allowed: bool, directory_allowed: bool) {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    fs::set_permissions(&fixture.project, fs::Permissions::from_mode(0o755)).unwrap();
    // Declared after the fixture so its ACL is removed before TempDir cleanup.
    let _acl = DenyAcl::install(&fixture, right);
    let file_query = super::check(&fixture.project, Creation::File);
    let directory_query = super::check(&fixture.project, Creation::Directory);
    let file_path = fixture.project.join("native-file");
    let directory_path = fixture.project.join("native-directory");
    let file_create = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file_path);
    let directory_create = fs::create_dir(&directory_path);
    // Collect both real operations before assertions to compare both ACL outcomes.
    fn assert_result<T: std::fmt::Debug>(result: io::Result<T>, allowed: bool, label: &str) {
        if allowed {
            result.unwrap_or_else(|error| panic!("{label} unexpectedly denied: {error}"));
        } else {
            let error = result.expect_err("ACL-denied operation must not succeed");
            assert_eq!(
                error.kind(),
                io::ErrorKind::PermissionDenied,
                "{label}: {error}"
            );
        }
    }
    assert_result(file_query, file_allowed, "file access query");
    assert_result(directory_query, directory_allowed, "directory access query");
    assert_result(file_create, file_allowed, "actual file create");
    assert_result(
        directory_create,
        directory_allowed,
        "actual directory create",
    );
}

#[cfg(target_os = "macos")]
#[test]
fn check_failure_log_destination_native_acl_denied_file_allows_directory() {
    assert_acl_creation("add_file", false, true);
}

#[cfg(target_os = "macos")]
#[test]
fn check_failure_log_destination_native_acl_denied_directory_allows_file() {
    assert_acl_creation("add_subdirectory", true, false);
}

#[cfg(target_os = "macos")]
#[test]
fn check_failure_log_destination_native_acl_search_denies_both_creates() {
    assert_acl_creation("search", false, false);
}
