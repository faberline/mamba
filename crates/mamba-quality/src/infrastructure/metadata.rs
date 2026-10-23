use crate::domain::{
    model::QualityIssue,
    source::{DependencySource, ProjectInfo, PythonVersion},
};
use pep440_rs::{Operator, Version, VersionSpecifiers};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn project_root(path: &Path) -> PathBuf {
    let start = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    for ancestor in start.ancestors() {
        if ancestor.join("pyproject.toml").is_file() || ancestor.join(".git").exists() {
            return ancestor.into();
        }
    }
    start.into()
}

pub fn target_version(requirement: Option<&str>) -> Result<PythonVersion, String> {
    let Some(requirement) = requirement else {
        return Ok(PythonVersion::Py312);
    };
    let specifiers: VersionSpecifiers = requirement
        .parse()
        .map_err(|error| format!("Invalid requires-python {requirement:?}: {error}"))?;
    let mut candidates = BTreeSet::new();
    let mut lower_bound = None;
    for specifier in specifiers.iter() {
        let version = specifier.version();
        if version.epoch() != 0 || version.release().len() > 3 {
            return Err(format!(
                "Unsupported Python version requirement: {requirement}"
            ));
        }
        let release = version.release();
        let major = release.first().copied().unwrap_or(0);
        let minor = release.get(1).copied().unwrap_or(0);
        let patch = release.get(2).copied().unwrap_or(0);
        candidates.insert(version.clone());
        for (major, minor, patch) in [
            (major, minor, patch),
            (major, minor, patch.saturating_add(1)),
            (major, minor.saturating_add(1), 0),
            (major.saturating_add(1), 0, 0),
        ] {
            candidates.insert(
                format!("{major}.{minor}.{patch}")
                    .parse::<Version>()
                    .unwrap(),
            );
        }
        if matches!(
            specifier.operator(),
            Operator::Equal
                | Operator::EqualStar
                | Operator::ExactEqual
                | Operator::TildeEqual
                | Operator::GreaterThan
                | Operator::GreaterThanEqual
        ) && lower_bound.as_ref().is_none_or(|bound| version > bound)
        {
            lower_bound = Some(version.clone());
        }
    }
    // An upper bound alone does not name a project minimum. Use the default,
    // but still apply every exclusion and upper bound to that default.
    let floor: Version = lower_bound.unwrap_or_else(|| "3.12.0".parse().unwrap());
    candidates.insert(floor.clone());
    for minor in 12..=15 {
        candidates.insert(format!("3.{minor}.0").parse().unwrap());
    }
    let minimum = candidates
        .iter()
        .find(|version| **version >= floor && specifiers.contains(version))
        .ok_or_else(|| format!("No supported minimum satisfies requires-python {requirement:?}"))?;
    match minimum.release() {
        [3, 12, ..] => Ok(PythonVersion::Py312),
        [3, 13, ..] => Ok(PythonVersion::Py313),
        [3, 14, ..] => Ok(PythonVersion::Py314),
        _ => Err(format!(
            "Python minimum {minimum} is unsupported; supported targets are 3.12, 3.13, and 3.14"
        )),
    }
}

fn installed_data(
    root: &Path,
    prefix: &str,
    target: PythonVersion,
) -> Result<Vec<DependencySource>, QualityIssue> {
    let environment = root.join(prefix);
    let cfg = environment.join("pyvenv.cfg");
    let contents = match fs::read_to_string(&cfg) {
        Ok(value) => Some(value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(QualityIssue::new(
                "package-metadata-read",
                error.to_string(),
                Some(cfg),
            ))
        }
    };
    let version = contents.as_deref().and_then(|contents| {
        contents.lines().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == "version").then_some(value.trim())
        })
    });
    let mut selected = None;
    if let Some(version) = version {
        let parts: Vec<_> = version.split('.').take(2).collect();
        if parts.len() != 2 || parts.iter().any(|part| part.parse::<u32>().is_err()) {
            return Err(QualityIssue::new(
                "package-python-version",
                format!("Invalid local package Python version {version:?}"),
                Some(cfg),
            ));
        }
        selected = Some(parts.join("."));
    }
    let mut locations = Vec::new();
    for lib in ["lib", "lib64"] {
        let directory = environment.join(lib);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(QualityIssue::new(
                    "package-directory-read",
                    error.to_string(),
                    Some(directory),
                ))
            }
        };
        for entry in entries {
            let entry = entry.map_err(|error| {
                QualityIssue::new(
                    "package-directory-read",
                    error.to_string(),
                    Some(directory.clone()),
                )
            })?;
            let name = entry.file_name();
            let Some(version) = name.to_str().and_then(|name| name.strip_prefix("python")) else {
                continue;
            };
            let parts: Vec<_> = version.split('.').collect();
            if parts.len() != 2 || parts.iter().any(|part| part.parse::<u32>().is_err()) {
                continue;
            }
            let path = entry.path().join("site-packages");
            if path.is_dir() {
                locations.push((version.to_string(), path));
            }
        }
    }
    locations.sort();
    if selected.is_none() {
        selected = if locations
            .iter()
            .any(|(version, _)| version == target.name())
        {
            Some(target.name().to_string())
        } else {
            let versions: BTreeSet<_> = locations.iter().map(|(version, _)| version).collect();
            if versions.len() > 1 {
                return Err(QualityIssue::new(
                    "ambiguous-package-data",
                    "Multiple local Python package directories lack pyvenv.cfg version data",
                    Some(environment),
                ));
            }
            locations.first().map(|(version, _)| version.clone())
        };
    }
    let mut sources = Vec::new();
    if let Some(contents) = contents {
        sources.push(DependencySource {
            kind: "environment-metadata".into(),
            path: Some(cfg),
            identity: "local pyvenv.cfg; package data is read without executing Python".into(),
            sha256: Some(sha256(contents.as_bytes())),
        });
    }
    let mut package_paths = BTreeSet::new();
    for (version, path) in locations {
        if selected.as_ref() == Some(&version) {
            let key = package_path(&path)?;
            if package_paths.insert(key) {
                sources.push(DependencySource {
                    kind: "installed-package-data".into(),
                    path: Some(path.clone()),
                    identity: format!(
                        "local Python {version} .py, .pyi, py.typed and package metadata"
                    ),
                    sha256: None,
                });
            }
            sources.extend(pth_data(&path, &mut package_paths)?);
        }
    }
    Ok(sources)
}

fn package_path_error(error: std::io::Error, path: &Path) -> QualityIssue {
    QualityIssue::new("package-path-read", error.to_string(), Some(path.into()))
}

fn package_path(path: &Path) -> Result<PathBuf, QualityIssue> {
    let path = std::path::absolute(path).map_err(|error| package_path_error(error, path))?;
    let mut normalized = PathBuf::new();
    for component in path.components() {
        if component == std::path::Component::ParentDir {
            normalized.pop();
        } else {
            normalized.push(component);
        }
    }
    Ok(normalized)
}

fn pth_data(
    site: &Path,
    seen: &mut BTreeSet<PathBuf>,
) -> Result<Vec<DependencySource>, QualityIssue> {
    let mut paths = fs::read_dir(site)
        .map_err(|error| package_path_error(error, site))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| package_path_error(error, site))?;
    paths.retain(|path| {
        path.extension().is_some_and(|value| value == "pth")
            && !path
                .file_name()
                .unwrap()
                .as_encoded_bytes()
                .starts_with(b".")
    });
    paths.sort();
    let mut sources = Vec::new();
    for path in paths {
        let bytes = fs::read(&path).map_err(|error| package_path_error(error, &path))?;
        let digest = sha256(&bytes);
        let observation = |message: String| DependencySource {
            kind: "unsupported-package-path".into(),
            path: Some(path.clone()),
            identity: message,
            sha256: Some(digest.clone()),
        };
        let content = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
        let text = match std::str::from_utf8(content) {
            Ok(text) => text,
            Err(_) => {
                sources.push(observation("non-UTF-8 .pth needs an unknown locale".into()));
                continue;
            }
        };
        let text = text.replace("\r\n", "\n");
        let separators = [
            '\n', '\r', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}',
        ];
        for (index, raw) in text.split(separators).enumerate() {
            let dynamic = raw.starts_with("import ") || raw.starts_with("import\t");
            let line = raw.trim_end_matches(|value: char| {
                value.is_whitespace() || matches!(value, '\x1c'..='\x1f')
            });
            if line.is_empty() || raw.starts_with('#') {
                continue;
            }
            let location = format!("{} line {}", path.display(), index + 1);
            if dynamic || line.contains('\0') {
                sources.push(observation(format!(
                    "{location}: dynamic or invalid .pth entry"
                )));
                continue;
            }
            let target = package_path(&site.join(line))?;
            let metadata = match fs::metadata(&target) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(package_path_error(error, &target)),
            };
            if !metadata.is_dir() {
                sources.push(observation(format!(
                    "{location}: non-directory .pth target"
                )));
            } else if seen.insert(target.clone()) {
                sources.push(DependencySource {
                    kind: "installed-package-data".into(),
                    path: Some(target),
                    identity: format!("static .pth path from {location}"),
                    sha256: Some(digest.clone()),
                });
            }
        }
    }
    Ok(sources)
}

pub fn project(root: &Path) -> Result<ProjectInfo, QualityIssue> {
    let path = root.join("pyproject.toml");
    let bytes = match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(QualityIssue::new(
                "project-read",
                error.to_string(),
                Some(path),
            ))
        }
    };
    let mut requires_python = None;
    if let Some(bytes) = &bytes {
        let contents = std::str::from_utf8(bytes).map_err(|error| {
            QualityIssue::new("project-metadata", error.to_string(), Some(path.clone()))
        })?;
        let value: toml::Value = toml::from_str(contents).map_err(|error| {
            QualityIssue::new("project-metadata", error.to_string(), Some(path.clone()))
        })?;
        if let Some(requirement) = value.get("project").and_then(|v| v.get("requires-python")) {
            requires_python = Some(
                requirement
                    .as_str()
                    .ok_or_else(|| {
                        QualityIssue::new(
                            "project-metadata",
                            "requires-python must be a string",
                            Some(path.clone()),
                        )
                    })?
                    .to_owned(),
            );
        }
        if value
            .get("project")
            .and_then(|v| v.get("dynamic"))
            .and_then(toml::Value::as_array)
            .is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item.as_str() == Some("requires-python"))
            })
        {
            return Err(QualityIssue::new(
                "dynamic-python-version",
                "requires-python is dynamic; static project data is required",
                Some(path),
            ));
        }
    }
    let python_version = target_version(requires_python.as_deref()).map_err(|message| {
        QualityIssue::new("unsupported-python-version", message, Some(path.clone()))
    })?;
    let mut dependencies = vec![DependencySource {
        kind: "project".into(),
        path: Some(root.into()),
        identity: "project sources and type stubs".into(),
        sha256: None,
    }];
    dependencies.push(super::embedded::source());
    dependencies.push(super::isort::sorting::source(python_version));
    if root.join("src").is_dir() {
        dependencies.push(DependencySource {
            kind: "project".into(),
            path: Some(root.join("src")),
            identity: "src layout".into(),
            sha256: None,
        });
    }
    for prefix in [".venv", "venv"] {
        dependencies.extend(installed_data(root, prefix, python_version)?);
    }
    Ok(ProjectInfo {
        root: root.into(),
        python_version,
        requires_python,
        metadata_sha256: bytes.as_deref().map(sha256),
        dependencies,
    })
}
