//! Preserve the raw output of each fatal-log CLI control.
use super::*;

pub(super) fn record(
    root: &Path,
    arguments: &[&str],
    temporary: &Path,
    restricted: bool,
    output: &std::process::Output,
) {
    let directory = if let Some(parent) = std::env::var_os("MAMBA_QUALITY_CLI_FATAL_EVIDENCE") {
        let parent = fs::canonicalize(parent).unwrap();
        assert!(!parent.starts_with(fs::canonicalize(root).unwrap()));
        tempfile::Builder::new()
            .prefix("cell-")
            .tempdir_in(parent)
            .unwrap()
    } else {
        tempfile::Builder::new()
            .prefix("quality-cli-fatal-capture-")
            .tempdir()
            .unwrap()
    };
    let directory = directory.keep();
    fs::write(directory.join("stdout.raw"), &output.stdout).unwrap();
    fs::write(directory.join("stderr.raw"), &output.stderr).unwrap();
    let mut sources = BTreeMap::new();
    for filename in ["main.py", "probe_003.py", "pyproject.toml"] {
        if let Ok(bytes) = fs::read(root.join(filename)) {
            sources.insert(filename, bytes);
        }
    }
    let metadata = json!({
        "schema": "mamba.quality.cli-fatal-control.v1",
        "binary": env!("CARGO_BIN_EXE_mamba"),
        "cwd": root,
        "arguments": arguments,
        "tmpdir": temporary,
        "private_src_restricted_while_child_ran": restricted,
        "actual_exit_code": output.status.code(),
        "source_bytes_after_child": sources,
        "whole_plan_accepted": false,
    });
    fs::write(
        directory.join("process.json"),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .unwrap();
    eprintln!("Retained fatal-log CLI output: {}", directory.display());
}
