//! Frozen original mypy lexical core-shadow symlink controls.
//!
//! Each test is one case/target cell. Set
//! MAMBA_QUALITY_MYPY_SHADOW_ALIAS_NATIVE_EVIDENCE_DIR to retain one full
//! ToolReport and source record per cell; assertions also run without it.
use crate::domain::{
    model::{CompletionStatus, QualityLocation, QualityTool},
    ports::ProjectStore,
    source::PythonVersion,
};
use crate::infrastructure::filesystem::Filesystem;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const FIXTURE: &str = include_str!("shadow_aliases.json");
const FIXTURE_SHA256: &str = "1ff72ea5736feddbbc11ffac8036007d700cafd53ac02e0eed1c9cac0b87d3ee";
const ORIGINAL_MANIFEST_SHA256: &str =
    "5ec836c4c742ab8a6a75453f930f3b7eafa21d6db814a8b204cede4e18b0dda2";
const PLAN_FIXTURES_SHA256: &str =
    "5fc9ff32d9cc80ce378b48f860acf7f3730eeddde0a8322cc4d242ba6e5a0da0";

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn corpus() -> Value {
    assert_eq!(sha256(FIXTURE.as_bytes()), FIXTURE_SHA256);
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        value["source_evidence"]["original_manifest_sha256"],
        ORIGINAL_MANIFEST_SHA256
    );
    assert_eq!(
        value["source_evidence"]["plan_fixtures_sha256"],
        PLAN_FIXTURES_SHA256
    );
    assert_eq!(value["case_count"], 5);
    assert_eq!(value["cell_count"], 15);
    assert_eq!(value["original_process_call_count"], 60);
    assert_eq!(value["source_evidence"]["native_comparison"], "unrun");
    assert_eq!(
        value["source_evidence"]["controller_runtime_314_historical_exact"],
        false
    );
    assert_eq!(value["source_evidence"]["whole_som_accepted"], false);
    assert_eq!(value["source_evidence"]["whole_plan_accepted"], false);
    value
}

fn case<'a>(corpus: &'a Value, name: &str) -> &'a Value {
    corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap_or_else(|| panic!("missing frozen case {name}"))
}

fn bytes_from_hex(value: &Value) -> Vec<u8> {
    let text = value.as_str().expect("expected hex bytes");
    assert!(text.len().is_multiple_of(2));
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn verify_original_runs(target: &Value, reporter: &str, expected_exit: i32) {
    let runs = target["reporters"][reporter]["runs"]
        .as_array()
        .expect("frozen reporter runs");
    assert_eq!(
        runs.len(),
        2,
        "each reporter must retain two original calls"
    );
    assert_eq!(target["reporters"][reporter]["repeat_bytes_stable"], true);
    let mut previous: Option<(i32, Vec<u8>, Vec<u8>)> = None;
    for run in runs {
        assert_eq!(run["timed_out"], false);
        assert_eq!(run["source_preserved"], true);
        let exit = run["actual_process_exit"].as_i64().unwrap() as i32;
        let stdout = bytes_from_hex(&run["stdout_hex"]);
        let stderr = bytes_from_hex(&run["stderr_hex"]);
        assert_eq!(sha256(&stdout), run["stdout_sha256"].as_str().unwrap());
        assert_eq!(sha256(&stderr), run["stderr_sha256"].as_str().unwrap());
        assert_eq!(exit, expected_exit);
        let current = (exit, stdout, stderr);
        if let Some(prior) = &previous {
            assert_eq!(prior, &current);
        }
        previous = Some(current);
    }
}

fn materialize(case: &Value, root: &Path) {
    for (relative, entry) in case["entries"].as_object().unwrap() {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        match entry["kind"].as_str().unwrap() {
            "regular" => {
                let bytes = bytes_from_hex(&entry["bytes_hex"]);
                assert_eq!(sha256(&bytes), entry["sha256"].as_str().unwrap());
                assert_eq!(bytes.len(), entry["bytes"].as_u64().unwrap() as usize);
                fs::write(&path, &bytes).unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(
                        &path,
                        fs::Permissions::from_mode(entry["mode"].as_u64().unwrap() as u32),
                    )
                    .unwrap();
                }
            }
            "symlink" => {
                let target = entry["link_target"].as_str().unwrap();
                assert_eq!(sha256(target.as_bytes()), entry["sha256"].as_str().unwrap());
                #[cfg(unix)]
                std::os::unix::fs::symlink(target, &path).unwrap();
                #[cfg(not(unix))]
                panic!("the frozen symlink control is Unix-only");
            }
            other => panic!("unsupported frozen entry kind {other}"),
        }
    }
}

#[cfg(unix)]
fn mode(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::PermissionsExt;
    u64::from(metadata.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn mode(metadata: &fs::Metadata) -> u64 {
    u64::from(metadata.permissions().readonly())
}

fn entry_state(path: &Path) -> Value {
    let link_metadata = fs::symlink_metadata(path).unwrap();
    let file_type = link_metadata.file_type();
    if file_type.is_symlink() {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let target = fs::read_link(path).unwrap();
            let target_bytes = target.as_os_str().as_bytes();
            let target_metadata = fs::metadata(path).ok();
            return json!({
                "kind":"symlink", "link_target":target.to_string_lossy(),
                "sha256":sha256(target_bytes), "bytes":target_bytes.len(),
                "mode":target_metadata.as_ref().map(mode), "link_mode":mode(&link_metadata),
            });
        }
        #[cfg(not(unix))]
        panic!("the frozen symlink control is Unix-only");
    }
    if file_type.is_dir() {
        return json!({"kind":"directory", "sha256":Value::Null, "bytes":Value::Null,
            "mode":mode(&link_metadata)});
    }
    assert!(
        file_type.is_file(),
        "unexpected filesystem object at {}",
        path.display()
    );
    let bytes = fs::read(path).unwrap();
    json!({"kind":"regular", "sha256":sha256(&bytes), "bytes":bytes.len(),
        "mode":mode(&link_metadata)})
}

fn source_state(root: &Path) -> BTreeMap<String, Value> {
    let mut state = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for item in fs::read_dir(&directory).unwrap() {
            let path = item.unwrap().path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string();
            let value = entry_state(&path);
            let is_directory = value["kind"] == "directory";
            state.insert(relative, value);
            if is_directory {
                pending.push(path);
            }
        }
    }
    state
}

fn expected_paths(case: &Value) -> BTreeMap<String, ()> {
    let mut paths = BTreeMap::new();
    for relative in case["entries"].as_object().unwrap().keys() {
        let mut parent = Path::new(relative).parent();
        while let Some(path) = parent {
            if path.as_os_str().is_empty() || path == Path::new(".") {
                break;
            }
            paths.insert(path.to_string_lossy().to_string(), ());
            parent = path.parent();
        }
        paths.insert(relative.clone(), ());
    }
    paths
}

fn frozen_entries_match(case: &Value, state: &BTreeMap<String, Value>) -> bool {
    case["entries"]
        .as_object()
        .unwrap()
        .iter()
        .all(|(relative, expected)| {
            let Some(actual) = state.get(relative) else {
                return false;
            };
            if actual["kind"] != expected["kind"]
                || actual["sha256"] != expected["sha256"]
                || actual["bytes"] != expected["bytes"]
                || (if expected["kind"] == "symlink" {
                    &actual["link_mode"]
                } else {
                    &actual["mode"]
                }) != &expected["mode"]
            {
                return false;
            }
            expected["kind"] != "symlink" || actual["link_target"] == expected["link_target"]
        })
}

fn requested_path(root: &Path, input: &str) -> PathBuf {
    // Keep lexical ./ in the request. Filesystem::load applies canonicalization.
    let separator = if root.to_string_lossy().ends_with('/') {
        ""
    } else {
        "/"
    };
    PathBuf::from(format!("{}{separator}{input}", root.to_string_lossy()))
}

fn original_location(diagnostic: &Value) -> Value {
    json!({"line": diagnostic["line"], "column": diagnostic["column"],
        "end_line": diagnostic["end_line"], "end_column": diagnostic["end_column"]})
}

fn compared_location(location: &QualityLocation) -> Value {
    let pos = |v: u32| {
        if v == QualityLocation::UNPOSITIONED_LINE {
            -1_i64
        } else {
            i64::from(v)
        }
    };
    let line = pos(location.line);
    let column = if location.line == QualityLocation::UNPOSITIONED_LINE {
        -1_i64
    } else {
        i64::from(location.column)
    };
    json!({"line": line, "column": column,
        "end_line": location.end_line.map(pos), "end_column": location.end_column})
}

fn expected_diagnostics(
    target: &Value,
    original_root: &str,
    native_root: &Path,
    entries: &Value,
) -> (Value, Vec<Value>) {
    let mut bindings = Vec::new();
    let diagnostics = target["reference_json_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            let message = d["message"].as_str().unwrap();
            let original_path = Path::new(d["file"].as_str().unwrap());
            let relative = if original_path.is_absolute() {
                original_path
                    .strip_prefix(original_root)
                    .expect("original diagnostic under frozen root")
            } else {
                original_path
            };
            let path = relative.to_string_lossy().to_string();
            assert!(
                entries.get(&path).is_some(),
                "diagnostic path must be a frozen entry"
            );
            bindings.push(
                json!({"original_diagnostic":d, "original_root":original_root,
                "native_root":native_root, "workspace_relative_path":path,
                "message_changed":false}),
            );
            json!({
                "rule":d["code"], "message":message, "hint":d["hint"], "path":path,
                "severity":d["severity"], "location":original_location(d),
            })
        })
        .collect();
    (Value::Array(diagnostics), bindings)
}

fn actual_diagnostics(report: &crate::domain::model::ToolReport, root: &Path) -> Value {
    Value::Array(report.diagnostics.iter().map(|d| {
        let serialized = serde_json::to_value(d).unwrap();
        let path = d.path.strip_prefix(root).unwrap_or(&d.path);
        json!({"rule": serialized["rule"], "message": d.message, "hint": d.hint,
            "path": path.to_string_lossy(), "severity": d.severity, "location": compared_location(&d.location)})
    }).collect())
}

fn write_evidence(case_name: &str, target: &str, evidence: &Value) {
    let Some(directory) = std::env::var_os("MAMBA_QUALITY_MYPY_SHADOW_ALIAS_NATIVE_EVIDENCE_DIR")
    else {
        return;
    };
    let directory = PathBuf::from(directory);
    assert!(directory.is_dir(), "evidence directory must exist");
    let path = directory.join(format!("{case_name}-{target}.json"));
    let bytes = serde_json::to_vec_pretty(evidence).unwrap();
    use std::{fs::OpenOptions, io::Write};
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    file.write_all(&bytes).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}

fn run_cell(case_name: &str, target_index: usize) {
    let corpus = corpus();
    let case = case(&corpus, case_name);
    let target = &case["targets"][target_index];
    let target_name = target["target"].as_str().unwrap();
    verify_original_runs(
        target,
        "json",
        target["reference_json_exit"].as_i64().unwrap() as i32,
    );
    verify_original_runs(
        target,
        "text",
        target["reference_text_exit"].as_i64().unwrap() as i32,
    );
    let original_root = case["original_case_root"].as_str().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    materialize(case, &root);
    let source_before = source_state(&root);
    let expected_paths = expected_paths(case);
    let source_pathset_match = source_before.keys().eq(expected_paths.keys());
    let frozen_entries_match = frozen_entries_match(case, &source_before);
    let baseline_extra_paths: Vec<_> = source_before
        .keys()
        .filter(|path| !expected_paths.contains_key(*path))
        .cloned()
        .collect();
    let baseline_missing_paths: Vec<_> = expected_paths
        .keys()
        .filter(|path| !source_before.contains_key(*path))
        .cloned()
        .collect();
    let inputs: Vec<_> = case["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| requested_path(&root, v.as_str().unwrap()))
        .collect();
    let native_input_argv: Vec<_> = inputs
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect();
    let mut workspace = Filesystem.load(&inputs, true);
    let version = match target_name {
        "3.12" => PythonVersion::Py312,
        "3.13" => PythonVersion::Py313,
        "3.14" => PythonVersion::Py314,
        other => panic!("unknown target {other}"),
    };
    for project in &mut workspace.projects {
        project.python_version = version;
    }
    let issues: Vec<_> = workspace
        .issues
        .iter()
        .map(|i| json!({"code": i.code, "message": i.message, "path": i.path}))
        .collect();
    let report = super::super::check(&workspace);
    let status = &target["expected_native_status"];
    let json_raw = bytes_from_hex(&target["reporters"]["json"]["runs"][0]["stdout_hex"]);
    let json_rows: Vec<Value> = json_raw
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).expect("original JSON diagnostic row"))
        .collect();
    assert_eq!(
        json_rows,
        *target["reference_json_diagnostics"].as_array().unwrap()
    );
    let text_raw = bytes_from_hex(&target["reporters"]["text"]["runs"][0]["stdout_hex"]);
    if target["reference_text_exit"] == 0 {
        let text = std::str::from_utf8(&text_raw).expect("original success summary is UTF-8");
        let prefix = "Success: no issues found in ";
        let suffix = if target["reference_text_success_source_count"] == 1 {
            " source file\n"
        } else {
            " source files\n"
        };
        let count = text
            .strip_prefix(prefix)
            .and_then(|tail| tail.strip_suffix(suffix))
            .expect("frozen original success summary shape")
            .parse::<usize>()
            .unwrap();
        assert_eq!(
            count,
            target["reference_text_success_source_count"]
                .as_u64()
                .unwrap() as usize
        );
    }
    if target["reference_json_exit"] == 2 {
        assert_eq!(
            status,
            &json!({"completion":"error", "reference_exit_code":2, "checked_files":0})
        );
    } else {
        assert_eq!(status["completion"], "incomplete");
        assert_eq!(status["reference_exit_code"], Value::Null);
        assert_eq!(
            status["checked_files"],
            target["reference_text_success_source_count"]
        );
    }
    let (expected, path_bindings) =
        expected_diagnostics(target, original_root, &root, &case["entries"]);
    let actual = actual_diagnostics(&report, &root);
    let diagnostics_match = actual == expected;
    let expected_completion = if status["completion"] == "error" {
        CompletionStatus::Error
    } else {
        CompletionStatus::Incomplete
    };
    let completion_match = report.completion == expected_completion;
    let expected_exit = status["reference_exit_code"].as_i64().map(|v| v as i32);
    let exit_match = report.reference_exit_code == expected_exit;
    let checked_match = report.checked_files == status["checked_files"].as_u64().unwrap() as usize;
    let identity_match = report.tool == QualityTool::Mypy && report.reference_version == "2.4.0";
    let remaining_nonempty = !report.remaining.is_empty();
    let source_after = source_state(&root);
    let source_preserved = source_before == source_after;
    let added_paths: Vec<_> = source_after
        .keys()
        .filter(|path| !source_before.contains_key(*path))
        .cloned()
        .collect();
    let missing_paths: Vec<_> = source_before
        .keys()
        .filter(|path| !source_after.contains_key(*path))
        .cloned()
        .collect();
    let changed_paths: Vec<_> = source_before
        .iter()
        .filter_map(|(path, before)| {
            source_after
                .get(path)
                .filter(|after| *after != before)
                .map(|_| path.clone())
        })
        .collect();
    let full_report = serde_json::to_value(&report).unwrap();
    let passed = diagnostics_match
        && completion_match
        && exit_match
        && checked_match
        && identity_match
        && remaining_nonempty
        && issues.is_empty()
        && source_pathset_match
        && frozen_entries_match
        && source_preserved;
    let mut evidence = json!({
        "schema":"mamba.quality.mypy-shadow-alias-native-cell.v2",
        "case":case_name,"target":target_name,"inputs":case["inputs"],
        "native_request_argv":native_input_argv,"original_root":original_root,"native_root":root,
        "source_entries":case["entries"],"source_state_before":source_before,"source_state_after":source_after,
        "source_pathset_match":source_pathset_match,"frozen_entries_match":frozen_entries_match,
        "source_baseline_extra_paths":baseline_extra_paths,"source_baseline_missing_paths":baseline_missing_paths,
        "source_added_paths":added_paths,
        "source_missing_paths":missing_paths,"source_changed_paths":changed_paths,"source_preserved":source_preserved,
    });
    evidence.as_object_mut().unwrap().extend(json!({
        "workspace_discovered":workspace.discovered,
        "workspace_sources":workspace.sources.iter().map(|s| json!({"path":s.path,"sha256":s.sha256,"is_stub":s.is_stub})).collect::<Vec<_>>(),
        "workspace_issues":issues,"expected_native_status":status,
        "actual_completion":report.completion,"actual_reference_exit_code":report.reference_exit_code,
        "actual_reference_exit_is_public_computed_state":true,
        "actual_checked_files":report.checked_files,"actual_tool":report.tool,"actual_reference_version":report.reference_version,
        "remaining":report.remaining,"expected_diagnostics":expected,"actual_diagnostics":actual,
        "path_bindings":path_bindings,
        "actual_tool_report":full_report,
        "original_capture":{"original_fixture_utf8":FIXTURE,"fixture_sha256":FIXTURE_SHA256,
            "original_command_argv":{"json":target["reporters"]["json"]["runs"][0]["original_command_argv"],
                "text":target["reporters"]["text"]["runs"][0]["original_command_argv"]},
            "original_command_cwd":{"json":target["reporters"]["json"]["runs"][0]["original_command_cwd"],
                "text":target["reporters"]["text"]["runs"][0]["original_command_cwd"]},
            "json_exit":target["reference_json_exit"],"text_exit":target["reference_text_exit"]},
        "comparisons":{"diagnostics_match":diagnostics_match,"completion_match":completion_match,
            "reference_exit_match":exit_match,"checked_files_match":checked_match,"identity_match":identity_match,
            "remaining_nonempty":remaining_nonempty,"workspace_issues_empty":issues.is_empty(),
            "source_pathset_match":source_pathset_match,"frozen_entries_match":frozen_entries_match,
            "source_preserved":source_preserved},
        "passed":passed,"whole_som_accepted":false,"whole_plan_accepted":false,
    }).as_object().unwrap().clone());
    write_evidence(case_name, target_name, &evidence);
    assert!(
        passed,
        "mypy shadow alias original comparison failed for {case_name}/{target_name}; evidence was written if enabled"
    );
}

macro_rules! cell {
    ($function:ident, $case:literal, $target:expr) => {
        #[test]
        fn $function() {
            run_cell($case, $target);
        }
    };
}

#[cfg(unix)]
cell!(lexical_typing_alias_py312, "lexical_typing_alias", 0);
#[cfg(unix)]
cell!(lexical_typing_alias_py313, "lexical_typing_alias", 1);
#[cfg(unix)]
cell!(lexical_typing_alias_py314, "lexical_typing_alias", 2);
#[cfg(unix)]
cell!(physical_typing_alias_py312, "physical_typing_alias", 0);
#[cfg(unix)]
cell!(physical_typing_alias_py313, "physical_typing_alias", 1);
#[cfg(unix)]
cell!(physical_typing_alias_py314, "physical_typing_alias", 2);
#[cfg(unix)]
cell!(duplicate_first_py312, "duplicate_first", 0);
#[cfg(unix)]
cell!(duplicate_first_py313, "duplicate_first", 1);
#[cfg(unix)]
cell!(duplicate_first_py314, "duplicate_first", 2);
#[cfg(unix)]
cell!(typing_first_py312, "typing_first", 0);
#[cfg(unix)]
cell!(typing_first_py313, "typing_first", 1);
#[cfg(unix)]
cell!(typing_first_py314, "typing_first", 2);
cell!(ordinary_file_control_py312, "ordinary_file_control", 0);
cell!(ordinary_file_control_py313, "ordinary_file_control", 1);
cell!(ordinary_file_control_py314, "ordinary_file_control", 2);
