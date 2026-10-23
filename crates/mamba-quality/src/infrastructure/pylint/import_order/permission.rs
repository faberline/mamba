//! CPython 3.12 and 3.13 pathlib propagate isort source classification errors.
use super::super::Checker;
use mamba_language::application::quality_syntax::PythonTarget;
use std::{fs, io};

pub(super) fn failure(checker: &Checker<'_>, package: &str) -> Option<String> {
    if checker.modules.target() == PythonTarget::Py314 {
        return None;
    }
    let root = checker.source.path.parent()?;
    for directory in [root.join("src"), root.to_path_buf()] {
        let path = directory.join(package);
        match fs::metadata(&path) {
            Err(error)
                if !matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                return Some(format!("{}: {error}", path.display()));
            }
            _ => {}
        }
        if path.with_extension("py").is_file()
            || path.join("__init__.py").is_file()
            || path.is_dir()
        {
            return None;
        }
    }
    None
}
