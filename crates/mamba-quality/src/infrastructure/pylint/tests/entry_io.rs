//! Original I/O comparisons use private paths and always restore permissions.
#![cfg(unix)]
use crate::domain::{model::CompletionStatus, ports::ProjectStore};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

struct Restricted {
    path: PathBuf,
    permissions: fs::Permissions,
}

impl Restricted {
    fn new(path: PathBuf) -> Self {
        let permissions = fs::metadata(&path).unwrap().permissions();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).unwrap();
        Self { path, permissions }
    }
}

impl Drop for Restricted {
    fn drop(&mut self) {
        fs::set_permissions(&self.path, self.permissions.clone()).unwrap();
    }
}

#[test]
fn pinned_entry_io_lists() {
    let records: Vec<Value> = serde_json::from_str(include_str!("entry_io_data.json")).unwrap();
    assert_eq!(records.len(), 27);
    for record in records {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        fs::write(
            root_path.join("pyproject.toml"),
            format!(
                "[project]\nrequires-python = '>= {}'\n",
                record["python"].as_str().unwrap()
            ),
        )
        .unwrap();
        for (name, source) in record["files"].as_object().unwrap() {
            let path = root_path.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, source["text"].as_str().unwrap()).unwrap();
        }
        let name = record["name"].as_str().unwrap();
        let input = record["inputs"][0].as_str().unwrap();
        let path = root_path.join(if name == "unreadable_last" {
            "unreadable.py"
        } else {
            input
        });
        let restricted = match name {
            "unreadable_file" | "unreadable_first" | "unreadable_last" | "quoted_path"
            | "backslash_path" | "unicode_path" => Some(Restricted::new(path.clone())),
            "unsearchable_parent" => Some(Restricted::new(path.parent().unwrap().into())),
            "broken_symlink" => {
                std::os::unix::fs::symlink("absent.py", &path).unwrap();
                None
            }
            "empty_directory" => {
                fs::create_dir(&path).unwrap();
                None
            }
            _ => panic!("Unexpected entry fixture: {name}"),
        };
        let inputs: Vec<PathBuf> = record["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|input| root_path.join(input.as_str().unwrap()))
            .collect();
        let workspace = Filesystem.load(&inputs, true);
        if matches!(name, "unsearchable_parent" | "broken_symlink") {
            assert_eq!(workspace.issues.len(), 1);
            assert_eq!(workspace.issues[0].code, "path-read");
        }
        if name == "unreadable_file" {
            assert_eq!(workspace.issues.len(), 1);
            assert_eq!(workspace.issues[0].code, "source-read");
        }
        let report = super::super::check(&workspace);
        let actual: Vec<_> = report.diagnostics.iter().map(|diagnostic| {
            let message = diagnostic.message.replace(root_path.to_str().unwrap(), "<project>");
            let path = diagnostic.path.strip_prefix(&root_path).unwrap_or(&diagnostic.path);
            assert!(diagnostic.fix.is_none() && diagnostic.hint.is_none());
            if diagnostic.severity == "fatal" {
                assert!(diagnostic.location.offset.is_none() && diagnostic.location.end_offset.is_none());
            }
            json!({"rule":diagnostic.rule,"message":message,"path":path,"severity":diagnostic.severity,
                "line":diagnostic.location.line,"column":diagnostic.location.column,
                "end_line":diagnostic.location.end_line,"end_column":diagnostic.location.end_column})
        }).collect();
        assert_eq!(
            Value::Array(actual),
            record["expected"],
            "{name} {}",
            record["python"]
        );
        assert_eq!(
            report.checked_files,
            usize::from(matches!(name, "unreadable_first" | "unreadable_last"))
        );
        assert_eq!(report.reference_exit_code, None);
        let exit = report.diagnostics.iter().fold(0, |exit, diagnostic| {
            exit | match diagnostic.severity.as_str() {
                "fatal" => 1,
                "error" => 2,
                _ => panic!("Unexpected severity"),
            }
        });
        assert_eq!(record["reference_exit"], exit);
        assert_eq!(
            report.completion,
            if exit == 0 {
                CompletionStatus::Incomplete
            } else {
                CompletionStatus::Error
            }
        );
        drop(restricted);
        for (name, source) in record["files"].as_object().unwrap() {
            assert_eq!(
                fs::read_to_string(root_path.join(name)).unwrap(),
                source["text"]
            );
        }
    }
}
