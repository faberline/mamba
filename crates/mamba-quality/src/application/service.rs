use crate::domain::{
    model::*,
    ports::{ProjectStore, QualityEngine},
    profile::FixedProfile,
    source::QualityWorkspace,
};
use sha2::{Digest, Sha256};
use std::{borrow::Cow, collections::BTreeMap};

#[cfg(test)]
mod tests;

pub(crate) fn execute(
    request: &QualityRequest,
    store: &dyn ProjectStore,
    engine: &dyn QualityEngine,
) -> QualityReport {
    let workspace = store.load(&request.paths, request.operation != QualityOperation::Info);
    let mut report = QualityReport {
        schema: REPORT_SCHEMA.into(),
        operation: request.operation,
        profile: FixedProfile::locked(),
        projects: workspace.projects.clone(),
        tools: Vec::new(),
        issues: workspace.issues.clone(),
        changes: Vec::new(),
        files: FileCounts {
            discovered: workspace.discovered,
            ..FileCounts::default()
        },
        analysis_completed: false,
        passed: false,
        exit_code: 2,
    };
    let transformations = match request.operation {
        QualityOperation::Fix { .. } => {
            vec![QualityTool::Ruff, QualityTool::Isort, QualityTool::Black]
        }
        QualityOperation::Format { .. } => vec![QualityTool::Isort, QualityTool::Black],
        _ => Vec::new(),
    };
    // Check and info share the immutable snapshot. Only a transformation needs
    // a separate candidate workspace.
    let mut working = Cow::Borrowed(&workspace);
    let mut touched: BTreeMap<usize, Vec<QualityTool>> = BTreeMap::new();
    let mut failed_transforms = BTreeMap::new();
    if !transformations.is_empty() {
        for iteration in 0..8 {
            let changed = Pipeline {
                profile: &report.profile,
                store,
                engine,
                issues: &mut report.issues,
                failures: &mut failed_transforms,
                touched: &mut touched,
            }
            .transform(working.to_mut(), &transformations);
            let stopped = failed_transforms.values().any(|(status, _)| {
                matches!(
                    status,
                    CompletionStatus::Error | CompletionStatus::Unsupported
                )
            });
            if !changed || stopped || !report.issues.is_empty() {
                break;
            }
            if iteration == 7 {
                report.issues.push(QualityIssue::new(
                    "fix-convergence",
                    "The quality pipeline did not converge after eight passes",
                    None,
                ));
            }
        }
        for (index, tools) in touched {
            let before = &workspace.sources[index];
            let after = &working.sources[index];
            if before.bytes == after.bytes {
                continue;
            }
            let diff = match request.operation {
                QualityOperation::Fix { diff: true }
                | QualityOperation::Format { diff: true, .. } => Some(
                    similar::TextDiff::from_lines(&before.text, &after.text)
                        .unified_diff()
                        .header(
                            &format!("a/{}", before.path.display()),
                            &format!("b/{}", before.path.display()),
                        )
                        .to_string(),
                ),
                _ => None,
            };
            report.changes.push(FileChange {
                path: before.path.clone(),
                original_sha256: before.sha256.clone(),
                candidate_sha256: after.sha256.clone(),
                applied: false,
                diff,
                tools,
            });
        }
    }
    let check_original = matches!(
        request.operation,
        QualityOperation::Format { check: true, .. }
    );
    let check_workspace = if check_original { &workspace } else { &working };
    for tool in QualityTool::ALL {
        let requested = request.operation != QualityOperation::Info
            && (request.operation.checks()
                || matches!(tool, QualityTool::Isort | QualityTool::Black));
        let mut tool_report = if requested {
            engine.check(check_workspace, &report.profile, tool)
        } else {
            ToolReport {
                tool,
                reference_version: tool.reference_version().into(),
                completion: CompletionStatus::NotRequested,
                reference_exit_code: None,
                checked_files: 0,
                diagnostics: Vec::new(),
                remaining: if request.operation == QualityOperation::Info {
                    engine.remaining(tool)
                } else {
                    Vec::new()
                },
            }
        };
        if let Some((status, remaining)) = failed_transforms.get(&tool) {
            tool_report.completion = *status;
            for item in remaining {
                if !tool_report.remaining.contains(item) {
                    tool_report.remaining.push(item.clone());
                }
            }
        }
        report.tools.push(tool_report);
    }
    let checks_completed = report.tools.iter().all(|tool| {
        matches!(
            tool.completion,
            CompletionStatus::Complete | CompletionStatus::NotRequested
        )
    });
    if request.operation.writes()
        && failed_transforms.is_empty()
        && report.issues.is_empty()
        && checks_completed
    {
        for change in &mut report.changes {
            let index = workspace
                .sources
                .iter()
                .position(|source| source.path == change.path)
                .unwrap();
            match store.publish(&workspace.sources[index], &working.sources[index].bytes) {
                Ok(()) => {
                    change.applied = true;
                    report.files.written += 1;
                }
                Err(failure) => {
                    if failure.applied {
                        change.applied = true;
                        report.files.written += 1;
                    }
                    report.issues.push(failure.issue);
                }
            }
        }
    }
    report.files.changed = report.changes.len();
    report.files.checked = report
        .tools
        .iter()
        .map(|tool| tool.checked_files)
        .max()
        .unwrap_or(0);
    report.finish();
    report
}

struct Pipeline<'a> {
    profile: &'a FixedProfile,
    store: &'a dyn ProjectStore,
    engine: &'a dyn QualityEngine,
    issues: &'a mut Vec<QualityIssue>,
    failures: &'a mut BTreeMap<QualityTool, (CompletionStatus, Vec<String>)>,
    touched: &'a mut BTreeMap<usize, Vec<QualityTool>>,
}

impl Pipeline<'_> {
    fn transform(&mut self, workspace: &mut QualityWorkspace, tools: &[QualityTool]) -> bool {
        let mut changed = false;
        for tool in tools {
            let result = self.engine.transform(workspace, self.profile, *tool);
            if result.completion != CompletionStatus::Complete {
                self.failures
                    .insert(*tool, (result.completion, result.remaining));
            }
            self.issues.extend(result.issues);
            for (index, candidate) in result.candidates {
                let source = &workspace.sources[index];
                if source.text == candidate {
                    continue;
                }
                if let Err(issue) =
                    self.engine
                        .validate(source, &candidate, workspace, *tool == QualityTool::Black)
                {
                    self.issues.push(issue);
                    continue;
                }
                match self.store.candidate_bytes(source, &candidate) {
                    Ok(bytes) => {
                        let source = &mut workspace.sources[index];
                        source.sha256 = format!("{:x}", Sha256::digest(&bytes));
                        source.bytes = bytes;
                        source.text = candidate;
                        let trace = self.touched.entry(index).or_default();
                        if !trace.contains(tool) {
                            trace.push(*tool);
                        }
                        changed = true;
                    }
                    Err(issue) => self.issues.push(issue),
                }
            }
        }
        changed
    }
}
