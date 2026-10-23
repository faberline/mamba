//! Captured second-pass fixed points keep the one-pass catalog intact.
use crate::domain::{
    model::{CompletionStatus, QualityIssue, QualityTool, ToolReport, TransformResult},
    ports::{ProjectStore, QualityEngine},
    profile::FixedProfile,
    source::{PythonVersion, QualityWorkspace},
};
use crate::infrastructure::{filesystem::Filesystem, native::NativeEngine};
use mamba_language::application::quality_syntax;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Write, path::Path};

const FIXTURE_SHA256: &str = "2d03da18d943d9bb102120f8bd59975f4dbba97c681c1d20cff0fba44932223f";

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn decode(value: &Value) -> Vec<u8> {
    let hex = value.as_str().unwrap();
    assert!(hex.len().is_multiple_of(2));
    (0..hex.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap())
        .collect()
}

fn encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn version(name: &str) -> PythonVersion {
    match name {
        "3.12" => PythonVersion::Py312,
        "3.13" => PythonVersion::Py313,
        "3.14" => PythonVersion::Py314,
        _ => panic!("Unknown protected target {name}"),
    }
}

fn with_source(workspace: &QualityWorkspace, text: &str, bytes: &[u8]) -> QualityWorkspace {
    let mut copy = workspace.clone();
    copy.sources[0].text = text.into();
    copy.sources[0].bytes = bytes.into();
    copy.sources[0].sha256 = digest(bytes);
    copy
}

fn transform_output(result: &TransformResult, input: &str) -> Option<String> {
    if !result.issues.is_empty() {
        return None;
    }
    match result.candidates.as_slice() {
        [] => Some(input.into()),
        [(index, output)] if *index == 0 => Some(output.clone()),
        _ => None,
    }
}

fn transform_record(result: &TransformResult) -> Value {
    json!({
        "completion":result.completion,
        "candidates":result.candidates,
        "issues":result.issues,
        "remaining":result.remaining,
    })
}

fn encoded_candidate(
    workspace: &QualityWorkspace,
    output: Option<&str>,
) -> (Option<Vec<u8>>, Option<QualityIssue>) {
    match output {
        Some(output) => match Filesystem.candidate_bytes(&workspace.sources[0], output) {
            Ok(bytes) => (Some(bytes), None),
            Err(issue) => (None, Some(issue)),
        },
        None => (None, None),
    }
}

fn raw_format(workspace: &QualityWorkspace, profile: &FixedProfile) -> Value {
    match super::super::format(&workspace.sources[0], workspace, profile) {
        Ok((output, remaining)) => json!({
            "output":output,
            "output_hex":encode(output.as_bytes()),
            "remaining":remaining,
            "error":null,
        }),
        Err(issue) => json!({"output":null,"output_hex":null,"remaining":[],"error":issue}),
    }
}

fn syntax(workspace: &QualityWorkspace, text: &str) -> Value {
    match quality_syntax::parse(
        text,
        crate::infrastructure::native::target(workspace, &workspace.sources[0]),
        workspace.sources[0].is_stub,
    ) {
        Ok(document) => json!({
            "valid":document.is_valid(),
            "error":null,
            "issues":document.issues.iter().map(|issue| json!({
                "message":issue.message,"start":issue.start,"end":issue.end,
                "version_error":issue.version_error,
            })).collect::<Vec<_>>(),
        }),
        Err(error) => json!({"valid":null,"error":error,"issues":[]}),
    }
}

fn validation(workspace: &QualityWorkspace, output: &str, structure: bool) -> Value {
    match NativeEngine.validate(&workspace.sources[0], output, workspace, structure) {
        Ok(()) => json!({"ok":true,"error":null}),
        Err(issue) => json!({"ok":false,"error":issue}),
    }
}

fn expected_public(case: &Value, path: &Path, reference_check_exit: i64) -> Value {
    let contract = &case["native_public_contract"];
    let diagnostics = if reference_check_exit == 1 {
        let mut diagnostic = contract["changed_diagnostic"].clone();
        diagnostic["path"] = json!(path);
        vec![diagnostic]
    } else {
        assert_eq!(reference_check_exit, 0);
        Vec::new()
    };
    json!({
        "tool":"isort","reference_version":"9.0.2",
        "completion":contract["completion"],
        "reference_exit_code":contract["reference_exit_code"],
        "checked_files":contract["checked_files"],
        "diagnostics":diagnostics,"remaining":contract["remaining"],
    })
}

fn public_record(report: &ToolReport) -> Value {
    serde_json::to_value(report).unwrap()
}

#[test]
fn pinned_original_fixed_points() {
    let fixture = include_str!("fixed_points.json");
    assert_eq!(digest(fixture.as_bytes()), FIXTURE_SHA256);
    let corpus: Value = serde_json::from_str(fixture).unwrap();
    let cases = corpus["cells"].as_array().unwrap();
    assert_eq!(cases.len(), 108);
    assert_eq!(corpus["source_count"], 36);
    let mut identities = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut rows = Vec::new();
    let mut failures = 0;
    let profile = FixedProfile::locked();

    for (number, case) in cases.iter().enumerate() {
        assert_eq!(case["number"], number);
        let id = case["id"].as_str().unwrap();
        let python = case["python"].as_str().unwrap();
        assert!(identities.insert((id.to_owned(), python.to_owned())));
        sources.insert(id.to_owned());
        let source_bytes = decode(&case["source_hex"]);
        let old_bytes = decode(&case["old_first_pass_hex"]);
        let stable_bytes = decode(&case["stable_hex"]);
        assert_eq!(digest(&source_bytes), case["source_sha256"]);
        assert_eq!(digest(&old_bytes), case["old_first_pass_sha256"]);
        assert_eq!(digest(&stable_bytes), case["stable_sha256"]);
        let source = String::from_utf8(source_bytes.clone()).unwrap();
        let old = String::from_utf8(old_bytes.clone()).unwrap();
        let stable = String::from_utf8(stable_bytes.clone()).unwrap();
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        assert!(case["files"].as_object().unwrap().is_empty());
        assert!(case["directories"].as_array().unwrap().is_empty());
        let path = root_path.join(format!("main.{}", case["extension"].as_str().unwrap()));
        fs::write(&path, &source_bytes).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        assert_eq!(workspace.sources.len(), 1);
        workspace.projects[0].python_version = version(python);
        assert_eq!(workspace.sources[0].text, source);
        assert_eq!(workspace.sources[0].bytes, source_bytes);
        let source_before = fs::read(&path).unwrap();

        // This observation preserves the existing one-pass oracle. It is not
        // the new stable-output pass condition. The old catalog remains separate.
        let first_pass = raw_format(&workspace, &profile);
        let public_source = NativeEngine.check(&workspace, &profile, QualityTool::Isort);
        let transform = NativeEngine.transform(&workspace, &profile, QualityTool::Isort);
        let output = transform_output(&transform, &source);
        let (bytes, encoding_error) = encoded_candidate(&workspace, output.as_deref());
        let output_matches = bytes.as_deref() == Some(stable_bytes.as_slice());
        let transform_matches = output_matches
            && output.as_deref() == Some(stable.as_str())
            && transform.completion == CompletionStatus::Incomplete
            && transform.issues.is_empty()
            && json!(transform.remaining) == case["native_public_contract"]["remaining"]
            && transform.candidates.len() == 1
            && transform.candidates[0].0 == 0;

        let mut repeated_output = None;
        let mut repeated_bytes = None;
        let mut repeated_encoding_error = None;
        let mut repeat_record = Value::Null;
        let mut repeat_first_pass = Value::Null;
        let mut public_candidate = Value::Null;
        let mut repeat_completion_matches = false;
        let mut repeat_issues_empty = false;
        if let (Some(output), Some(bytes)) = (output.as_ref(), bytes.as_ref()) {
            let repeated_workspace = with_source(&workspace, output, bytes);
            repeat_first_pass = raw_format(&repeated_workspace, &profile);
            public_candidate = public_record(&NativeEngine.check(
                &repeated_workspace,
                &profile,
                QualityTool::Isort,
            ));
            let repeated =
                NativeEngine.transform(&repeated_workspace, &profile, QualityTool::Isort);
            repeated_output = transform_output(&repeated, output);
            (repeated_bytes, repeated_encoding_error) =
                encoded_candidate(&repeated_workspace, repeated_output.as_deref());
            repeat_completion_matches = repeated.completion == CompletionStatus::Incomplete;
            repeat_issues_empty = repeated.issues.is_empty()
                && repeated.candidates.is_empty()
                && json!(repeated.remaining) == case["native_public_contract"]["remaining"];
            repeat_record = transform_record(&repeated);
        }
        let repeat_matches = repeated_bytes.as_deref() == Some(stable_bytes.as_slice())
            && repeated_output.as_deref() == Some(stable.as_str())
            && repeated_bytes == bytes
            && repeat_completion_matches
            && repeat_issues_empty;

        let old_workspace = with_source(&workspace, &old, &old_bytes);
        let stable_workspace = with_source(&workspace, &stable, &stable_bytes);
        let public_old = NativeEngine.check(&old_workspace, &profile, QualityTool::Isort);
        let public_stable = NativeEngine.check(&stable_workspace, &profile, QualityTool::Isort);
        let expected_source = expected_public(
            case,
            &path,
            case["original_actual_exits"]["source_check"]
                .as_i64()
                .unwrap(),
        );
        let expected_old = expected_public(
            case,
            &path,
            case["original_actual_exits"]["old_first_pass_check"]
                .as_i64()
                .unwrap(),
        );
        let expected_stable = expected_public(
            case,
            &path,
            case["original_actual_exits"]["stable_check"]
                .as_i64()
                .unwrap(),
        );
        let public_matches = public_record(&public_source) == expected_source
            && public_record(&public_old) == expected_old
            && public_record(&public_stable) == expected_stable
            && public_candidate == expected_stable;
        let output_syntax = output.as_ref().map(|output| syntax(&workspace, output));
        let syntax_matches = output_syntax
            .as_ref()
            .is_some_and(|syntax| syntax["valid"] == true);
        let source_after = fs::read(&path).unwrap();
        let preserved = source_before == source_after
            && source_after == source_bytes
            && workspace.sources[0].text == source
            && workspace.sources[0].bytes == source_bytes;
        let matches =
            transform_matches && repeat_matches && public_matches && syntax_matches && preserved;
        failures += usize::from(!matches);
        let mut row = json!({
            "number":number,"id":id,"python":python,"case_index":case["case_index"],
            "source_sha256":case["source_sha256"],"source_hex":encode(&source_bytes),
            "old_first_pass_hex":encode(&old_bytes),"stable_expected_hex":encode(&stable_bytes),
            "original_actual_exits":case["original_actual_exits"],"provenance":case["provenance"],
            "raw_one_pass":first_pass,"raw_repeat_one_pass":repeat_first_pass,
            "transform":transform_record(&transform),"transform_output":output,
            "transform_output_hex":bytes.as_ref().map(|bytes| encode(bytes)),
            "transform_encoding_error":encoding_error,"repeat_transform":repeat_record,
            "repeat_output":repeated_output,"repeat_output_hex":repeated_bytes.as_ref().map(|bytes| encode(bytes)),
            "repeat_encoding_error":repeated_encoding_error,
        });
        let fields = json!({
            "public_source":public_record(&public_source),"public_old_first_pass":public_record(&public_old),
            "public_stable_reference":public_record(&public_stable),"public_native_candidate":public_candidate,
            "expected_public_source":expected_source,"expected_public_old_first_pass":expected_old,
            "expected_public_stable":expected_stable,
            "source_syntax":syntax(&workspace,&source),"old_first_pass_syntax":syntax(&workspace,&old),
            "stable_reference_syntax":syntax(&workspace,&stable),"native_output_syntax":output_syntax,
            "syntax_validation":output.as_ref().map(|output| validation(&workspace,output,false)),
            "strict_ast_source_to_native":output.as_ref().map(|output| validation(&workspace,output,true)),
            "strict_ast_old_first_pass_to_stable":validation(&old_workspace,&stable,true),
            "strict_ast_source_to_stable":validation(&workspace,&stable,true),
            "source_before_hex":encode(&source_before),"source_after_hex":encode(&source_after),
            "source_preserved":preserved,"transform_matches_stable":transform_matches,
            "repeat_matches_stable":repeat_matches,"public_fields_match":public_matches,
            "syntax_valid":syntax_matches,"matches":matches,
        });
        row.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        rows.push(row);
    }
    assert_eq!(identities.len(), 108);
    assert_eq!(sources.len(), 36);
    if let Some(destination) = std::env::var_os("MAMBA_QUALITY_ISORT_FIXED_POINTS_EVIDENCE") {
        let evidence = serde_json::to_vec_pretty(&json!({
            "schema":"mamba.quality.isort-fixed-points-native-comparison.v1",
            "fixture_sha256":FIXTURE_SHA256,"cases":rows,"failed":failures,
            "expected_cells":108,"expected_sources":36,
            "whole_isort_accepted":false,"whole_plan_accepted":false,
        }))
        .unwrap();
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .unwrap()
            .write_all(&evidence)
            .unwrap();
    }
    assert_eq!(rows.len(), 108);
    assert_eq!(
        failures, 0,
        "Protected stable isort outputs or public fields differ"
    );
}
