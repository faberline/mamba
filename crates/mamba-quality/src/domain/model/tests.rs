//! Information must survive serialization without changing quality exit status.
use super::*;

#[test]
fn information_preserves_the_zero_one_two_exit_contract() {
    let diagnostic: QualityDiagnostic = serde_json::from_value(serde_json::json!({
        "tool":"pylint", "rule":"I0013", "message":"Ignoring entire file", "hint":null,
        "path":"main.py", "severity":"info", "fix":null,
        "location":{"line":3,"column":0,"end_line":null,"end_column":null,"offset":null,"end_offset":null}
    })).unwrap();
    let mut report = QualityReport {
        schema: REPORT_SCHEMA.into(),
        operation: QualityOperation::Check,
        profile: FixedProfile::locked(),
        projects: Vec::new(),
        tools: QualityTool::ALL
            .into_iter()
            .map(|tool| ToolReport {
                tool,
                reference_version: tool.reference_version().into(),
                completion: CompletionStatus::Complete,
                reference_exit_code: Some(0),
                checked_files: 1,
                diagnostics: if tool == QualityTool::Pylint {
                    vec![diagnostic.clone()]
                } else {
                    Vec::new()
                },
                remaining: Vec::new(),
            })
            .collect(),
        issues: Vec::new(),
        changes: Vec::new(),
        files: FileCounts::default(),
        analysis_completed: false,
        passed: false,
        exit_code: 2,
    };
    report.finish();
    assert_eq!(report.exit_code, 0);
    assert!(report.passed && report.analysis_completed);
    let encoded = serde_json::to_value(&report).unwrap();
    assert_eq!(encoded["schema"], REPORT_SCHEMA);
    assert_eq!(
        encoded["tools"][3]["diagnostics"][0]["message"],
        "Ignoring entire file"
    );
    let pylint = report
        .tools
        .iter_mut()
        .find(|tool| tool.tool == QualityTool::Pylint)
        .unwrap();
    pylint.completion = CompletionStatus::Incomplete;
    pylint.reference_exit_code = None;
    report.finish();
    assert_eq!(report.exit_code, 2);
    assert!(!report.passed && !report.analysis_completed);
    let pylint = report
        .tools
        .iter_mut()
        .find(|tool| tool.tool == QualityTool::Pylint)
        .unwrap();
    pylint.completion = CompletionStatus::Complete;
    pylint.diagnostics[0].severity = "warning".into();
    pylint.reference_exit_code = Some(4);
    report.finish();
    assert_eq!(report.exit_code, 1);
    assert!(!report.passed && report.analysis_completed);
    report.tools[3].diagnostics.clear();
    report.tools[3].reference_exit_code = Some(0);
    let mut note = diagnostic;
    note.tool = QualityTool::Mypy;
    note.rule.clear();
    note.message = "note".into();
    note.severity = "note".into();
    report.tools[2].diagnostics.push(note);
    report.finish();
    assert_eq!(report.exit_code, 0);
    assert!(report.passed && report.analysis_completed);
    assert_eq!(
        serde_json::to_value(&report).unwrap()["tools"][2]["diagnostics"][0]["rule"],
        serde_json::Value::Null
    );
}
