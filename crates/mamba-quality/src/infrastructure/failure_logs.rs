//! Retained failure logs have a request-local, Check-only destination policy.
mod access;

use crate::domain::source::QualityWorkspace;
use std::{
    cell::OnceCell,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Creation {
    File,
    Directory,
}

pub(super) struct FailureLogs {
    check_only: bool,
    destination: OnceCell<io::Result<Option<PathBuf>>>,
}

impl FailureLogs {
    pub(super) const fn new(check_only: bool) -> Self {
        Self {
            check_only,
            destination: OnceCell::new(),
        }
    }

    pub(super) fn file(
        &self,
        workspace: &QualityWorkspace,
        prefix: &str,
        suffix: &str,
    ) -> io::Result<tempfile::NamedTempFile> {
        create_file(self.destination(workspace, Creation::File)?, prefix, suffix)
    }

    pub(super) fn directory(
        &self,
        workspace: &QualityWorkspace,
        prefix: &str,
    ) -> io::Result<tempfile::TempDir> {
        create_directory(self.destination(workspace, Creation::Directory)?, prefix)
    }

    fn destination(
        &self,
        workspace: &QualityWorkspace,
        creation: Creation,
    ) -> io::Result<Option<&Path>> {
        if !self.check_only {
            return Ok(None);
        }
        let projects: Vec<_> = workspace
            .projects
            .iter()
            .map(|project| project.root.clone())
            .collect();
        let temporary = std::env::temp_dir();
        let temporary = if temporary.is_absolute() {
            temporary
        } else {
            match std::env::current_dir() {
                Ok(directory) => directory.join(temporary),
                Err(_) => return Ok(None),
            }
        };
        self.destination_for(
            &temporary,
            &projects,
            creation,
            access::check,
            platform_temp,
        )
    }

    fn destination_for(
        &self,
        temporary: &Path,
        project_roots: &[PathBuf],
        creation: Creation,
        guard: impl FnOnce(&Path, Creation) -> io::Result<()>,
        fallback: impl FnOnce() -> io::Result<PathBuf>,
    ) -> io::Result<Option<&Path>> {
        if !self.check_only {
            return Ok(None);
        }
        let Some(projects) = original_project_directory(temporary, project_roots)? else {
            return Ok(None);
        };
        // A deny or an unsupported check preserves the exact old Builder call.
        // Do this before every real file/directory create, even with a cached base.
        if guard(temporary, creation).is_err() {
            return Ok(None);
        }
        self.destination_with(|| fallback_for(&projects, fallback).map(Some))
    }

    fn destination_with(
        &self,
        resolve: impl FnOnce() -> io::Result<Option<PathBuf>>,
    ) -> io::Result<Option<&Path>> {
        if !self.check_only {
            return Ok(None);
        }
        match self.destination.get_or_init(resolve) {
            Ok(path) => Ok(path.as_deref()),
            Err(error) => Err(match error.raw_os_error() {
                Some(code) => io::Error::from_raw_os_error(code),
                None => io::Error::new(error.kind(), error.to_string()),
            }),
        }
    }
}

fn create_file(
    base: Option<&Path>,
    prefix: &str,
    suffix: &str,
) -> io::Result<tempfile::NamedTempFile> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(prefix).suffix(suffix);
    match base {
        Some(path) => builder.tempfile_in(path),
        None => builder.tempfile(),
    }
}

fn create_directory(base: Option<&Path>, prefix: &str) -> io::Result<tempfile::TempDir> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(prefix);
    match base {
        Some(path) => builder.tempdir_in(path),
        None => builder.tempdir(),
    }
}

/// All injected paths are absolute. No missing suffix is inferred to be usable.
fn original_project_directory(
    temporary: &Path,
    project_roots: &[PathBuf],
) -> io::Result<Option<Vec<PathBuf>>> {
    if !temporary.is_absolute() || project_roots.iter().any(|path| !path.is_absolute()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Failure-log resolver inputs must be absolute paths",
        ));
    }
    let Ok(temporary) = existing_directory(temporary) else {
        return Ok(None);
    };
    let projects: Vec<_> = project_roots
        .iter()
        .map(|project| fs::canonicalize(project).unwrap_or_else(|_| project.clone()))
        .collect();
    if projects
        .iter()
        .any(|project| temporary.starts_with(project))
    {
        Ok(Some(projects))
    } else {
        Ok(None)
    }
}

fn fallback_for(
    projects: &[PathBuf],
    fallback: impl FnOnce() -> io::Result<PathBuf>,
) -> io::Result<PathBuf> {
    let fallback = fallback()?;
    if !fallback.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "The system temporary path must be absolute",
        ));
    }
    let path = existing_directory(&fallback)?;
    if projects.iter().any(|project| path.starts_with(project)) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "The system temporary directory is inside an analyzed project",
        ));
    }
    Ok(path)
}

fn existing_directory(path: &Path) -> io::Result<PathBuf> {
    let path = fs::canonicalize(path)?;
    if !fs::metadata(&path)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "The system temporary path is not a directory",
        ));
    }
    Ok(path)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn platform_temp() -> io::Result<PathBuf> {
    #[cfg(target_os = "macos")]
    let path = Path::new("/private/tmp");
    #[cfg(target_os = "linux")]
    let path = Path::new("/tmp");
    Ok(path.to_path_buf())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_temp() -> io::Result<PathBuf> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "No checked system temporary directory is defined for this platform",
    ))
}

#[cfg(test)]
mod tests;
