//! Namespace packages climb only to an actual __init__.py[i] anchor.
use crate::domain::model::{QualityDiagnostic, QualityLocation, QualityTool};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

pub(super) fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn identifier(name: &str) -> bool {
    static IDENTIFIER: OnceLock<regex::Regex> = OnceLock::new();
    IDENTIFIER
        .get_or_init(|| {
            regex::Regex::new(r"^[_\p{XID_Start}][\p{XID_Continue}]*$")
                .expect("fixed identifier regex")
        })
        .is_match(name)
}

pub(super) fn module(
    path: &Path,
    package_files: &[PathBuf],
) -> Result<String, Box<QualityDiagnostic>> {
    let packages: BTreeSet<_> = package_files
        .iter()
        .filter_map(|p| normalize(p).parent().map(Path::to_path_buf))
        .collect();
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let mut parts = Vec::new();
    let mut anchored = 0;
    for directory in parent.ancestors() {
        let name = directory.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let name = name.strip_suffix("-stubs").unwrap_or(name);
        if !identifier(name) {
            if packages.contains(directory) {
                let init = package_files
                    .iter()
                    .find(|p| normalize(p).parent() == Some(directory))
                    .unwrap();
                return Err(Box::new(QualityDiagnostic {
                    tool: QualityTool::Mypy,
                    rule: String::new(),
                    message: format!(
                        "{name} contains {} but is not a valid Python package name",
                        init.file_name().unwrap().to_string_lossy()
                    ),
                    hint: None,
                    path: path.into(),
                    location: QualityLocation::unpositioned(),
                    severity: "error".into(),
                    fix: None,
                }));
            }
            break;
        }
        parts.push(name.to_owned());
        if packages.contains(directory) {
            anchored = parts.len();
        }
    }
    parts.truncate(anchored);
    parts.reverse();
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    if stem != "__init__" {
        parts.push(stem.into_owned());
    }
    Ok(parts.join("."))
}
