//! Public Check controls for the remaining real failure-log branches.
#![cfg(unix)]
use super::*;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::os::unix::fs::MetadataExt;

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

struct Restricted {
    path: PathBuf,
    permissions: fs::Permissions,
}

impl Restricted {
    fn src(root: &Path) -> Self {
        // The fixture creates this private directory. Never restrict another path.
        assert_ne!(
            fs::metadata(root).unwrap().uid(),
            0,
            "run this test as a real non-root user"
        );
        let executable = fs::metadata(env!("CARGO_BIN_EXE_mamba")).unwrap();
        assert_eq!(
            executable.permissions().mode() & 0o6000,
            0,
            "CLI must inherit the non-root identity"
        );
        let path = root.join("src");
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert!(metadata.file_type().is_dir());
        assert_eq!(fs::canonicalize(&path).unwrap().parent().unwrap(), root);
        let restriction = Self {
            path,
            permissions: metadata.permissions(),
        };
        fs::set_permissions(&restriction.path, fs::Permissions::from_mode(0)).unwrap();
        // Fail rather than pass if root or a privilege override can bypass chmod.
        let denied = fs::metadata(root.join("src/pkg")).unwrap_err();
        assert_eq!(denied.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(denied.raw_os_error(), Some(13));
        restriction
    }
}

impl Drop for Restricted {
    fn drop(&mut self) {
        fs::set_permissions(&self.path, self.permissions.clone()).unwrap();
    }
}

struct ChildGuard(std::process::Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn bounded_output(command: &mut Command) -> std::process::Output {
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let mut child = ChildGuard(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut stdout = child.0.stdout.take().unwrap();
    let mut stderr = child.0.stderr.take().unwrap();
    let stdout = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let stderr = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "CLI timed out after 120 seconds");
        std::thread::sleep(Duration::from_millis(10));
    };
    std::process::Output {
        status,
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    }
}

fn check(root: &Path, outside: Option<&Path>, restrict_src: bool) -> Value {
    let before = project_tree(root);
    let restriction = restrict_src.then(|| Restricted::src(root));
    let output = bounded_output(
        Command::new(env!("CARGO_BIN_EXE_mamba"))
            .current_dir(root)
            .env_clear()
            .env("PATH", root.join("empty-tool-path"))
            .env("TMPDIR", outside.unwrap_or(root))
            .env("RUFF_OUTPUT_FORMAT", "github")
            .env("PYLINTRC", root.join("missing-pylintrc"))
            .env("MYPY_CONFIG_FILE", root.join("missing-mypy.ini"))
            .args(["quality", "check", "main.py", "--json"]),
    );
    drop(restriction);
    super::fatal_capture::record(
        root,
        &["quality", "check", "main.py", "--json"],
        outside.unwrap_or(root),
        restrict_src,
        &output,
    );
    // Restore permissions first, then compare every original name and byte.
    assert_eq!(project_tree(root), before, "Check changed the project tree");
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

fn log_bytes(root: &Path, path: &Path, outside: Option<&Path>, depth: usize) -> Vec<u8> {
    assert!(path.is_absolute());
    assert!(fs::symlink_metadata(path).unwrap().file_type().is_file());
    let real = fs::canonicalize(path).unwrap();
    assert!(!real.starts_with(root));
    assert!(!path.starts_with(root));
    let parent = if let Some(outside) = outside {
        fs::canonicalize(outside).unwrap()
    } else if cfg!(target_os = "macos") {
        fs::canonicalize("/private/tmp").unwrap()
    } else if cfg!(target_os = "linux") {
        fs::canonicalize("/tmp").unwrap()
    } else {
        panic!("define the exact system temporary directory for this platform");
    };
    assert_eq!(real.ancestors().nth(depth).unwrap(), parent.as_path());
    fs::read(path).unwrap()
}

fn with_saved_log_path(
    report: &Value,
    name: &str,
    rule: &str,
    from: &Path,
    to: &Path,
    suffix: &str,
) -> Value {
    let mut result = report.clone();
    let tool = result["tools"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|tool| tool["tool"] == name)
        .unwrap();
    let matching: Vec<_> = tool["diagnostics"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .filter(|diagnostic| diagnostic["rule"] == rule)
        .collect();
    assert_eq!(matching.len(), 1);
    let diagnostic = matching.into_iter().next().unwrap();
    let message = diagnostic["message"].as_str().unwrap();
    let prefix = message
        .strip_suffix(suffix)
        .unwrap()
        .strip_suffix(from.to_str().unwrap())
        .unwrap();
    diagnostic["message"] = json!(format!("{prefix}{}{suffix}", to.display()));
    result
}

fn invalid_output_log(
    root: &Path,
    value: &Value,
    expected: &Value,
    hash: &str,
    outside: Option<&Path>,
) -> PathBuf {
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
    let marker = "This invalid output might be helpful: ";
    let (body, _) = expected["message"]
        .as_str()
        .unwrap()
        .rsplit_once(marker)
        .unwrap();
    let prefix = format!("{body}{marker}");
    let path = PathBuf::from(
        diagnostic["message"]
            .as_str()
            .unwrap()
            .strip_prefix(&prefix)
            .unwrap(),
    );
    let filename = path.file_name().unwrap().to_str().unwrap();
    let random = filename
        .strip_prefix("blk_")
        .unwrap()
        .strip_suffix(".log")
        .unwrap();
    assert_eq!(random.len(), 6);
    assert!(random.bytes().all(|byte| byte.is_ascii_alphanumeric()));
    let bytes = log_bytes(root, &path, outside, 1);
    assert_eq!(bytes.len(), 50);
    assert_eq!(sha(&bytes), hash);
    path
}

fn permission_log(
    root: &Path,
    value: &Value,
    case: &Value,
    target: usize,
    outside: Option<&Path>,
) -> Option<PathBuf> {
    let report = tool(value, "pylint");
    assert_eq!(report["reference_version"], "4.1.2");
    assert_eq!(report["completion"], case["expected_completion"][target]);
    assert_eq!(report["checked_files"], 1);
    let expected = case["expected"][target].as_array().unwrap();
    let actual = report["diagnostics"].as_array().unwrap();
    assert_eq!(actual.len(), expected.len());
    let fatal = expected
        .iter()
        .any(|diagnostic| diagnostic["rule"] == "F0002");
    assert_eq!(
        report["reference_exit_code"],
        if fatal {
            case["reference_exits"][target].clone()
        } else {
            Value::Null
        }
    );
    let source_path = root.join("main.py");
    let mut saved = None;
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual["tool"], "pylint");
        assert_eq!(actual["rule"], expected["rule"]);
        assert_eq!(actual["severity"], expected["severity"]);
        assert_eq!(
            actual["path"],
            json!(root.join(expected["path"].as_str().unwrap()))
        );
        for name in ["line", "column", "end_line", "end_column"] {
            assert_eq!(actual["location"][name], expected["location"][name]);
        }
        assert!(actual["fix"].is_null());
        if expected["rule"] != "F0002" {
            assert_eq!(actual["message"], expected["message"]);
            continue;
        }
        let prefix = format!("{0}: Fatal error while checking '{0}'. Please open an issue in our bug tracker so we address this. There is a pre-filled template that you can use in '", source_path.display());
        let message = actual["message"].as_str().unwrap();
        let path = PathBuf::from(
            message
                .strip_prefix(&prefix)
                .unwrap()
                .strip_suffix("'.")
                .unwrap(),
        );
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
        assert!((1..=12).contains(&numbers[1]) && (1..=31).contains(&numbers[2]));
        assert!(numbers[3] < 24 && numbers[4] < 60 && numbers[5] < 60);
        let directory = path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        let random = directory.strip_prefix("mamba-quality-pylint-").unwrap();
        assert_eq!(random.len(), 6);
        assert!(random.bytes().all(|byte| byte.is_ascii_alphanumeric()));
        // The child has an empty environment. These macOS/Linux errors use C text.
        let denied = format!(
            "{}: Permission denied (os error 13)",
            root.join("src/pkg").display()
        );
        let bytes = log_bytes(root, &path, outside, 2);
        let contents = format!(
            "Pinned Pylint 4.1.2 / Astroid 4.3.3 build failure\n\
             Native Rust reproduction: isort source classification at Pylint module exit.\n\
             Reference exception: PermissionError: {denied}\n\
             Source: {}\nSHA-256: {}\n\n```python\n{}\n```\n",
            source_path.display(),
            case["source_sha256"]["main.py"].as_str().unwrap(),
            case["files"]["main.py"].as_str().unwrap()
        );
        assert_eq!(bytes, contents.as_bytes());
        assert!(saved.replace(path).is_none());
    }
    assert_eq!(saved.is_some(), fatal);
    saved
}

#[test]
fn check_formatted_invalid_input_keeps_exact_original_output_log() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../mamba-quality/src/infrastructure/black/tests/interpolation_comments.json"
    ))
    .unwrap();
    let case = cases
        .iter()
        .find(|case| case["name"] == "f_inner_comment_fmt_skip")
        .unwrap();
    let source = case["source"].as_str().unwrap();
    assert_eq!(sha(source.as_bytes()), case["source_sha256"]);
    assert_eq!(
        case["source_sha256"],
        "761bab92f3601e0262a50ff9fa565fa9e3dd25e100e13a62ee6068eabfb3bfbd"
    );
    for (target, version) in VERSIONS.into_iter().enumerate() {
        let expected = &case["expected"][target];
        assert_eq!(expected["kind"], "error");
        assert_eq!(expected["exception_type"], "black.parsing.InvalidInput");
        assert_eq!(expected["reference_exit"], 123);
        let hash = case["invalid_output_sha256"][target].as_str().unwrap();
        assert_eq!(
            hash,
            "941a8d71f6521a57279b17ac5f9670490d941f672190b715cb7bd600b33fd9ed"
        );
        let root = fs::canonicalize(project(version, source).keep()).unwrap();
        let value = check(&root, None, false);
        assert_eq!(value["projects"][0]["python_version"], version);
        let redirected = invalid_output_log(&root, &value, expected, hash, None);
        let outside = tempfile::Builder::new()
            .prefix("quality-cli-outside-")
            .tempdir()
            .unwrap()
            .keep();
        let control = check(&root, Some(&outside), false);
        let original = invalid_output_log(&root, &control, expected, hash, Some(&outside));
        assert_eq!(fs::read(&redirected).unwrap(), fs::read(&original).unwrap());
        assert_eq!(
            with_saved_log_path(
                &value,
                "black",
                "black.parsing.InvalidInput",
                &redirected,
                &original,
                ""
            ),
            control
        );
        assert_eq!(fs::read(root.join("main.py")).unwrap(), source.as_bytes());
    }
}

#[test]
fn check_import_permission_log_and_python314_nonfatal_control() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../mamba-quality/src/infrastructure/pylint/tests/dependency_io.json"
    ))
    .unwrap();
    let case = cases
        .iter()
        .find(|case| case["name"] == "src_permission_original")
        .unwrap();
    assert_eq!(case["restricted"], "src");
    assert_eq!(case["inputs"], json!(["main.py"]));
    assert_eq!(case["reference_exits"], json!([3, 3, 2]));
    for (target, version) in VERSIONS.into_iter().enumerate() {
        let root =
            fs::canonicalize(project(version, case["files"]["main.py"].as_str().unwrap()).keep())
                .unwrap();
        for (name, source) in case["files"].as_object().unwrap() {
            let source = source.as_str().unwrap();
            assert_eq!(sha(source.as_bytes()), case["source_sha256"][name]);
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, source).unwrap();
        }
        let value = check(&root, None, true);
        assert_eq!(value["projects"][0]["python_version"], version);
        let redirected = permission_log(&root, &value, case, target, None);
        let outside = tempfile::Builder::new()
            .prefix("quality-cli-outside-")
            .tempdir()
            .unwrap()
            .keep();
        let control = check(&root, Some(&outside), true);
        let original = permission_log(&root, &control, case, target, Some(&outside));
        if target < 2 {
            let redirected = redirected.unwrap();
            let original = original.unwrap();
            assert_eq!(fs::read(&redirected).unwrap(), fs::read(&original).unwrap());
            assert_eq!(
                with_saved_log_path(&value, "pylint", "F0002", &redirected, &original, "'."),
                control
            );
        } else {
            assert!(redirected.is_none() && original.is_none());
            assert_eq!(value, control);
        }
        for (name, source) in case["files"].as_object().unwrap() {
            assert_eq!(
                fs::read(root.join(name)).unwrap(),
                source.as_str().unwrap().as_bytes()
            );
        }
    }
}
