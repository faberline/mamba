//! Cache complete reports only. A failed cache operation is a cache miss.
mod fingerprint;
#[cfg(test)]
mod tests;

use crate::domain::{
    model::*,
    ports::QualityEngine,
    profile::FixedProfile,
    source::{QualityWorkspace, SourceSnapshot},
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    cell::OnceCell,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const CACHE_SCHEMA: &str = "mamba.quality.cache.v2";
const MAX_REPORT_BYTES: u64 = 64 * 1024 * 1024;

pub struct CachedEngine<E> {
    engine: E,
    executable: OnceCell<Option<String>>,
}

impl<E> CachedEngine<E> {
    pub fn new(engine: E) -> Self {
        Self {
            engine,
            executable: OnceCell::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    schema: String,
    key: String,
    report: ToolReport,
}

struct Entry {
    path: PathBuf,
    key: String,
    _writer: File,
}

impl Entry {
    fn claim(directory: &Path, key: &str) -> Option<Self> {
        // Check before mkdir: an existing ancestor symlink can redirect writes.
        for ancestor in directory
            .ancestors()
            .filter(|path| !path.as_os_str().is_empty())
        {
            match fs::symlink_metadata(ancestor) {
                Ok(metadata) if metadata.file_type().is_dir() => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => return None,
            }
        }
        fs::create_dir_all(directory).ok()?;
        // Do not follow a project-created cache-directory symlink.
        for path in [directory, directory.parent()?] {
            if !fs::symlink_metadata(path).ok()?.file_type().is_dir() {
                return None;
            }
        }
        let lock = directory.join(format!("{key}.lock"));
        if fs::symlink_metadata(&lock).is_ok_and(|metadata| !metadata.file_type().is_file()) {
            return None;
        }
        let writer = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock)
            .ok()?;
        writer.try_lock_exclusive().ok()?;
        Some(Self {
            path: directory.join(format!("{key}.json")),
            key: key.into(),
            _writer: writer,
        })
    }

    fn read(&self, tool: QualityTool, count: usize) -> Option<ToolReport> {
        let metadata = fs::symlink_metadata(&self.path).ok()?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_REPORT_BYTES {
            return None;
        }
        let mut data = Vec::new();
        File::open(&self.path)
            .ok()?
            .take(MAX_REPORT_BYTES + 1)
            .read_to_end(&mut data)
            .ok()?;
        if data.len() as u64 > MAX_REPORT_BYTES {
            return None;
        }
        let envelope: Envelope = serde_json::from_slice(&data).ok()?;
        let report = envelope.report;
        (envelope.schema == CACHE_SCHEMA
            && envelope.key == self.key
            && report.tool == tool
            && report.reference_version == tool.reference_version()
            && report.completion == CompletionStatus::Complete
            && report.checked_files == count
            && report.remaining.is_empty())
        .then_some(report)
    }

    fn write(&self, report: &ToolReport) -> Option<()> {
        if report.completion != CompletionStatus::Complete {
            return None;
        }
        let parent = self.path.parent()?;
        let data = serde_json::to_vec(&Envelope {
            schema: CACHE_SCHEMA.into(),
            key: self.key.clone(),
            report: report.clone(),
        })
        .ok()?;
        if data.len() as u64 > MAX_REPORT_BYTES {
            return None;
        }
        let mut temporary = tempfile::NamedTempFile::new_in(parent).ok()?;
        temporary.write_all(&data).ok()?;
        temporary.as_file().sync_all().ok()?;
        temporary.persist(&self.path).ok()?;
        Some(())
    }
}

impl<E: QualityEngine> QualityEngine for CachedEngine<E> {
    fn remaining(&self, tool: QualityTool) -> Vec<String> {
        self.engine.remaining(tool)
    }

    fn check(
        &self,
        workspace: &QualityWorkspace,
        profile: &FixedProfile,
        tool: QualityTool,
    ) -> ToolReport {
        // Partial engines cannot produce reusable acceptance results.
        let entry = if workspace.issues.is_empty() && self.engine.remaining(tool).is_empty() {
            self.executable
                .get_or_init(|| {
                    std::env::current_exe()
                        .ok()
                        .and_then(|path| fs::read(path).ok())
                        .map(|bytes| super::metadata::sha256(&bytes))
                })
                .as_deref()
                .and_then(|identity| fingerprint::key(workspace, profile, tool, identity))
                .and_then(|key| {
                    let directory = workspace
                        .projects
                        .first()?
                        .root
                        .join(".mamba-quality-cache")
                        .join("v2");
                    Entry::claim(&directory, &key)
                })
        } else {
            None
        };
        if let Some(cached) = entry
            .as_ref()
            .and_then(|entry| entry.read(tool, workspace.sources.len()))
        {
            return cached;
        }
        let report = self.engine.check(workspace, profile, tool);
        if let Some(entry) = entry {
            if self
                .executable
                .get()
                .and_then(Option::as_deref)
                .and_then(|identity| fingerprint::key(workspace, profile, tool, identity))
                .as_deref()
                == Some(&entry.key)
            {
                entry.write(&report);
            }
        }
        report
    }

    fn transform(
        &self,
        workspace: &QualityWorkspace,
        profile: &FixedProfile,
        tool: QualityTool,
    ) -> TransformResult {
        self.engine.transform(workspace, profile, tool)
    }

    fn validate(
        &self,
        original: &SourceSnapshot,
        candidate: &str,
        workspace: &QualityWorkspace,
        equivalent: bool,
    ) -> Result<(), QualityIssue> {
        self.engine
            .validate(original, candidate, workspace, equivalent)
    }
}
