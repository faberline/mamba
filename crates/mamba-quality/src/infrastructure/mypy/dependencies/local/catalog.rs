//! A request-local directory listing proves absent names without a metadata probe.
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

const MAX_DIRECTORIES: usize = 4_096;
const MAX_NAMES: usize = 65_536;
const MAX_NAMES_PER_DIRECTORY: usize = 8_192;

pub(crate) struct Catalog {
    directories: HashMap<PathBuf, Option<HashSet<OsString>>>,
    names: usize,
    max_directories: usize,
    max_names: usize,
    avoided_probes: usize,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            directories: HashMap::new(),
            names: 0,
            max_directories: MAX_DIRECTORIES,
            max_names: MAX_NAMES,
            avoided_probes: 0,
        }
    }
}

impl Catalog {
    // A complete root listing can exclude every filesystem shape for a
    // top-level package. Any present shape, incomplete listing or limit keeps
    // the child lookup unknown; boolean metadata misses are never proof.
    pub(in super::super) fn absent_top_level(
        &mut self,
        root: &Path,
        top: &str,
        errors: &mut BTreeSet<String>,
    ) -> bool {
        if !self.directories.contains_key(root) {
            if self.directories.len() >= self.max_directories || self.names >= self.max_names {
                return false;
            }
            let limit = (self.max_names - self.names).min(MAX_NAMES_PER_DIRECTORY);
            let names = Self::absence_names(root, limit, errors);
            if let Some(names) = &names {
                self.names += names.len();
            }
            self.directories.insert(root.to_path_buf(), names);
        }
        self.directories
            .get(root)
            .and_then(Option::as_ref)
            .is_some_and(|names| {
                [
                    top.to_owned(),
                    format!("{top}-stubs"),
                    format!("{top}.pyi"),
                    format!("{top}.py"),
                ]
                .iter()
                .all(|name| !names.contains(std::ffi::OsStr::new(name)))
            })
    }

    fn absence_names(
        root: &Path,
        limit: usize,
        errors: &mut BTreeSet<String>,
    ) -> Option<HashSet<OsString>> {
        let entries = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) => {
                if !matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) {
                    errors.insert(format!("{}: {error}", root.display()));
                }
                return None;
            }
        };
        let mut names = HashSet::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    errors.insert(format!("{}: {error}", root.display()));
                    return None;
                }
            };
            names.insert(entry.file_name());
            if names.len() > limit {
                return None;
            }
        }
        Some(names)
    }

    /// A single lookup uses live probes without reading directory listings.
    #[cfg(test)]
    pub(super) fn uncached() -> Self {
        Self {
            max_directories: 0,
            max_names: 0,
            ..Self::default()
        }
    }

    pub(super) fn may_exist(&mut self, path: &Path) -> bool {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return true;
        };
        if !self.directories.contains_key(parent)
            && self.directories.len() < self.max_directories
            && self.names < self.max_names
        {
            let limit = (self.max_names - self.names).min(MAX_NAMES_PER_DIRECTORY);
            let names = Self::read_names(parent, limit);
            if let Some(names) = &names {
                self.names += names.len();
            }
            self.directories.insert(parent.to_path_buf(), names);
        }
        self.directories
            .get(parent)
            .and_then(Option::as_ref)
            .is_none_or(|names| names.contains(name))
    }

    pub(in super::super) fn is_file(&mut self, path: &Path, errors: &mut BTreeSet<String>) -> bool {
        if self.may_exist(path) {
            path.is_file() && self.has_exact_name(path, errors)
        } else {
            self.skip_probe();
            false
        }
    }

    pub(in super::super) fn is_dir(&mut self, path: &Path, errors: &mut BTreeSet<String>) -> bool {
        if self.may_exist(path) {
            path.is_dir() && self.has_exact_name(path, errors)
        } else {
            self.skip_probe();
            false
        }
    }

    /// Metadata can match a different case on some filesystems. Check the
    /// lexical entry name before canonicalizing, so exact symlink aliases stay valid.
    pub(in super::super) fn has_exact_name(
        &self,
        path: &Path,
        errors: &mut BTreeSet<String>,
    ) -> bool {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return true;
        };
        if let Some(Some(names)) = self.directories.get(parent) {
            return names.contains(name);
        }
        // A full or failed catalog is unknown, not an exact match. Stream the
        // positive candidate's parent without retaining another set of names.
        let entries = match fs::read_dir(parent) {
            Ok(entries) => entries,
            Err(error) => {
                errors.insert(format!("{}: {error}", parent.display()));
                return false;
            }
        };
        for entry in entries {
            match entry {
                Ok(entry) if entry.file_name().as_os_str() == name => return true,
                Ok(_) => {}
                Err(error) => {
                    errors.insert(format!("{}: {error}", parent.display()));
                    return false;
                }
            }
        }
        false
    }

    pub(super) fn skip_probe(&mut self) {
        self.avoided_probes += 1;
    }

    fn read_names(parent: &Path, limit: usize) -> Option<HashSet<OsString>> {
        let entries = fs::read_dir(parent).ok()?;
        let mut names = HashSet::new();
        for entry in entries {
            let entry = entry.ok()?;
            names.insert(entry.file_name());
            if names.len() > limit {
                return None;
            }
        }
        Some(names)
    }

    #[cfg(test)]
    pub(super) fn with_limits(max_directories: usize, max_names: usize) -> Self {
        Self {
            max_directories,
            max_names,
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub(super) fn avoided_probes(&self) -> usize {
        self.avoided_probes
    }
}

#[cfg(test)]
mod tests {
    use super::Catalog;
    use std::{collections::BTreeSet, fs};

    #[test]
    fn module_candidates_require_exact_directory_entry_names() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("session.py"),
            b"class Session:\n    pass\n",
        )
        .unwrap();
        let expected = temp.path().join("session.py");
        let wrong_case = temp.path().join("Session.py");
        let mut errors = BTreeSet::new();
        let mut cached = Catalog::default();

        assert!(cached.is_file(&expected, &mut errors));
        assert!(!cached.is_file(&wrong_case, &mut errors));
        assert!(!cached.is_dir(&temp.path().join("Session"), &mut errors));
        assert!(errors.is_empty());

        let mut uncached = Catalog::with_limits(0, 0);
        assert!(uncached.is_file(&expected, &mut errors));
        assert!(!uncached.is_file(&wrong_case, &mut errors));
        assert!(!uncached.is_dir(&temp.path().join("Session"), &mut errors));
        assert!(errors.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn exact_symlink_module_names_remain_valid() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("session.py");
        let alias = temp.path().join("SessionAlias.py");
        fs::write(&target, b"class Session:\n    pass\n").unwrap();
        symlink("session.py", &alias).unwrap();
        let mut errors = BTreeSet::new();
        let mut catalog = Catalog::default();

        assert!(catalog.is_file(&alias, &mut errors));
        assert_eq!(
            alias.canonicalize().unwrap(),
            target.canonicalize().unwrap()
        );
        assert!(!catalog.is_file(&temp.path().join("sessionalias.py"), &mut errors));
        assert!(errors.is_empty());
    }
}
