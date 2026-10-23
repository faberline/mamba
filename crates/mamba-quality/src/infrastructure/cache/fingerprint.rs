use crate::domain::{model::QualityTool, profile::FixedProfile, source::QualityWorkspace};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};

fn data_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|name| name.to_str()),
        Some("py" | "pyi" | "so" | "pyd" | "dylib" | "dll")
    ) || matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("py.typed" | "METADATA" | "PKG-INFO" | "RECORD" | "WHEEL" | "pyproject.toml")
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
                | "__pycache__"
                | ".tox"
                | ".nox"
                | ".mypy_cache"
                | ".ruff_cache"
                | ".cache"
                | ".pytest_cache"
                | "node_modules"
                | "build"
                | "dist"
                | "target"
                | ".mamba-quality-cache"
        )
    )
}

fn feed(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

pub(super) fn key(
    workspace: &QualityWorkspace,
    profile: &FixedProfile,
    tool: QualityTool,
    executable: &str,
) -> Option<String> {
    let mut hash = Sha256::new();
    feed(&mut hash, b"mamba.quality.cache.v3");
    feed(&mut hash, executable.as_bytes());
    feed(&mut hash, tool.name().as_bytes());
    feed(&mut hash, &serde_json::to_vec(profile).ok()?);
    feed(&mut hash, &serde_json::to_vec(&workspace.projects).ok()?);
    feed(
        &mut hash,
        &serde_json::to_vec(&workspace.explicit_files).ok()?,
    );
    feed(&mut hash, &serde_json::to_vec(&workspace.scan_roots).ok()?);
    if tool == QualityTool::Mypy {
        feed(
            &mut hash,
            &serde_json::to_vec(&workspace.source_requests).ok()?,
        );
    }
    for source in &workspace.sources {
        feed(&mut hash, source.path.to_string_lossy().as_bytes());
        feed(&mut hash, source.sha256.as_bytes());
        feed(&mut hash, &(source.project as u64).to_le_bytes());
    }
    let mut paths = BTreeSet::new();
    for project in &workspace.projects {
        for dependency in &project.dependencies {
            let Some(root) = &dependency.path else {
                continue;
            };
            let walker = ignore::WalkBuilder::new(root)
                .standard_filters(false)
                .follow_links(false)
                .filter_entry(|entry| {
                    entry.depth() == 0
                        || !entry.file_type().is_some_and(|kind| kind.is_dir())
                        || !excluded(entry.path())
                })
                .build();
            for entry in walker {
                let entry = entry.ok()?;
                // Symlinked dependencies need an explicit dependency snapshot.
                // Until that exists, bypass the cache for such projects.
                if entry.file_type().is_some_and(|kind| kind.is_symlink()) {
                    return None;
                }
                if entry.file_type().is_some_and(|kind| kind.is_dir())
                    || (entry.file_type().is_some_and(|kind| kind.is_file())
                        && data_file(entry.path()))
                {
                    paths.insert(entry.into_path());
                }
            }
        }
    }
    for path in paths {
        feed(&mut hash, path.to_string_lossy().as_bytes());
        if path.is_dir() {
            feed(&mut hash, b"directory");
            continue;
        }
        feed(&mut hash, b"file");
        if let Some(snapshot) = workspace.sources.iter().find(|source| source.path == path) {
            feed(&mut hash, snapshot.sha256.as_bytes());
        } else {
            feed(
                &mut hash,
                super::super::metadata::sha256(&fs::read(path).ok()?).as_bytes(),
            );
        }
    }
    Some(format!("{:x}", hash.finalize()))
}
