// These tests retain real failure logs. Do not remove the reported files.
use super::*;
use sha2::{Digest, Sha256};

fn fixture_source(case: &Value) -> &str {
    let source = case["source"].as_str().unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        case["source_sha256"].as_str().unwrap()
    );
    source
}

fn check(root: &Path, filename: &str, source: &str, outside: Option<&Path>) -> Value {
    let before = project_tree(root);
    let output = if let Some(tmpdir) = outside {
        // Match command() exactly, except for this one child environment value.
        Command::new(env!("CARGO_BIN_EXE_mamba"))
            .current_dir(root)
            .env_clear()
            .env("PATH", root.join("empty-tool-path"))
            .env("TMPDIR", tmpdir)
            .env("RUFF_OUTPUT_FORMAT", "github")
            .env("PYLINTRC", root.join("missing-pylintrc"))
            .env("MYPY_CONFIG_FILE", root.join("missing-mypy.ini"))
            .args(["quality", "check", "--json"])
            .output()
            .unwrap()
    } else {
        command(root, &["quality", "check", "--json"])
    };
    super::fatal_capture::record(
        root,
        &["quality", "check", "--json"],
        outside.unwrap_or(root),
        false,
        &output,
    );
    assert_eq!(project_tree(root), before, "Check changed the project tree");
    assert_eq!(fs::read(root.join(filename)).unwrap(), source.as_bytes());
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty(), "stderr: {:?}", output.stderr);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], "mamba.quality.v2");
    assert_eq!(value["operation"], json!({"name": "check"}));
    assert_eq!(value["exit_code"], 2);
    assert_eq!(value["analysis_completed"], false);
    assert_eq!(value["passed"], false);
    assert_eq!(value["files"]["discovered"], 1);
    assert_eq!(value["files"]["written"], 0);
    assert_eq!(value["changes"], json!([]));
    value
}

fn retained_external_log(
    root: &Path,
    path: &Path,
    outside: Option<&Path>,
    parent_depth: usize,
) -> Vec<u8> {
    assert!(path.is_absolute(), "log path must be absolute: {path:?}");
    let metadata = fs::symlink_metadata(path).unwrap();
    assert!(metadata.file_type().is_file(), "log must be a real file");
    let real = fs::canonicalize(path).unwrap();
    assert!(!real.starts_with(fs::canonicalize(root).unwrap()));
    assert!(!path.starts_with(root));
    let storage_parent = if let Some(tmpdir) = outside {
        fs::canonicalize(tmpdir).unwrap()
    } else {
        let system_directory = if cfg!(target_os = "macos") {
            Path::new("/private/tmp")
        } else if cfg!(target_os = "linux") {
            Path::new("/tmp")
        } else {
            panic!("fatal-log relocation needs an explicit system directory on this platform");
        };
        fs::canonicalize(system_directory).unwrap()
    };
    assert_eq!(
        real.ancestors().nth(parent_depth).unwrap(),
        storage_parent.as_path()
    );
    fs::read(path).unwrap()
}

fn black_log(
    root: &Path,
    value: &Value,
    expected: &Value,
    outside: Option<&Path>,
) -> (Vec<u8>, PathBuf) {
    let report = tool(value, "black");
    assert_eq!(report["reference_version"], "26.10.0");
    assert_eq!(report["completion"], "error");
    assert_eq!(report["checked_files"], 0);
    assert!(report["reference_exit_code"].is_null());
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic["tool"], "black");
    assert_eq!(diagnostic["rule"], expected["exception_type"]);
    assert_eq!(diagnostic["path"], json!(root.join("main.py")));
    assert_eq!(diagnostic["severity"], "error");
    assert_eq!(
        diagnostic["location"],
        json!({
            "line": 1, "column": 1, "end_line": null, "end_column": null,
            "offset": null, "end_offset": null,
        })
    );
    assert!(diagnostic["fix"].is_null());
    let message = diagnostic["message"].as_str().unwrap();
    let actual_path = message
        .strip_prefix(expected["expected_message_prefix"].as_str().unwrap())
        .unwrap();
    let path = Path::new(actual_path);
    if let Some(tmpdir) = outside {
        assert_eq!(
            fs::canonicalize(path.parent().unwrap()).unwrap(),
            fs::canonicalize(tmpdir).unwrap()
        );
    }
    let filename = path.file_name().unwrap().to_str().unwrap();
    let random = filename
        .strip_prefix("blk_")
        .unwrap()
        .strip_suffix(".log")
        .unwrap();
    assert_eq!(random.len(), 6);
    assert!(random.bytes().all(|byte| byte.is_ascii_alphanumeric()));
    let bytes = retained_external_log(root, path, outside, 1);
    let hex = expected["log_bytes_hex"].as_str().unwrap();
    assert_eq!(hex.len() % 2, 0);
    let frozen: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect();
    assert_eq!(frozen.len() as u64, expected["log_size"].as_u64().unwrap());
    assert_eq!(
        format!("{:x}", Sha256::digest(&frozen)),
        expected["log_sha256"].as_str().unwrap()
    );
    assert_eq!(bytes, frozen);
    (bytes, path.to_path_buf())
}

fn pylint_log(
    root: &Path,
    value: &Value,
    case: &Value,
    outside: Option<&Path>,
) -> (Vec<u8>, PathBuf) {
    let report = tool(value, "pylint");
    assert_eq!(report["reference_version"], "4.1.2");
    assert_eq!(report["completion"], "error");
    assert_eq!(report["checked_files"], 1);
    assert_eq!(report["reference_exit_code"], case["reference_exit"]);
    let expected = case["original_messages"].as_array().unwrap();
    assert_eq!(expected.len(), 1);
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic["tool"], "pylint");
    assert_eq!(diagnostic["rule"], expected[0]["messageId"]);
    assert_eq!(diagnostic["severity"], expected[0]["type"]);
    let source_path = root.join(case["filename"].as_str().unwrap());
    assert_eq!(diagnostic["path"], json!(source_path));
    for (actual, frozen) in [
        ("line", "line"),
        ("column", "column"),
        ("end_line", "endLine"),
        ("end_column", "endColumn"),
    ] {
        assert_eq!(diagnostic["location"][actual], expected[0][frozen]);
    }
    assert!(diagnostic["fix"].is_null());
    let message = diagnostic["message"].as_str().unwrap();
    let prefix = format!(
        "{0}: Fatal error while checking '{0}'. Please open an issue in our bug tracker so we address this. There is a pre-filled template that you can use in '",
        source_path.display()
    );
    let actual_path = message
        .strip_prefix(&prefix)
        .unwrap()
        .strip_suffix("'.")
        .unwrap();
    let path = Path::new(actual_path);
    if let Some(tmpdir) = outside {
        assert_eq!(
            fs::canonicalize(path.parent().unwrap().parent().unwrap()).unwrap(),
            fs::canonicalize(tmpdir).unwrap()
        );
    }
    let filename = path.file_name().unwrap().to_str().unwrap();
    let stamp = filename
        .strip_prefix("pylint-crash-")
        .unwrap()
        .strip_suffix(".txt")
        .unwrap();
    let pieces: Vec<_> = stamp.split('-').collect();
    assert_eq!(pieces.len(), 6);
    for (piece, width) in pieces.iter().zip([4, 2, 2, 2, 2, 2]) {
        assert_eq!(piece.len(), width);
        assert!(piece.bytes().all(|byte| byte.is_ascii_digit()));
    }
    let numbers: Vec<u32> = pieces.iter().map(|piece| piece.parse().unwrap()).collect();
    assert!((1..=12).contains(&numbers[1]));
    assert!((1..=31).contains(&numbers[2]));
    assert!(numbers[3] < 24 && numbers[4] < 60 && numbers[5] < 60);
    let parent = path
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    let random = parent.strip_prefix("mamba-quality-pylint-").unwrap();
    assert_eq!(random.len(), 6);
    assert!(random.bytes().all(|byte| byte.is_ascii_alphanumeric()));
    let bytes = retained_external_log(root, path, outside, 2);
    let expected_bytes = format!(
        "Pinned Pylint 4.1.2 / Astroid 4.3.3 build failure\n\
         Native Rust reproduction: sorting mixed bytes and str slot constants.\n\
         Reference exception: TypeError: '<' not supported between instances of 'bytes' and 'str'\n\
         Source: {}\nSHA-256: {}\n\n```python\n{}\n```\n",
        source_path.display(),
        case["source_sha256"].as_str().unwrap(),
        fixture_source(case)
    );
    assert_eq!(bytes.as_slice(), expected_bytes.as_bytes());
    (bytes, path.to_path_buf())
}

// Change exactly the parsed saved-log suffix in one diagnostic message.
// All other fields, diagnostics, order, and source paths must remain identical.
fn with_saved_log_path(report: &Value, name: &str, from: &Path, to: &Path, suffix: &str) -> Value {
    let mut adjusted = report.clone();
    let target = adjusted["tools"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|tool| tool["tool"] == name)
        .unwrap();
    let diagnostics = target["diagnostics"].as_array_mut().unwrap();
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = &mut diagnostics[0];
    let message = diagnostic["message"].as_str().unwrap();
    let without_suffix = message.strip_suffix(suffix).unwrap();
    let prefix = without_suffix.strip_suffix(from.to_str().unwrap()).unwrap();
    diagnostic["message"] = Value::String(format!("{prefix}{}{suffix}", to.display()));
    adjusted
}

#[test]
fn check_black_safety_error_retains_exact_log_outside_project() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../mamba-quality/src/infrastructure/black/tests/safety.json"
    ))
    .unwrap();
    let logs: Value = serde_json::from_str(include_str!(
        "../../../../mamba-quality/src/infrastructure/black/tests/safety_logs.json"
    ))
    .unwrap();
    let case = cases
        .iter()
        .find(|case| case["name"] == "historical_safety_000")
        .unwrap();
    let source = fixture_source(case);
    assert_eq!(
        case["source_sha256"],
        "9f34fe60cd0c3cc2be075fcfc79f33a1cd0a1900408e11a4a1a21ff83a7e5445"
    );
    for version in VERSIONS {
        let expected = logs["cells"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["case"] == case["name"] && row["python"] == version)
            .unwrap();
        assert_eq!(expected["source_sha256"], case["source_sha256"]);
        assert_eq!(expected["exception_type"], "black.parsing.ASTSafetyError");
        assert_eq!(expected["reference_exit"], 123);
        // Retain the project too, so a regressed in-project log survives a panic.
        let project = project(version, source).keep();
        let root = fs::canonicalize(project).unwrap();
        let value = check(&root, "main.py", source, None);
        assert_eq!(value["projects"][0]["python_version"], version);
        let (redirected, redirected_path) = black_log(&root, &value, expected, None);
        // Keep this control directory and its genuine failure log too.
        let outside = tempfile::Builder::new()
            .prefix("quality-cli-outside-")
            .tempdir()
            .unwrap()
            .keep();
        let control = check(&root, "main.py", source, Some(&outside));
        let (original, original_path) = black_log(&root, &control, expected, Some(&outside));
        assert_eq!(redirected, original);
        assert_eq!(
            with_saved_log_path(&value, "black", &redirected_path, &original_path, ""),
            control
        );
    }
}

#[test]
fn check_pylint_slots_fatal_retains_exact_log_outside_project() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../mamba-quality/src/infrastructure/pylint/tests/build_probes.json"
    ))
    .unwrap();
    for version in VERSIONS {
        let case = cases
            .iter()
            .find(|case| case["filename"] == "probe_003.py" && case["python"] == version)
            .unwrap();
        assert_eq!(case["fatal"], true);
        assert_eq!(case["reference_exit"], 1);
        assert_eq!(
            case["source_sha256"],
            "3d461e9492d46262586cac87c6af71f1e94422dd92c7201f3cad82b003c9587b"
        );
        let source = fixture_source(case);
        // Retain the project too, so a regressed in-project log survives a panic.
        let project = project(version, source).keep();
        let root = fs::canonicalize(project).unwrap();
        fs::rename(root.join("main.py"), root.join("probe_003.py")).unwrap();
        let value = check(&root, "probe_003.py", source, None);
        assert_eq!(value["projects"][0]["python_version"], version);
        let (redirected, redirected_path) = pylint_log(&root, &value, case, None);
        let outside = tempfile::Builder::new()
            .prefix("quality-cli-outside-")
            .tempdir()
            .unwrap()
            .keep();
        let control = check(&root, "probe_003.py", source, Some(&outside));
        let (original, original_path) = pylint_log(&root, &control, case, Some(&outside));
        assert_eq!(redirected, original);
        assert_eq!(
            with_saved_log_path(&value, "pylint", &redirected_path, &original_path, "'."),
            control
        );
    }
}
