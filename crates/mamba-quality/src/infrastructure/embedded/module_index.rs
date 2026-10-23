//! Index immutable embedded files by their original paths.
use super::STDLIB;
use include_dir::{Dir, DirEntry, File};
use std::{collections::HashMap, path::Path, sync::OnceLock};

struct Index<'a> {
    files: HashMap<&'a Path, &'a File<'a>>,
}

impl<'a> Index<'a> {
    fn new(directory: &'a Dir<'a>) -> Self {
        fn collect<'a>(directory: &'a Dir<'a>, files: &mut HashMap<&'a Path, &'a File<'a>>) {
            for entry in directory.entries() {
                match entry {
                    DirEntry::Dir(nested) => collect(nested, files),
                    DirEntry::File(file) => {
                        files.insert(file.path(), file);
                    }
                }
            }
        }

        let mut files = HashMap::new();
        collect(directory, &mut files);
        Self { files }
    }

    fn file(&self, name: &str) -> Option<&'a File<'a>> {
        let path = name.replace('.', "/");
        self.files
            .get(Path::new(&format!("{path}.pyi")))
            .or_else(|| self.files.get(Path::new(&format!("{path}/__init__.pyi"))))
            .copied()
    }

    fn module(&self, name: &str) -> Option<&'a str> {
        self.file(name).and_then(File::contents_utf8)
    }
}

static INDEX: OnceLock<Index<'static>> = OnceLock::new();

pub(super) fn module(name: &str) -> Option<&'static str> {
    INDEX.get_or_init(|| Index::new(&STDLIB)).module(name)
}

pub(super) fn module_source(name: &str) -> Option<super::ModuleSource> {
    let file = INDEX.get_or_init(|| Index::new(&STDLIB)).file(name)?;
    Some(super::ModuleSource {
        text: file.contents_utf8()?,
        path: file.path(),
    })
}

#[cfg(test)]
mod tests;
