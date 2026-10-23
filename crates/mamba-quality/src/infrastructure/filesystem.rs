use super::{encoding, metadata};
use crate::domain::{
    model::QualityIssue,
    ports::{ProjectStore, PublishFailure},
    source::{QualityWorkspace, RequestedSource, SourceRequest, SourceSnapshot},
};
use fs2::FileExt;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub struct Filesystem;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PublishSyncPoint {
    TemporaryFile,
    #[cfg(unix)]
    ParentDirectory,
}

#[cfg(test)]
mod tests;

fn issue(code: &str, path: &Path, error: impl std::fmt::Display) -> QualityIssue {
    QualityIssue::new(code, error.to_string(), Some(path.into()))
}

fn python_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("py" | "pyi")
    )
}

fn excluded(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(
            ".git"
                | ".hg"
                | ".svn"
                | ".venv"
                | "venv"
                | "env"
                | ".direnv"
                | ".tox"
                | ".nox"
                | "__pycache__"
                | ".mypy_cache"
                | ".ruff_cache"
                | ".pytest_cache"
                | ".cache"
                | "node_modules"
                | "target"
                | "build"
                | "dist"
                | "_build"
                | "site-packages"
                | "__pypackages__"
                | ".mamba-quality-cache"
        )
    )
}

fn add_project(
    workspace: &mut QualityWorkspace,
    roots: &mut BTreeMap<PathBuf, usize>,
    path: &Path,
) -> Result<usize, QualityIssue> {
    let root = metadata::project_root(path);
    if let Some(index) = roots.get(&root) {
        return Ok(*index);
    }
    let project = metadata::project(&root)?;
    let index = workspace.projects.len();
    workspace.projects.push(project);
    roots.insert(root, index);
    Ok(index)
}

fn requested_source(path: PathBuf, source_path: PathBuf) -> RequestedSource {
    let package_files = path
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .flat_map(|directory| {
            [
                directory.join("__init__.pyi"),
                directory.join("__init__.py"),
            ]
        })
        .filter(|path| path.is_file())
        .collect();
    RequestedSource {
        path,
        source_path,
        package_files,
    }
}

impl ProjectStore for Filesystem {
    fn load(&self, paths: &[PathBuf], scan: bool) -> QualityWorkspace {
        let mut workspace = QualityWorkspace::default();
        let paths = if paths.is_empty() {
            match std::env::current_dir() {
                Ok(path) => vec![path],
                Err(error) => {
                    workspace.issues.push(QualityIssue::new(
                        "current-directory",
                        error.to_string(),
                        None,
                    ));
                    return workspace;
                }
            }
        } else {
            paths.to_vec()
        };
        let mut sources = BTreeSet::new();
        let mut roots = BTreeMap::new();
        let current_directory = std::env::current_dir().ok();
        for input in paths {
            let path = match fs::canonicalize(&input) {
                Ok(path) => path,
                Err(error) => {
                    workspace.issues.push(issue("path-read", &input, error));
                    continue;
                }
            };
            if let Err(error) = add_project(&mut workspace, &mut roots, &path) {
                workspace.issues.push(error);
                continue;
            }
            if !scan {
                continue;
            }
            let requested_path = if input.is_absolute() {
                Some(input.clone())
            } else {
                current_directory
                    .as_ref()
                    .map(|directory| directory.join(&input))
            };
            if path.is_file() {
                if python_file(&path) {
                    if let Some(requested_path) = requested_path {
                        workspace.source_requests.push(SourceRequest {
                            path: requested_path.clone(),
                            canonical_path: path.clone(),
                            directory: false,
                            sources: vec![requested_source(requested_path, path.clone())],
                        });
                    }
                    workspace.explicit_files.push(path.clone());
                    sources.insert(path);
                } else {
                    workspace.issues.push(issue(
                        "unsupported-file",
                        &path,
                        "Expected a .py or .pyi file",
                    ));
                }
                continue;
            }
            if !path.is_dir() {
                workspace.issues.push(issue(
                    "unsupported-file",
                    &path,
                    "Expected a regular file or directory",
                ));
                continue;
            }
            workspace.scan_roots.push(path.clone());
            let mut request = requested_path.map(|requested_path| SourceRequest {
                path: requested_path,
                canonical_path: path.clone(),
                directory: true,
                sources: Vec::new(),
            });
            let walker = ignore::WalkBuilder::new(&path)
                .hidden(false)
                .ignore(false)
                .git_global(false)
                .git_exclude(true)
                .git_ignore(true)
                .require_git(false)
                .follow_links(false)
                .filter_entry(|entry| {
                    !entry.file_type().is_some_and(|kind| kind.is_dir()) || !excluded(entry.path())
                })
                .build();
            for entry in walker {
                match entry {
                    Ok(entry)
                        if entry.file_type().is_some_and(|kind| kind.is_file())
                            && python_file(entry.path()) =>
                    {
                        let source_path = entry.into_path();
                        if let Some(request) = &mut request {
                            if let Ok(relative) = source_path.strip_prefix(&path) {
                                request.sources.push(requested_source(
                                    request.path.join(relative),
                                    source_path.clone(),
                                ));
                            }
                        }
                        sources.insert(source_path);
                    }
                    Ok(_) => {}
                    Err(error) => workspace.issues.push(issue("discovery", &path, error)),
                }
            }
            if let Some(mut request) = request {
                request
                    .sources
                    .sort_by(|left, right| left.path.cmp(&right.path));
                workspace.source_requests.push(request);
            }
        }
        workspace.discovered = sources.len();
        for path in sources {
            let loaded = (|| {
                let project = add_project(&mut workspace, &mut roots, &path)?;
                let bytes = fs::read(&path).map_err(|error| issue("source-read", &path, error))?;
                let (text, encoding, decoding_issue) = match encoding::decode(&path, &bytes) {
                    Ok((text, encoding)) => (text, encoding, None),
                    Err(error) => {
                        workspace.issues.push(error.clone());
                        (
                            String::new(),
                            crate::domain::source::SourceEncoding::Utf8 { bom: false },
                            Some(error),
                        )
                    }
                };
                if text.len() > u32::MAX as usize {
                    return Err(issue(
                        "source-size",
                        &path,
                        "Source exceeds the supported byte-offset range",
                    ));
                }
                Ok(SourceSnapshot {
                    is_stub: path.extension().is_some_and(|extension| extension == "pyi"),
                    path: path.clone(),
                    project,
                    sha256: metadata::sha256(&bytes),
                    bytes,
                    text,
                    encoding,
                    decoding_issue,
                })
            })();
            match loaded {
                Ok(source) => workspace.sources.push(source),
                Err(error) => workspace.issues.push(error),
            }
        }
        workspace
    }

    fn candidate_bytes(
        &self,
        source: &SourceSnapshot,
        text: &str,
    ) -> Result<Vec<u8>, QualityIssue> {
        encoding::encode(&source.path, text, &source.encoding)
    }

    fn publish(&self, source: &SourceSnapshot, candidate: &[u8]) -> Result<(), PublishFailure> {
        self.publish_with_sync(source, candidate, |_, file| file.sync_all())
    }
}

impl Filesystem {
    pub(crate) fn publish_with_sync(
        &self,
        source: &SourceSnapshot,
        candidate: &[u8],
        mut sync: impl FnMut(PublishSyncPoint, &fs::File) -> std::io::Result<()>,
    ) -> Result<(), PublishFailure> {
        let path = &source.path;
        let mut original =
            fs::File::open(path).map_err(|error| issue("source-read", path, error))?;
        original
            .try_lock_exclusive()
            .map_err(|error| issue("source-busy", path, error))?;
        let mut current = Vec::new();
        original
            .read_to_end(&mut current)
            .map_err(|error| issue("source-read", path, error))?;
        if metadata::sha256(&current) != source.sha256 {
            return Err(issue(
                "source-changed",
                path,
                "Source changed after the analysis snapshot",
            )
            .into());
        }
        let permissions = original
            .metadata()
            .map_err(|error| issue("source-read", path, error))?
            .permissions();
        if permissions.readonly() {
            return Err(issue("source-readonly", path, "Source is read-only").into());
        }
        let parent = path
            .parent()
            .ok_or_else(|| issue("source-write", path, "Source has no parent directory"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|error| issue("source-write", path, error))?;
        temporary
            .write_all(candidate)
            .map_err(|error| issue("source-write", path, error))?;
        temporary
            .as_file()
            .set_permissions(permissions)
            .map_err(|error| issue("source-write", path, error))?;
        sync(PublishSyncPoint::TemporaryFile, temporary.as_file())
            .map_err(|error| issue("source-write", path, error))?;
        // Check the path again, not only the locked inode. An external writer can
        // replace that inode while the original descriptor still holds its lock.
        let latest = fs::read(path).map_err(|error| issue("source-read", path, error))?;
        if metadata::sha256(&latest) != source.sha256 {
            return Err(issue(
                "source-changed",
                path,
                "Source changed before atomic replacement",
            )
            .into());
        }
        temporary
            .persist(path)
            .map_err(|error| issue("source-write", path, error.error))?;
        #[cfg(unix)]
        fs::File::open(parent)
            .and_then(|directory| sync(PublishSyncPoint::ParentDirectory, &directory))
            .map_err(|error| PublishFailure {
                issue: issue("source-write", path, error),
                applied: true,
            })?;
        Ok(())
    }
}
