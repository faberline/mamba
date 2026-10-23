//! Direct tests for the real filesystem publication sync seam.
//!
//! Proposed declaration in `infrastructure/filesystem/tests.rs`:
//! `#[cfg(unix)] mod publication;`

use super::super::{Filesystem, PublishSyncPoint};
use crate::domain::{ports::ProjectStore, source::SourceSnapshot};
use serde_json::{json, Value};
use std::{fs, io, path::PathBuf};
use tempfile::TempDir;

const ORIGINAL: &[u8] = b"VALUE=1\n";
const CANDIDATE: &[u8] = b"VALUE = 1\n";

#[derive(Debug)]
struct Observation {
    point: PublishSyncPoint,
    source: Vec<u8>,
    source_inode: u64,
    file_is_file: bool,
    file_is_directory: bool,
    file_len: Option<u64>,
}

fn fixture() -> (TempDir, PathBuf, SourceSnapshot) {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let input_path = directory.path().join("main.py");
    fs::write(&input_path, ORIGINAL).unwrap();
    fs::set_permissions(&input_path, fs::Permissions::from_mode(0o751)).unwrap();
    let workspace = Filesystem.load(std::slice::from_ref(&input_path), true);
    assert!(workspace.issues.is_empty(), "{:?}", workspace.issues);
    assert_eq!(workspace.sources.len(), 1);
    (
        directory,
        workspace.sources[0].path.clone(),
        workspace.sources[0].clone(),
    )
}

fn names(path: &std::path::Path) -> Vec<String> {
    let mut values: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    values.sort();
    values
}

fn mode(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn point_name(point: PublishSyncPoint) -> &'static str {
    match point {
        PublishSyncPoint::TemporaryFile => "TemporaryFile",
        PublishSyncPoint::ParentDirectory => "ParentDirectory",
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn record(
    case: &str,
    expected: Value,
    actual: Value,
    assertions: &mut Vec<Value>,
    mismatches: &mut Vec<String>,
) {
    let passed = expected == actual;
    if !passed {
        mismatches.push(case.to_string());
    }
    assertions.push(json!({
        "passed": passed,
        "expected": expected,
        "actual": actual,
    }));
}

fn write_evidence(case: &str, rows: &[Value], mismatches: &[String]) {
    let Some(path) = std::env::var_os("MAMBA_QUALITY_PUBLICATION_EVIDENCE") else {
        return;
    };
    // The service tests write the base path. This suffix keeps parallel test
    // processes from overwriting the direct filesystem observations.
    let path = PathBuf::from(format!(
        "{}.filesystem.{case}.json",
        PathBuf::from(path).display()
    ));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(&json!({
            "schema": "mamba.quality.publication-filesystem-tests.v1",
            "rows": rows,
            "mismatch_count": mismatches.len(),
            "mismatches": mismatches,
            "whole_suite_accepted": false,
        }))
        .unwrap(),
    )
    .unwrap();
}

fn result_fields(
    result: Result<(), crate::domain::ports::PublishFailure>,
) -> (Option<bool>, Option<Value>) {
    match result {
        Ok(()) => (Some(true), None),
        Err(failure) => (
            Some(failure.applied),
            Some(json!({
                "code": failure.issue.code,
                "message": failure.issue.message,
                "path": failure.issue.path,
            })),
        ),
    }
}

#[test]
fn pre_persist_sync_failure_keeps_original() {
    use std::os::unix::fs::MetadataExt;
    let (directory, path, source) = fixture();
    let original_inode = fs::metadata(&path).unwrap().ino();
    let mut calls = Vec::new();
    let result = Filesystem.publish_with_sync(&source, CANDIDATE, |point, file| {
        let metadata = file.metadata()?;
        calls.push(Observation {
            point,
            source: fs::read(&path)?,
            source_inode: fs::metadata(&path)?.ino(),
            file_is_file: metadata.is_file(),
            file_is_directory: metadata.is_dir(),
            file_len: metadata.is_file().then_some(metadata.len()),
        });
        if point == PublishSyncPoint::TemporaryFile {
            Err(io::Error::other("injected temporary sync failure"))
        } else {
            file.sync_all()
        }
    });
    let (applied, issue) = result_fields(result);
    let actual = json!({
        "applied": applied,
        "issue": issue,
        "calls": calls.iter().map(|call| json!({
            "point": point_name(call.point),
            "source_hex": hex(&call.source),
            "source_inode": call.source_inode,
            "file_is_file": call.file_is_file,
            "file_is_directory": call.file_is_directory,
            "file_len": call.file_len,
        })).collect::<Vec<_>>(),
        "source_hex": hex(&fs::read(&path).unwrap()),
        "source_inode": fs::metadata(&path).unwrap().ino(),
        "source_mode": mode(&path),
        "directory_entries": names(directory.path()),
    });
    let expected = json!({
        "applied": false,
        "issue": {
            "code": "source-write",
            "message": "injected temporary sync failure",
            "path": path,
        },
        "calls": [{
            "point": "TemporaryFile",
            "source_hex": hex(ORIGINAL),
            "source_inode": original_inode,
            "file_is_file": true,
            "file_is_directory": false,
        "file_len": Some(CANDIDATE.len()),
        }],
        "source_hex": hex(ORIGINAL),
        "source_inode": original_inode,
        "source_mode": 0o751,
        "directory_entries": ["main.py"],
    });
    let case = "pre_persist_sync_failure_keeps_original";
    let mut assertions = Vec::new();
    let mut mismatches = Vec::new();
    record(
        case,
        expected.clone(),
        actual.clone(),
        &mut assertions,
        &mut mismatches,
    );
    let rows = vec![
        json!({"case": case, "expected": expected, "actual": actual, "assertions": assertions}),
    ];
    write_evidence(case, &rows, &mismatches);
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

#[test]
fn post_persist_directory_sync_failure_keeps_candidate() {
    use std::os::unix::fs::MetadataExt;
    let (directory, path, source) = fixture();
    let original_inode = fs::metadata(&path).unwrap().ino();
    let mut calls = Vec::new();
    let result = Filesystem.publish_with_sync(&source, CANDIDATE, |point, file| {
        let metadata = file.metadata()?;
        calls.push(Observation {
            point,
            source: fs::read(&path)?,
            source_inode: fs::metadata(&path)?.ino(),
            file_is_file: metadata.is_file(),
            file_is_directory: metadata.is_dir(),
            file_len: metadata.is_file().then_some(metadata.len()),
        });
        if point == PublishSyncPoint::ParentDirectory {
            Err(io::Error::other("injected directory sync failure"))
        } else {
            file.sync_all()
        }
    });
    let (applied, issue) = result_fields(result);
    let actual = json!({
        "applied": applied,
        "issue": issue,
        "calls": calls.iter().map(|call| json!({
            "point": point_name(call.point),
            "source_hex": hex(&call.source),
            "source_inode": call.source_inode,
            "file_is_file": call.file_is_file,
            "file_is_directory": call.file_is_directory,
            "file_len": call.file_len,
        })).collect::<Vec<_>>(),
        "source_hex": hex(&fs::read(&path).unwrap()),
        "source_mode": mode(&path),
        "directory_entries": names(directory.path()),
    });
    let expected = json!({
        "applied": true,
        "issue": {
            "code": "source-write",
            "message": "injected directory sync failure",
            "path": path,
        },
        "calls": [
            {
                "point": "TemporaryFile",
                "source_hex": hex(ORIGINAL),
                "source_inode": original_inode,
                "file_is_file": true,
                "file_is_directory": false,
                "file_len": Some(CANDIDATE.len()),
            },
            {
                "point": "ParentDirectory",
                "source_hex": hex(CANDIDATE),
                "source_inode": Value::Null,
                "file_is_file": false,
                "file_is_directory": true,
                "file_len": Value::Null,
            },
        ],
        "source_hex": hex(CANDIDATE),
        "source_mode": 0o751,
        "directory_entries": ["main.py"],
    });
    let case = "post_persist_directory_sync_failure_keeps_candidate";
    let mut assertions = Vec::new();
    let mut mismatches = Vec::new();
    let overall = expected.clone();
    // A replacement must change the inode. Compare it separately so the raw
    // row keeps the real inode instead of hiding it behind a boolean.
    let observed_calls = calls
        .iter()
        .map(|call| {
            json!({
                "point": point_name(call.point),
                "source_hex": hex(&call.source),
                "source_inode": call.source_inode,
                "file_is_file": call.file_is_file,
                "file_is_directory": call.file_is_directory,
                "file_len": call.file_len,
            })
        })
        .collect::<Vec<_>>();
    let normalized_calls = calls
        .iter()
        .map(|call| {
            json!({
                "point": point_name(call.point),
                "source_hex": hex(&call.source),
                "file_is_file": call.file_is_file,
                "file_is_directory": call.file_is_directory,
                "file_len": call.file_len,
            })
        })
        .collect::<Vec<_>>();
    let inode_changed = calls
        .get(1)
        .is_some_and(|call| call.source_inode != original_inode);
    let mut actual_for_assertion = actual.clone();
    actual_for_assertion["calls"] = json!(normalized_calls);
    let mut expected_for_assertion = expected.clone();
    expected_for_assertion["calls"] = json!([
        {
            "point": "TemporaryFile",
            "source_hex": hex(ORIGINAL),
            "file_is_file": true,
            "file_is_directory": false,
            "file_len": Some(CANDIDATE.len()),
        },
        {
            "point": "ParentDirectory",
            "source_hex": hex(CANDIDATE),
            "file_is_file": false,
            "file_is_directory": true,
            "file_len": Value::Null,
        },
    ]);
    record(
        case,
        expected_for_assertion,
        actual_for_assertion,
        &mut assertions,
        &mut mismatches,
    );
    record(
        "post_persist_directory_sync_failure_changes_inode",
        json!(true),
        json!(inode_changed),
        &mut assertions,
        &mut mismatches,
    );
    let rows = vec![json!({
        "case": case,
        "expected": overall,
        "actual": {"state": actual, "observed_calls": observed_calls, "original_inode": original_inode},
        "assertions": assertions,
    })];
    write_evidence(case, &rows, &mismatches);
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

#[test]
fn normal_publish_keeps_candidate_mode_and_cleans_temp() {
    let (directory, path, source) = fixture();
    let result = Filesystem.publish(&source, CANDIDATE);
    let published = result.is_ok();
    let (applied, issue) = result_fields(result);
    let actual = json!({
        "published": published,
        "applied": applied,
        "issue": issue,
        "source_hex": hex(&fs::read(&path).unwrap()),
        "source_mode": mode(&path),
        "directory_entries": names(directory.path()),
    });
    let expected = json!({
        "published": true,
        "applied": true,
        "issue": null,
        "source_hex": hex(CANDIDATE),
        "source_mode": 0o751,
        "directory_entries": ["main.py"],
    });
    let case = "normal_publish_keeps_candidate_mode_and_cleans_temp";
    let mut assertions = Vec::new();
    let mut mismatches = Vec::new();
    record(
        case,
        expected.clone(),
        actual.clone(),
        &mut assertions,
        &mut mismatches,
    );
    let rows = vec![
        json!({"case": case, "expected": expected, "actual": actual, "assertions": assertions}),
    ];
    write_evidence(case, &rows, &mismatches);
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}
