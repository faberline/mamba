//! Protected service tests for the frozen publication fault contract.
//!
//! Proposed declaration in `application/service/tests.rs`:
//! `#[cfg(unix)] mod publication;`

use super::*;
use crate::domain::{
    model::{CompletionStatus, QualityIssue, QualityOperation, QualityRequest, QualityTool},
    ports::{ProjectStore, PublishFailure},
    source::{QualityWorkspace, SourceSnapshot},
};
use crate::infrastructure::filesystem::PublishSyncPoint;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::os::unix::fs::MetadataExt;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    fs, io,
    path::PathBuf,
};

const ORIGINAL: &[u8] = b"VALUE=1\n";
const CANDIDATE: &[u8] = b"VALUE = 1\n";
const ORIGINAL_SHA256: &str = "ddced955472eca8cf0eea19edf798c43514e97feae458369b17732a4a6453e4b";
const CANDIDATE_SHA256: &str = "e13df8c44af5dea1e412403910b99cc5a48f2ccbf68a66b3374d6ab9cef9fc65";

#[derive(Clone, Copy, Debug)]
enum Fault {
    None,
    At(PublishSyncPoint),
}

#[derive(Debug)]
struct SyncObservation {
    point: PublishSyncPoint,
    source_bytes: Vec<u8>,
    source_inode: u64,
    file_is_file: bool,
    file_is_directory: bool,
    file_len: u64,
}

struct PublicationStore {
    filesystem: Filesystem,
    fault: Fault,
    publish_calls: Cell<usize>,
    sync_calls: RefCell<Vec<SyncObservation>>,
    loaded_source: RefCell<Option<PathBuf>>,
}

impl PublicationStore {
    fn new(fault: Fault) -> Self {
        Self {
            filesystem: Filesystem,
            fault,
            publish_calls: Cell::new(0),
            sync_calls: RefCell::new(Vec::new()),
            loaded_source: RefCell::new(None),
        }
    }
}

impl ProjectStore for PublicationStore {
    fn load(&self, paths: &[PathBuf], scan: bool) -> QualityWorkspace {
        let workspace = self.filesystem.load(paths, scan);
        *self.loaded_source.borrow_mut() =
            workspace.sources.first().map(|source| source.path.clone());
        workspace
    }

    fn candidate_bytes(
        &self,
        source: &SourceSnapshot,
        text: &str,
    ) -> Result<Vec<u8>, QualityIssue> {
        self.filesystem.candidate_bytes(source, text)
    }

    fn publish(&self, source: &SourceSnapshot, candidate: &[u8]) -> Result<(), PublishFailure> {
        self.publish_calls.set(self.publish_calls.get() + 1);
        let path = source.path.clone();
        let fault = self.fault;
        self.filesystem
            .publish_with_sync(source, candidate, |point, file| {
                let source_bytes = fs::read(&path)?;
                let source_inode = fs::metadata(&path)?.ino();
                let metadata = file.metadata()?;
                self.sync_calls.borrow_mut().push(SyncObservation {
                    point,
                    source_bytes,
                    source_inode,
                    file_is_file: metadata.is_file(),
                    file_is_directory: metadata.is_dir(),
                    file_len: metadata.len(),
                });
                let failure = match (fault, point) {
                    (
                        Fault::At(PublishSyncPoint::TemporaryFile),
                        PublishSyncPoint::TemporaryFile,
                    ) => Some("injected temporary sync failure"),
                    (
                        Fault::At(PublishSyncPoint::ParentDirectory),
                        PublishSyncPoint::ParentDirectory,
                    ) => Some("injected directory sync failure"),
                    _ => None,
                };
                match failure {
                    Some(message) => Err(io::Error::other(message)),
                    None => file.sync_all(),
                }
            })
    }
}

#[derive(Clone, Copy)]
enum Event {
    Normal,
    TemporarySyncError,
    DirectorySyncError,
}

impl Event {
    fn fault(self) -> Fault {
        match self {
            Self::Normal => Fault::None,
            Self::TemporarySyncError => Fault::At(PublishSyncPoint::TemporaryFile),
            Self::DirectorySyncError => Fault::At(PublishSyncPoint::ParentDirectory),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::TemporarySyncError => "temporary_sync_error",
            Self::DirectorySyncError => "directory_sync_error",
        }
    }

    fn message(self) -> Option<&'static str> {
        match self {
            Self::Normal => None,
            Self::TemporarySyncError => Some("injected temporary sync failure"),
            Self::DirectorySyncError => Some("injected directory sync failure"),
        }
    }

    fn applied(self) -> bool {
        !matches!(self, Self::TemporarySyncError)
    }

    fn expected_calls(self) -> &'static [&'static str] {
        match self {
            Self::TemporarySyncError => &["TemporaryFile"],
            Self::Normal | Self::DirectorySyncError => &["TemporaryFile", "ParentDirectory"],
        }
    }
}

fn point_name(point: PublishSyncPoint) -> &'static str {
    match point {
        PublishSyncPoint::TemporaryFile => "TemporaryFile",
        PublishSyncPoint::ParentDirectory => "ParentDirectory",
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn names(path: &std::path::Path) -> Vec<String> {
    let mut values: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    values.sort();
    values
}

fn keys(value: &Value) -> Vec<String> {
    let mut values: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
    values.sort();
    values
}

fn check(
    case: &str,
    field: &str,
    expected: Value,
    actual: Value,
    mismatches: &mut Vec<String>,
    assertions: &mut Vec<Value>,
) {
    let passed = expected == actual;
    if !passed {
        mismatches.push(format!("{case}:{field}"));
    }
    assertions.push(json!({
        "field": field,
        "passed": passed,
        "expected": expected,
        "actual": actual,
    }));
}

fn requested_tools(operation: QualityOperation) -> BTreeSet<QualityTool> {
    match operation {
        QualityOperation::Format { .. } => [QualityTool::Isort, QualityTool::Black].into(),
        QualityOperation::Fix { .. } | QualityOperation::Check => QualityTool::ALL.into(),
        QualityOperation::Info => BTreeSet::new(),
    }
}

fn expected_tools(operation: QualityOperation) -> Value {
    let requested = requested_tools(operation);
    Value::Array(
        QualityTool::ALL
            .into_iter()
            .map(|tool| {
                let yes = requested.contains(&tool);
                json!({
                    "tool": tool,
                    "reference_version": tool.reference_version(),
                    "completion": if yes { "complete" } else { "not_requested" },
                    "reference_exit_code": if yes { Some(0) } else { None },
                    "checked_files": if yes { 1 } else { 0 },
                    "diagnostics": [],
                    "remaining": [],
                })
            })
            .collect(),
    )
}

fn fixture(version: &str) -> (tempfile::TempDir, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.py");
    fs::write(&path, ORIGINAL).unwrap();
    fs::write(
        directory.path().join("pyproject.toml"),
        format!("[project]\nrequires-python = '>={version}'\n"),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o751)).unwrap();
    (directory, path)
}

fn mode(path: &std::path::Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    format!(
        "{:04o}",
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    )
}

fn operation_name(operation: QualityOperation) -> &'static str {
    match operation {
        QualityOperation::Format { .. } => "format",
        QualityOperation::Fix { .. } => "fix",
        QualityOperation::Check => "check",
        QualityOperation::Info => "info",
    }
}

fn write_evidence(rows: &[Value], mismatches: &[String]) {
    let Some(path) = std::env::var_os("MAMBA_QUALITY_PUBLICATION_EVIDENCE") else {
        return;
    };
    let path = PathBuf::from(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(&json!({
            "schema": "mamba.quality.publication-fault-tests.v1",
            "rows": rows,
            "mismatch_count": mismatches.len(),
            "mismatches": mismatches,
            "whole_suite_accepted": false,
        }))
        .unwrap(),
    )
    .unwrap();
}

fn record_service_cell(
    version: &str,
    operation: QualityOperation,
    event: Event,
    rows: &mut Vec<Value>,
    mismatches: &mut Vec<String>,
) {
    use std::os::unix::fs::MetadataExt;
    let case = format!("{version}/{} / {}", operation_name(operation), event.name());
    let (directory, input_path) = fixture(version);
    let original_inode = fs::metadata(&input_path).unwrap().ino();
    let store = PublicationStore::new(event.fault());
    let request = QualityRequest::new(operation, vec![input_path.clone()]);
    let report = execute(&request, &store, &Engine::new(None, false));
    let path = store.loaded_source.borrow().clone().unwrap();
    let report_json = serde_json::to_value(&report).unwrap();
    let applied = event.applied();
    let expected_exit = if event.message().is_some() { 2 } else { 0 };
    let expected_analysis = event.message().is_none();
    let expected_files = json!({
        "discovered": 1,
        "checked": 1,
        "changed": 1,
        "written": usize::from(applied),
    });
    let expected_changes = json!([{
        "path": path,
        "original_sha256": ORIGINAL_SHA256,
        "candidate_sha256": CANDIDATE_SHA256,
        "applied": applied,
        "diff": null,
        "tools": ["black"],
    }]);
    let expected_issues = match event.message() {
        Some(message) => json!([{
            "code": "source-write",
            "message": message,
            "path": path,
        }]),
        None => json!([]),
    };
    let expected_top_keys = json!([
        "analysis_completed",
        "changes",
        "exit_code",
        "files",
        "issues",
        "operation",
        "passed",
        "profile",
        "projects",
        "schema",
        "tools",
    ]);
    let expected_files_keys = json!(["changed", "checked", "discovered", "written"]);
    let expected_change_keys = json!([
        "applied",
        "candidate_sha256",
        "diff",
        "original_sha256",
        "path",
        "tools",
    ]);
    let expected_issue_keys = json!(["code", "message", "path"]);
    let actual_bytes = fs::read(&path).unwrap();
    let calls = store.sync_calls.borrow();
    let actual_points: Vec<_> = calls.iter().map(|call| point_name(call.point)).collect();
    let actual_callback_bytes: Vec<_> = calls.iter().map(|call| hex(&call.source_bytes)).collect();
    let expected_callback_bytes: Vec<_> = event
        .expected_calls()
        .iter()
        .map(|point| match *point {
            "TemporaryFile" => hex(ORIGINAL),
            "ParentDirectory" => hex(CANDIDATE),
            _ => unreachable!(),
        })
        .collect();
    let callback_checks: Vec<_> = calls
        .iter()
        .map(|call| match call.point {
            PublishSyncPoint::TemporaryFile => {
                call.source_bytes == ORIGINAL
                    && call.source_inode == original_inode
                    && call.file_is_file
                    && !call.file_is_directory
                    && call.file_len == CANDIDATE.len() as u64
            }
            PublishSyncPoint::ParentDirectory => {
                call.source_bytes == CANDIDATE
                    && call.source_inode != original_inode
                    && !call.file_is_file
                    && call.file_is_directory
            }
        })
        .collect();
    let actual_change = report_json["changes"].as_array().unwrap().first().cloned();
    let actual_issue = report_json["issues"].as_array().unwrap().first().cloned();
    let mut assertions = Vec::new();
    let mut field_checks = vec![
        (
            "top_level_keys",
            expected_top_keys,
            json!(keys(&report_json)),
        ),
        (
            "schema",
            json!("mamba.quality.v2"),
            report_json["schema"].clone(),
        ),
        (
            "operation",
            serde_json::to_value(operation).unwrap(),
            report_json["operation"].clone(),
        ),
        (
            "files_keys",
            expected_files_keys,
            json!(keys(&report_json["files"])),
        ),
        ("files", expected_files, report_json["files"].clone()),
        (
            "changes_len",
            json!(1),
            json!(report_json["changes"].as_array().unwrap().len()),
        ),
        (
            "change_keys",
            expected_change_keys,
            json!(actual_change.as_ref().map(keys).unwrap_or_default()),
        ),
        ("changes", expected_changes, report_json["changes"].clone()),
        ("issues", expected_issues, report_json["issues"].clone()),
    ];
    if event.message().is_some() {
        field_checks.push((
            "issue_keys",
            expected_issue_keys,
            json!(actual_issue.as_ref().map(keys).unwrap_or_default()),
        ));
    }
    field_checks.extend([
        (
            "tools",
            expected_tools(operation),
            report_json["tools"].clone(),
        ),
        (
            "analysis_completed",
            json!(expected_analysis),
            report_json["analysis_completed"].clone(),
        ),
        (
            "passed",
            json!(expected_analysis),
            report_json["passed"].clone(),
        ),
        (
            "exit_code",
            json!(expected_exit),
            report_json["exit_code"].clone(),
        ),
        ("publish_calls", json!(1), json!(store.publish_calls.get())),
        (
            "sync_calls",
            json!(event.expected_calls()),
            json!(actual_points),
        ),
        (
            "callback_source_bytes",
            json!(expected_callback_bytes),
            json!(actual_callback_bytes),
        ),
        (
            "callback_file_and_replacement_checks",
            json!(vec![true; calls.len()]),
            json!(callback_checks),
        ),
        (
            "source_bytes",
            json!(hex(if applied { CANDIDATE } else { ORIGINAL })),
            json!(hex(&actual_bytes)),
        ),
        ("source_mode", json!("0751"), json!(mode(&path))),
        (
            "directory_entries",
            json!(["main.py", "pyproject.toml"]),
            json!(names(directory.path())),
        ),
        (
            "source_hashes",
            json!({"original": ORIGINAL_SHA256, "candidate": CANDIDATE_SHA256}),
            json!({"original": sha256(ORIGINAL), "candidate": sha256(CANDIDATE)}),
        ),
    ]);
    for (field, expected, actual) in field_checks {
        check(&case, field, expected, actual, mismatches, &mut assertions);
    }
    rows.push(json!({
        "case": case,
        "python_target": version,
        "operation": operation,
        "event": event.name(),
        "expected": {
            "source_hex": hex(if applied { CANDIDATE } else { ORIGINAL }),
            "applied": applied,
            "written": usize::from(applied),
            "exit_code": expected_exit,
            "sync_calls": event.expected_calls(),
            "issue_message": event.message(),
        },
        "actual": {
            "source_path": path,
            "original_inode": original_inode,
            "source_hex": hex(&actual_bytes),
            "report": report_json,
            "publish_calls": store.publish_calls.get(),
            "sync_observations": calls.iter().map(|call| json!({
                "point": point_name(call.point),
                "source_hex": hex(&call.source_bytes),
                "source_inode": call.source_inode,
                "file_is_file": call.file_is_file,
                "file_is_directory": call.file_is_directory,
                "file_len": call.file_len,
            })).collect::<Vec<_>>(),
            "mode_octal": mode(&path),
            "directory_entries": names(directory.path()),
        },
        "assertions": assertions,
    }));
}

#[test]
fn publication_sync_failures_report_actual_replacement() {
    assert_eq!(sha256(ORIGINAL), ORIGINAL_SHA256);
    assert_eq!(sha256(CANDIDATE), CANDIDATE_SHA256);
    let mut rows = Vec::new();
    let mut mismatches = Vec::new();
    for version in ["3.12", "3.13", "3.14"] {
        for operation in [
            QualityOperation::Format {
                check: false,
                diff: false,
            },
            QualityOperation::Fix { diff: false },
        ] {
            for event in [
                Event::Normal,
                Event::TemporarySyncError,
                Event::DirectorySyncError,
            ] {
                record_service_cell(version, operation, event, &mut rows, &mut mismatches);
            }
        }
    }
    record_write_gates(&mut rows, &mut mismatches);
    if rows.len() != 26 {
        mismatches.push(format!("row_count:{}", rows.len()));
    }
    write_evidence(&rows, &mismatches);
    assert!(
        mismatches.is_empty(),
        "publication fault mismatches: {mismatches:#?}"
    );
}

fn record_write_gates(rows: &mut Vec<Value>, failures: &mut Vec<String>) {
    let gates = [
        (QualityOperation::Check, 0usize, 0usize, 0),
        (QualityOperation::Info, 0, 0, 0),
        (QualityOperation::Fix { diff: true }, 1, 0, 0),
        (
            QualityOperation::Format {
                check: false,
                diff: true,
            },
            1,
            0,
            0,
        ),
        (
            QualityOperation::Format {
                check: true,
                diff: false,
            },
            1,
            0,
            1,
        ),
    ];
    for (operation, changed, written, exit) in gates {
        let (directory, path) = fixture("3.12");
        let store = PublicationStore::new(Fault::At(PublishSyncPoint::ParentDirectory));
        let report = execute(
            &QualityRequest::new(operation, vec![path.clone()]),
            &store,
            &Engine::new(None, false),
        );
        let report_json = serde_json::to_value(&report).unwrap();
        let actual = json!({
            "publish_calls": store.publish_calls.get(),
            "sync_calls": store.sync_calls.borrow().iter().map(|call| point_name(call.point)).collect::<Vec<_>>(),
            "discovered": report.files.discovered,
            "checked": report.files.checked,
            "changed": report.files.changed,
            "written": report.files.written,
            "exit_code": report.exit_code,
            "applied": report.changes.iter().map(|change| change.applied).collect::<Vec<_>>(),
            "source_hex": hex(&fs::read(&path).unwrap()),
            "directory_entries": names(directory.path()),
        });
        let expected = json!({
            "publish_calls": 0,
            "sync_calls": [],
            "discovered": if operation == QualityOperation::Info { 0 } else { 1 },
            "checked": if operation == QualityOperation::Info { 0 } else { 1 },
            "changed": changed,
            "written": written,
            "exit_code": exit,
            "applied": vec![false; changed],
            "source_hex": hex(ORIGINAL),
            "directory_entries": ["main.py", "pyproject.toml"],
        });
        let case = format!("write-gate/{}", operation_name(operation));
        let mut assertions = Vec::new();
        check(
            &case,
            "report_and_filesystem",
            expected,
            actual.clone(),
            failures,
            &mut assertions,
        );
        if operation == QualityOperation::Info {
            check(
                &case,
                "info_tools_not_requested",
                json!(true),
                json!(report.tools.iter().all(|tool| {
                    tool.completion == CompletionStatus::NotRequested
                        && tool.reference_exit_code.is_none()
                })),
                failures,
                &mut assertions,
            );
        }
        rows.push(json!({
            "case": case,
            "operation": operation,
            "actual": {"report": report_json, "observed": actual},
            "assertions": assertions,
        }));
    }
    for status in [
        CompletionStatus::Error,
        CompletionStatus::Unsupported,
        CompletionStatus::Incomplete,
    ] {
        let (directory, path) = fixture("3.12");
        let store = PublicationStore::new(Fault::At(PublishSyncPoint::ParentDirectory));
        let report = execute(
            &QualityRequest::new(QualityOperation::Fix { diff: false }, vec![path.clone()]),
            &store,
            &Engine::new(Some(status), false),
        );
        let report_json = serde_json::to_value(&report).unwrap();
        let actual = json!({
            "publish_calls": store.publish_calls.get(),
            "sync_calls": store.sync_calls.borrow().iter().map(|call| point_name(call.point)).collect::<Vec<_>>(),
            "written": report.files.written,
            "changed": report.files.changed,
            "exit_code": report.exit_code,
            "applied": report.changes.iter().map(|change| change.applied).collect::<Vec<_>>(),
            "source_hex": hex(&fs::read(&path).unwrap()),
            "directory_entries": names(directory.path()),
            "black_completion": report.tools.iter().find(|tool| tool.tool == QualityTool::Black).unwrap().completion,
        });
        let expected = json!({
            "publish_calls": 0,
            "sync_calls": [],
            "written": 0,
            "changed": 1,
            "exit_code": 2,
            "applied": [false],
            "source_hex": hex(ORIGINAL),
            "directory_entries": ["main.py", "pyproject.toml"],
            "black_completion": status,
        });
        let case = format!("incomplete/{status:?}");
        let mut assertions = Vec::new();
        check(
            &case,
            "report_and_filesystem",
            expected,
            actual.clone(),
            failures,
            &mut assertions,
        );
        rows.push(json!({
            "case": case,
            "completion_status": status,
            "actual": {"report": report_json, "observed": actual},
            "assertions": assertions,
        }));
    }
}
