//! Compare public diagnostics and retain explicit declined transformation errors.
use super::*;

pub(super) fn diagnostics(report: &ToolReport, root: &Path) -> Value {
    Value::Array(report.diagnostics.iter().map(|message| json!({
        "rule":message.rule,"message":message.message,
        "path":message.path.strip_prefix(root).unwrap(),"severity":message.severity,
        "location":{"line":message.location.line,"column":message.location.column,
            "end_line":message.location.end_line,"end_column":message.location.end_column},
        "fix":message.fix,
    })).collect())
}

pub(super) fn errors(
    result: &TransformResult,
    workspace: &QualityWorkspace,
    root: &Path,
    cases: &BTreeMap<String, Value>,
    target: usize,
) -> Vec<Value> {
    assert_eq!(
        result.completion == CompletionStatus::Error,
        !result.issues.is_empty()
    );
    result
        .issues
        .iter()
        .map(|issue| {
            let path = issue.path.as_ref().unwrap();
            let name = path.strip_prefix(root).unwrap().to_str().unwrap();
            let source = workspace
                .sources
                .iter()
                .find(|source| source.path == *path)
                .unwrap();
            let expected = source.decoding_issue.is_some()
                || cases[name]["expected"][target]["before"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|message| {
                        matches!(message["rule"].as_str(), Some("invalid-syntax" | "E902"))
                    });
            json!({"path":name,"code":issue.code,
            "message":issue.message.replace(root.to_str().unwrap(), ""),"expected":expected})
        })
        .collect()
}
