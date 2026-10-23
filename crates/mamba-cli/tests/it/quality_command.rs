//! Public quality commands run through the actual mamba executable.
use serde_json::{json, Value};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};
const VERSIONS: [&str; 3] = ["3.12", "3.13", "3.14"];
const SOURCE: &str = "\"\"\"Command fixture.\"\"\"\nVALUE: int = 1\n";

fn project(version: &str, source: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("pyproject.toml"),
        format!("[project]\nrequires-python = '>={version}'\n"),
    )
    .unwrap();
    fs::write(directory.path().join("main.py"), source).unwrap();
    directory
}

fn command(root: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mamba"))
        .current_dir(root)
        .env_clear()
        .env("PATH", root.join("empty-tool-path"))
        .env("TMPDIR", root)
        .env("RUFF_OUTPUT_FORMAT", "github")
        .env("PYLINTRC", root.join("missing-pylintrc"))
        .env("MYPY_CONFIG_FILE", root.join("missing-mypy.ini"))
        .args(arguments)
        .output()
        .unwrap()
}

fn report(root: &Path, arguments: &[&str]) -> Value {
    let output = command(root, arguments);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{arguments:?}: {error}; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(value["schema"], "mamba.quality.v2");
    assert_eq!(
        value["exit_code"].as_i64(),
        output.status.code().map(i64::from)
    );
    assert!(matches!(output.status.code(), Some(0..=2)));
    value
}

#[derive(Debug, PartialEq, Eq)]
struct TreeEntry {
    kind: &'static str,
    permissions: u32,
    modified: SystemTime,
    bytes: Option<Vec<u8>>,
    symlink_target: Option<PathBuf>,
}

fn project_tree(root: &Path) -> BTreeMap<PathBuf, TreeEntry> {
    fn visit(root: &Path, directory: &Path, snapshot: &mut BTreeMap<PathBuf, TreeEntry>) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            let metadata = fs::symlink_metadata(&path).unwrap();
            let file_type = metadata.file_type();
            let (kind, bytes, symlink_target) = if file_type.is_symlink() {
                ("symlink", None, Some(fs::read_link(&path).unwrap()))
            } else if file_type.is_dir() {
                ("directory", None, None)
            } else if file_type.is_file() {
                ("file", Some(fs::read(&path).unwrap()), None)
            } else {
                ("other", None, None)
            };
            snapshot.insert(
                relative,
                TreeEntry {
                    kind,
                    permissions: permissions(&metadata),
                    modified: metadata.modified().unwrap(),
                    bytes,
                    symlink_target,
                },
            );
            if file_type.is_dir() {
                visit(root, &path, snapshot);
            }
        }
    }

    let mut snapshot = BTreeMap::new();
    let metadata = fs::symlink_metadata(root).unwrap();
    snapshot.insert(
        PathBuf::new(),
        TreeEntry {
            kind: "directory",
            permissions: permissions(&metadata),
            modified: metadata.modified().unwrap(),
            bytes: None,
            symlink_target: None,
        },
    );
    visit(root, root, &mut snapshot);
    snapshot
}

#[cfg(unix)]
fn permissions(metadata: &fs::Metadata) -> u32 {
    metadata.permissions().mode()
}

#[cfg(not(unix))]
fn permissions(metadata: &fs::Metadata) -> u32 {
    u32::from(metadata.permissions().readonly())
}

fn tool<'a>(value: &'a Value, name: &str) -> &'a Value {
    value["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["tool"] == name)
        .unwrap()
}

#[test]
fn info_locks_versions_policy_and_default_python_without_tool_settings() {
    for version in VERSIONS {
        let directory = project(version, SOURCE);
        let root = directory.path();
        let metadata = format!(
            "[project]\nrequires-python = '>={version}.1'\n\
             [tool.ruff]\nselect = ['ALL']\n\
             [tool.black]\nline-length = 120\n"
        );
        fs::write(root.join("pyproject.toml"), &metadata).unwrap();
        fs::write(root.join(".pylintrc"), "[MAIN]\ndisable=all\n").unwrap();
        fs::write(root.join("mypy.ini"), "[mypy]\nignore_errors=True\n").unwrap();
        let value = report(root, &["quality", "info", "--json"]);
        assert_eq!(value["exit_code"], 0);
        assert_eq!(value["operation"]["name"], "info");
        assert_eq!(value["projects"][0]["python_version"], version);
        assert_eq!(value["profile"]["line_width"], 88);
        assert_eq!(
            value["profile"]["versions"],
            json!({"ruff":"0.16.10","isort":"9.0.2","mypy":"2.4.0",
                "pylint":"4.1.2","black":"26.10.0","pydantic":"2.13.5"})
        );
        assert_eq!(
            value["profile"]["ruff"]["select"],
            json!(["E4", "E7", "E9", "F", "UP", "B", "SIM", "RUF100"])
        );
        assert_eq!(
            value["profile"]["mypy"]["strict_flags"]
                .as_array()
                .unwrap()
                .len(),
            13
        );
        assert_eq!(
            value["profile"]["pylint"]["rules"]
                .as_array()
                .unwrap()
                .len(),
            383
        );
        assert_eq!(value["files"]["discovered"], 0);
        assert!(value["projects"][0]["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|source| source["kind"] == "embedded-type-stubs" && source["path"].is_null()));
        assert_eq!(
            fs::read_to_string(root.join("pyproject.toml")).unwrap(),
            metadata
        );
        assert_eq!(fs::read_to_string(root.join("main.py")).unwrap(), SOURCE);
        fs::remove_file(root.join("pyproject.toml")).unwrap();
        let default = report(root, &["quality", "info", "--json"]);
        assert_eq!(default["projects"][0]["python_version"], "3.12");
    }
}

#[test]
fn default_check_matches_explicit_check_and_never_writes_project_tree() {
    for version in VERSIONS {
        let directory = project(version, SOURCE);
        let root = directory.path();
        let before = project_tree(root);
        let implicit = command(root, &["quality"]);
        assert_eq!(
            project_tree(root),
            before,
            "implicit Check wrote to project"
        );
        let explicit = command(root, &["quality", "check"]);
        assert_eq!(
            project_tree(root),
            before,
            "explicit Check wrote to project"
        );
        assert_eq!(implicit.status.code(), explicit.status.code());
        assert_eq!(implicit.stdout, explicit.stdout);
        assert_eq!(implicit.stderr, explicit.stderr);
        let value = report(root, &["quality", "check", "--json"]);
        assert_eq!(project_tree(root), before, "JSON Check wrote to project");
        assert_eq!(value["operation"]["name"], "check");
        assert_eq!(value["projects"][0]["python_version"], version);
        assert_eq!(value["tools"].as_array().unwrap().len(), 5);
        assert_eq!(value["files"]["written"], 0);
        if value["analysis_completed"] == false {
            assert_eq!(value["exit_code"], 2);
            assert_eq!(value["passed"], false);
        }
        assert_eq!(fs::read_to_string(root.join("main.py")).unwrap(), SOURCE);
    }
}

#[test]
fn check_discovery_and_explicit_inputs_keep_python_stub_tool_counts() {
    for version in VERSIONS {
        let directory = project(version, SOURCE);
        let root = directory.path();
        fs::write(root.join(".gitignore"), "ignored.py\n").unwrap();
        fs::write(root.join("ignored.py"), SOURCE).unwrap();
        fs::write(root.join("visible.pyi"), "VALUE: int\n").unwrap();
        fs::create_dir(root.join(".venv")).unwrap();
        fs::write(root.join(".venv/vendor.py"), SOURCE).unwrap();
        let discovered = report(root, &["quality", "check", "--json"]);
        assert_eq!(discovered["files"]["discovered"], 2);
        for name in ["ruff", "isort", "mypy", "black"] {
            assert_eq!(tool(&discovered, name)["checked_files"], 2, "{name}");
        }
        assert_eq!(tool(&discovered, "pylint")["checked_files"], 1);
        let explicit = report(
            root,
            &[
                "quality",
                "check",
                "ignored.py",
                ".venv/vendor.py",
                "--json",
            ],
        );
        assert_eq!(explicit["files"]["discovered"], 2);
        assert_eq!(tool(&explicit, "pylint")["checked_files"], 2);
        for name in ["main.py", "ignored.py", ".venv/vendor.py"] {
            assert_eq!(fs::read_to_string(root.join(name)).unwrap(), SOURCE);
        }
        assert_eq!(
            fs::read_to_string(root.join("visible.pyi")).unwrap(),
            "VALUE: int\n"
        );
    }
}

#[test]
fn fix_diff_and_format_read_only_modes_keep_candidate_changes_unapplied() {
    let original = "import sys\nimport os\n\nVALUE='one'\n";
    for version in VERSIONS {
        let directory = project(version, original);
        let root = directory.path();
        for arguments in [
            vec!["quality", "fix", "--diff", "--json"],
            vec!["quality", "format", "--diff", "--json"],
            vec!["quality", "format", "--check", "--json"],
        ] {
            let value = report(root, &arguments);
            assert_eq!(value["files"]["written"], 0);
            assert!(value["files"]["changed"].as_u64().unwrap() > 0);
            assert!(value["changes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|change| change["applied"] == false));
            if arguments.contains(&"--diff") {
                assert!(value["changes"].as_array().unwrap().iter().any(|change| {
                    change["diff"]
                        .as_str()
                        .is_some_and(|diff| diff.contains("@@"))
                }));
            }
            assert_eq!(fs::read_to_string(root.join("main.py")).unwrap(), original);
        }
    }
}

#[test]
fn incomplete_formatters_cannot_publish_from_write_commands() {
    for version in VERSIONS {
        for operation in ["fix", "format"] {
            let original = "VALUE='one'\n";
            let directory = project(version, original);
            let root = directory.path();
            let value = report(root, &["quality", operation, "--json"]);
            let incomplete = ["isort", "black"]
                .iter()
                .any(|name| tool(&value, name)["completion"] != "complete");
            if incomplete {
                assert_eq!(value["exit_code"], 2);
                assert_eq!(value["files"]["written"], 0);
                assert_eq!(fs::read_to_string(root.join("main.py")).unwrap(), original);
            } else {
                assert_eq!(
                    fs::read_to_string(root.join("main.py")).unwrap(),
                    "VALUE = \"one\"\n"
                );
                assert_eq!(value["files"]["written"], 1);
                let repeated = report(root, &["quality", operation, "--json"]);
                assert_eq!(repeated["files"]["changed"], 0);
                assert_eq!(repeated["files"]["written"], 0);
            }
        }
    }
}

#[test]
fn unsupported_versions_and_profile_override_flags_return_error_exit() {
    for version in ["3.11", "3.15"] {
        let directory = project(version, SOURCE);
        let value = report(directory.path(), &["quality", "info", "--json"]);
        assert_eq!(value["exit_code"], 2);
        assert_eq!(value["passed"], false);
        assert!(value["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["code"] == "unsupported-python-version"));
    }
    let directory = project("3.12", SOURCE);
    for arguments in [
        vec!["quality", "format", "--check", "--diff"],
        vec!["quality", "check", "--python", "python"],
        vec!["quality", "check", "--select", "ALL"],
        vec!["quality", "check", "--config", "custom.toml"],
        vec!["quality", "check", "--engine", "cpython"],
    ] {
        let output = command(directory.path(), &arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(!output.stderr.is_empty());
        assert_eq!(
            fs::read_to_string(directory.path().join("main.py")).unwrap(),
            SOURCE
        );
    }
}

// Byte-path controls are Unix-only. Their process arguments remain ASCII.
#[cfg(unix)]
mod bytepath_controls {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::ffi::OsString;
    use std::io::Write;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::PathBuf;

    fn digest(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn snapshot(root: &Path) -> Value {
        let mut files = serde_json::Map::new();
        for name in ["main.py", "pyproject.toml"] {
            let bytes = fs::read(root.join(name)).unwrap();
            files.insert(
                name.into(),
                json!({"hex": hex(&bytes), "sha256": digest(&bytes), "bytes": bytes.len()}),
            );
        }
        Value::Object(files)
    }

    fn fresh_write(path: &Path, bytes: &[u8]) {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
    }

    fn record_raw(evidence: &Path, case: &str, output: &std::process::Output) {
        let directory = evidence.join(case);
        fs::create_dir(&directory).unwrap();
        fresh_write(&directory.join("stdout"), &output.stdout);
        fresh_write(&directory.join("stderr"), &output.stderr);
    }

    fn excluded_invalid_byte_cases() -> Vec<Value> {
        #[cfg(target_os = "macos")]
        {
            VERSIONS
                .iter()
                .flat_map(|version| ["json", "text"].map(move |mode| (*version, mode)))
                .map(|(version, mode)| {
                    json!({
                        "case": format!("{version}-invalid-byte-{mode}"),
                        "status": "not_run",
                        "reason": "The observed macOS APFS temporary filesystem rejected the non-UTF-8 path component during setup (EILSEQ, raw OS code 92), before the CLI started.",
                    })
                })
                .collect()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Vec::new()
        }
    }

    fn write_rows(
        path: &Path,
        rows: &[Value],
        unrun_cases: &[Value],
        mismatches: Option<&[String]>,
    ) {
        // Match the existing publication tests: write observed data before
        // the final assertion. Raw files never replace a previous capture.
        fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "schema": "mamba.quality.cli-bytepath-controls.v1",
                "platform": std::env::consts::OS,
                "rows": rows,
                "unrun_cases": unrun_cases,
                "mismatches": mismatches,
                "whole_cli_accepted": false,
                "whole_plan_accepted": false,
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn check(case: &str, label: &str, expected: Value, actual: Value, failures: &mut Vec<String>) {
        if expected != actual {
            failures.push(format!(
                "{case}/{label}: expected {expected}; actual {actual}"
            ));
        }
    }

    #[test]
    fn info_byte_paths_preserve_output_and_error_exit_contract() {
        let parent = tempfile::tempdir().unwrap();
        let binary = Path::new(env!("CARGO_BIN_EXE_mamba"));
        let binary_sha256 = digest(&fs::read(binary).unwrap());
        let harness_sha256 = digest(include_bytes!("quality_command.rs"));
        let evidence = std::env::var_os("MAMBA_QUALITY_CLI_BYTEPATH_EVIDENCE").map(PathBuf::from);
        let raw_directory = evidence.as_ref().map(|path| {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            // Reserve the report too. A controller must choose a fresh path.
            fresh_write(path, b"{\"capture_started\":true}\n");
            let raw = path.with_extension("raw");
            fs::create_dir(&raw).unwrap();
            raw
        });
        let mut rows = Vec::new();
        let unrun_cases = excluded_invalid_byte_cases();
        for version in VERSIONS {
            let target = parent.path().join(version);
            fs::create_dir(&target).unwrap();
            let path_cases = vec![
                ("ascii", b"quality-ascii".as_slice()),
                ("unicode", "quality-水".as_bytes()),
            ];
            #[cfg(target_os = "linux")]
            let path_cases = {
                let mut cases = path_cases;
                cases.push(("invalid-byte", b"quality-\xff".as_slice()));
                cases
            };
            for (kind, name) in path_cases {
                let root = target.join(OsString::from_vec(name.to_vec()));
                fs::create_dir(&root).unwrap();
                fs::write(root.join("main.py"), SOURCE.as_bytes()).unwrap();
                fs::write(
                    root.join("pyproject.toml"),
                    format!("[project]\nrequires-python = '>={version}'\n"),
                )
                .unwrap();
                let root = fs::canonicalize(root).unwrap();
                for (mode, arguments) in [
                    ("json", vec!["quality", "info", "--json"]),
                    ("text", vec!["quality", "info"]),
                ] {
                    let case = format!("{version}-{kind}-{mode}");
                    let before = snapshot(&root);
                    let output = command(&root, &arguments);
                    let after = snapshot(&root);
                    if let Some(raw) = &raw_directory {
                        record_raw(raw, &case, &output);
                        let directory = raw.join(&case);
                        for name in ["main.py", "pyproject.toml"] {
                            // All fixture bytes are also in the JSON snapshots.
                            fresh_write(&directory.join(name), &fs::read(root.join(name)).unwrap());
                        }
                    }
                    // JSON records use cwd bytes, not a lossy PathBuf encoding.
                    rows.push(json!({
                        "case": case, "target": version, "path_kind": kind, "mode": mode,
                        "arguments": arguments,
                        "cwd_bytes_hex": hex(root.as_os_str().as_bytes()),
                        "cwd_display": root.display().to_string(),
                        "binary_sha256": binary_sha256,
                        "harness_sha256": harness_sha256,
                        "exit_code": output.status.code(), "success": output.status.success(),
                        "stdout_hex": hex(&output.stdout), "stderr_hex": hex(&output.stderr),
                        "stdout_sha256": digest(&output.stdout), "stderr_sha256": digest(&output.stderr),
                        "stdout_bytes": output.stdout.len(), "stderr_bytes": output.stderr.len(),
                        "input_before": before, "input_after": after,
                        "expected_exit_code": if kind == "invalid-byte" && mode == "json" { 2 } else { 0 },
                        "captured_before_assertions": true,
                        "parsed_json": serde_json::from_slice::<Value>(&output.stdout).ok(),
                        "stdout_utf8": std::str::from_utf8(&output.stdout).ok(),
                        "stderr_utf8": std::str::from_utf8(&output.stderr).ok(),
                    }));
                    if let Some(path) = &evidence {
                        write_rows(path, &rows, &unrun_cases, None);
                    }
                }
            }
        }
        // Capture every cell first. The expected failing baseline must not
        // stop collection at the first invalid-byte case.
        let mut failures = Vec::new();
        check(
            "matrix",
            "supported_platform",
            json!(true),
            json!(cfg!(any(target_os = "macos", target_os = "linux"))),
            &mut failures,
        );
        check(
            "matrix",
            "row_count",
            json!(if cfg!(target_os = "linux") { 18 } else { 12 }),
            json!(rows.len()),
            &mut failures,
        );
        check(
            "matrix",
            "unrun_count",
            json!(if cfg!(target_os = "macos") { 6 } else { 0 }),
            json!(unrun_cases.len()),
            &mut failures,
        );
        for row in &rows {
            let case = row["case"].as_str().unwrap();
            let invalid_json = row["path_kind"] == "invalid-byte" && row["mode"] == "json";
            check(
                case,
                "exit",
                row["expected_exit_code"].clone(),
                row["exit_code"].clone(),
                &mut failures,
            );
            check(
                case,
                "input_bytes",
                row["input_before"].clone(),
                row["input_after"].clone(),
                &mut failures,
            );
            if invalid_json {
                check(
                    case,
                    "no_lossy_json",
                    json!(0),
                    row["stdout_bytes"].clone(),
                    &mut failures,
                );
                let error = row["stderr_utf8"].as_str().unwrap_or("");
                check(
                    case,
                    "explicit_serialization_error",
                    json!(true),
                    json!(error.contains("path contains invalid UTF-8 characters")),
                    &mut failures,
                );
            } else {
                check(
                    case,
                    "empty_stderr",
                    json!(0),
                    row["stderr_bytes"].clone(),
                    &mut failures,
                );
                if row["mode"] == "json" {
                    let value = &row["parsed_json"];
                    check(
                        case,
                        "schema",
                        json!("mamba.quality.v2"),
                        value["schema"].clone(),
                        &mut failures,
                    );
                    check(
                        case,
                        "operation",
                        json!({"name": "info"}),
                        value["operation"].clone(),
                        &mut failures,
                    );
                    check(
                        case,
                        "reported_exit",
                        json!(0),
                        value["exit_code"].clone(),
                        &mut failures,
                    );
                    check(
                        case,
                        "completed",
                        json!(true),
                        value["analysis_completed"].clone(),
                        &mut failures,
                    );
                    check(
                        case,
                        "passed",
                        json!(true),
                        value["passed"].clone(),
                        &mut failures,
                    );
                    check(
                        case,
                        "target",
                        row["target"].clone(),
                        value["projects"][0]["python_version"].clone(),
                        &mut failures,
                    );
                    check(
                        case,
                        "exact_unicode_root",
                        row["cwd_display"].clone(),
                        value["projects"][0]["root"].clone(),
                        &mut failures,
                    );
                    check(
                        case,
                        "file_counts",
                        json!({"discovered":0,"checked":0,"changed":0,"written":0}),
                        value["files"].clone(),
                        &mut failures,
                    );
                    check(
                        case,
                        "no_issues",
                        json!([]),
                        value["issues"].clone(),
                        &mut failures,
                    );
                    check(
                        case,
                        "no_changes",
                        json!([]),
                        value["changes"].clone(),
                        &mut failures,
                    );
                    check(
                        case,
                        "versions",
                        json!({"ruff":"0.16.10","isort":"9.0.2","mypy":"2.4.0","pylint":"4.1.2","black":"26.10.0","pydantic":"2.13.5"}),
                        value["profile"]["versions"].clone(),
                        &mut failures,
                    );
                    let tools = value["tools"].as_array();
                    check(
                        case,
                        "five_info_tools",
                        json!(5),
                        json!(tools.map(Vec::len)),
                        &mut failures,
                    );
                    check(
                        case,
                        "not_requested",
                        json!(true),
                        json!(tools.is_some_and(|tools| tools.iter().all(|tool| tool
                            ["completion"]
                            == "not_requested"
                            && tool["reference_exit_code"].is_null()
                            && tool["checked_files"] == 0))),
                        &mut failures,
                    );
                } else {
                    let text = row["stdout_utf8"].as_str().unwrap_or("");
                    let first = format!(
                        "{}: Python {}\n",
                        row["cwd_display"].as_str().unwrap(),
                        row["target"].as_str().unwrap()
                    );
                    check(
                        case,
                        "current_display_prefix",
                        json!(true),
                        json!(text.starts_with(&first)),
                        &mut failures,
                    );
                    let dependency = format!(
                        "Dependency data: project sources and type stubs ({})\n",
                        row["cwd_display"].as_str().unwrap()
                    );
                    check(
                        case,
                        "current_dependency_display",
                        json!(true),
                        json!(text.contains(&dependency)),
                        &mut failures,
                    );
                    for line in [
                        "Line width: 88\n",
                        "ruff: 0.16.10\n",
                        "isort: 9.0.2\n",
                        "mypy: 2.4.0\n",
                        "pylint: 4.1.2\n",
                        "black: 26.10.0\n",
                        "pydantic: 2.13.5\n",
                    ] {
                        check(
                            case,
                            "fixed_text_line",
                            json!(true),
                            json!(text.contains(line)),
                            &mut failures,
                        );
                    }
                }
            }
        }
        if let Some(path) = &evidence {
            write_rows(path, &rows, &unrun_cases, Some(&failures));
        }
        assert!(
            failures.is_empty(),
            "byte-path contract mismatches: {failures:#?}"
        );
    }
}

#[path = "quality_command/fatal_check_logs.rs"]
mod fatal_check_logs;

#[path = "quality_command/remaining_fatal_logs.rs"]
mod remaining_fatal_logs;

#[path = "quality_command/capture.rs"]
mod fatal_capture;
