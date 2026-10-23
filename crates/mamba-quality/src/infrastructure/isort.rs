//! Import sorting uses the locked isort standard-library data and Black profile.
mod blocks;
mod comments;
mod directives;
mod invalid;
mod literals;
mod model;
mod placement;
mod render;
pub(super) mod sorting;
#[cfg(test)]
mod tests;

use crate::domain::{
    model::*,
    profile::FixedProfile,
    source::{QualityWorkspace, SourceSnapshot},
};
use mamba_language::application::quality_syntax::{
    self,
    python_ast::{
        visitor::{walk_stmt, Visitor},
        Alias, Stmt,
    },
};
use model::{Binding, Import};
use ruff_text_size::Ranged;
use sha2::{Digest, Sha256};

#[derive(Default)]
struct Imports<'a> {
    statements: Vec<&'a Stmt>,
}
impl<'a> Visitor<'a> for Imports<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if matches!(statement, Stmt::Import(_) | Stmt::ImportFrom(_)) {
            self.statements.push(statement);
        }
        walk_stmt(self, statement);
    }
}

fn line_start(source: &str, offset: usize) -> usize {
    source[..offset]
        .rfind(['\r', '\n'])
        .map_or(0, |index| index + 1)
}
fn line_end(source: &str, offset: usize) -> usize {
    source[offset..]
        .find(['\r', '\n'])
        .map_or(source.len(), |index| {
            let index = offset + index;
            index
                + if source[index..].starts_with("\r\n") {
                    2
                } else {
                    1
                }
        })
}

fn raw_name(name: &impl Ranged, source: &str) -> String {
    source[name.range().start().to_usize()..name.range().end().to_usize()]
        .chars()
        .filter(|character| !character.is_whitespace() && *character != '\\')
        .collect()
}

fn bindings(names: &[Alias], source: &str) -> Vec<Binding> {
    names
        .iter()
        .map(|name| Binding {
            name: raw_name(&name.name, source),
            alias: name.asname.as_ref().map(|name| raw_name(name, source)),
            comment: String::new(),
            opening_comment: String::new(),
        })
        .collect()
}

fn trailing_comma(document: &quality_syntax::SyntaxDocument<'_>, start: usize, end: usize) -> bool {
    let code: Vec<_> = document
        .tokens
        .iter()
        .filter(|token| {
            token.start as usize >= start
                && token.end as usize <= end
                && token.kind != "Comment"
                && !document.text(token).trim().is_empty()
        })
        .map(|token| document.text(token))
        .collect();
    code.ends_with(&[",", ")"])
}

fn split_marker_is_nonterminal_comment(line: &str) -> bool {
    let comment = line.trim();
    let mut markers = comment.split("isort:").skip(1).peekable();
    markers.peek().is_some()
        && markers.all(|marker| marker.trim_start().starts_with("split"))
        && !comment.ends_with("# isort: split")
}

fn format(
    source: &SourceSnapshot,
    workspace: &QualityWorkspace,
    profile: &FixedProfile,
) -> Result<(String, Vec<String>), QualityIssue> {
    if let Some(issue) = &source.decoding_issue {
        return Err(issue.clone());
    }
    let text = &source.text;
    if directives::skip_file(text) {
        return Ok((text.clone(), Vec::new()));
    }
    let parsed = quality_syntax::parse(
        text,
        super::native::target(workspace, source),
        source.is_stub,
    )
    .map_err(|message| QualityIssue::new("isort-syntax", message, Some(source.path.clone())))?;
    if !parsed.is_valid() {
        return invalid::format(source, workspace, profile, &parsed);
    }
    let mut collector = Imports::default();
    for statement in &parsed.parsed.syntax().body {
        collector.visit_stmt(statement);
    }
    collector
        .statements
        .sort_by_key(|statement| statement.range().start());
    let newline = model::newline(text);
    let mut blocks: Vec<blocks::Block> = Vec::new();
    let mut edits = literals::edits(&parsed, source.is_stub, &source.path)?;
    let remaining = Vec::new();
    for statement in collector.statements {
        let start = statement.range().start().to_usize();
        let end = statement.range().end().to_usize();
        let physical_start = line_start(text, start);
        let mut physical_end = line_end(text, end);
        while physical_end < text.len() {
            let previous = &text[line_start(
                text,
                physical_end.saturating_sub(if text[..physical_end].ends_with("\r\n") {
                    2
                } else {
                    1
                }),
            )..physical_end];
            if !previous
                .split('#')
                .next()
                .unwrap_or_default()
                .trim_end()
                .ends_with('\\')
            {
                break;
            }
            physical_end = line_end(text, physical_end);
        }
        if !directives::enabled(&parsed, physical_start, physical_end) {
            let line = &text[physical_start..physical_end];
            if directives::enabled(&parsed, physical_start, physical_start)
                && (line.contains("isort:skip") || line.contains("isort: skip"))
                && text[physical_end..].trim().is_empty()
                && text[physical_start..start].trim().is_empty()
                && start == physical_start
            {
                let replacement = if line.ends_with(['\n', '\r']) {
                    line.to_owned()
                } else {
                    line.to_owned() + newline
                };
                edits.push((physical_start, text.len(), replacement));
            }
            continue;
        }
        let physical_code = text[physical_start..physical_end]
            .split('#')
            .next()
            .unwrap_or_default();
        let pure_imports = physical_code.split(';').all(|part| {
            let part = part.trim();
            part.is_empty()
                || part.starts_with("from ")
                || part.starts_with("from.import")
                || part.starts_with("import ")
        });
        if !pure_imports {
            continue;
        }
        let indent = &physical_code[..physical_code.len() - physical_code.trim_start().len()];
        let mut unit = match statement {
            Stmt::Import(statement) => Import {
                module: String::new(),
                from: false,
                names: bindings(&statement.names, text),
                leading: Vec::new(),
                comment: String::new(),
                multiline: false,
                section: 0,
            },
            Stmt::ImportFrom(statement) => Import {
                module: format!(
                    "{}{}",
                    ".".repeat(statement.level as usize),
                    statement
                        .module
                        .as_ref()
                        .map(|name| raw_name(name, text))
                        .unwrap_or_default()
                ),
                from: true,
                names: bindings(&statement.names, text),
                leading: Vec::new(),
                comment: String::new(),
                multiline: trailing_comma(&parsed, start, end),
                section: 0,
            },
            _ => unreachable!(),
        };
        let comment_end = if text[end..physical_end].trim_start().starts_with(';') {
            end
        } else {
            physical_end
        };
        comments::collect(&parsed, start, comment_end, &mut unit);
        let join = blocks.last().is_some_and(|block| {
            block.indent == indent
                && (block.end >= physical_start
                    || text[block.end..physical_start].lines().all(|line| {
                        line.trim().is_empty()
                            || (line.trim_start().starts_with('#')
                                && (!line.contains("isort:")
                                    || split_marker_is_nonterminal_comment(line)
                                    || line.contains("isort: imports-")
                                    || line.contains("isort:imports-")))
                    }))
        });
        if !join {
            let (block_start, leading) = if indent.is_empty() {
                blocks::preceding(&parsed, physical_start)
            } else {
                (physical_start, Vec::new())
            };
            unit.leading = leading;
            blocks.push(blocks::Block {
                start: block_start,
                end: physical_end,
                indent: indent.into(),
                imports: Vec::new(),
                residual: Vec::new(),
            });
        }
        let block = blocks.last_mut().unwrap();
        if join && block.end <= physical_start {
            unit.leading = blocks::gap(block, &text[block.end..physical_start]);
        }
        block.end = physical_end;
        if unit.from {
            unit.section =
                model::section(&unit.module, &workspace.projects[source.project], profile);
            block.imports.push(unit);
        } else {
            for (index, name) in unit.names.iter().enumerate() {
                block.imports.push(Import {
                    module: name.name.clone(),
                    from: false,
                    names: vec![name.clone()],
                    leading: if index == 0 {
                        unit.leading.clone()
                    } else {
                        Vec::new()
                    },
                    comment: name.comment.clone(),
                    multiline: false,
                    section: model::section(
                        &name.name,
                        &workspace.projects[source.project],
                        profile,
                    ),
                });
            }
        }
    }
    let mut limit = text.len();
    for block in blocks.into_iter().rev() {
        let next_start = block.start;
        let (start, end, rendered) = blocks::output(
            block,
            &parsed,
            limit,
            newline,
            source.is_stub,
            workspace.projects[source.project].python_version,
        );
        edits.push((start, end, rendered));
        limit = next_start;
    }
    edits.sort_by_key(|(start, end, _)| (*start, *end));
    if edits.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(QualityIssue::new(
            "isort-conflict",
            "Import and literal edits overlap",
            Some(source.path.clone()),
        ));
    }
    let mut output = text.clone();
    for (start, end, rendered) in edits.into_iter().rev() {
        output.replace_range(start..end, &rendered);
    }
    Ok((output, remaining))
}

pub fn check(workspace: &QualityWorkspace, profile: &FixedProfile) -> ToolReport {
    let mut report = ToolReport {
        tool: QualityTool::Isort,
        reference_version: "9.0.2".into(),
        completion: CompletionStatus::Complete,
        reference_exit_code: Some(0),
        checked_files: 0,
        diagnostics: Vec::new(),
        remaining: Vec::new(),
    };
    for source in &workspace.sources {
        match format(source, workspace, profile) {
            Ok((candidate, remaining)) => {
                report.checked_files += 1;
                report.remaining.extend(remaining);
                if model::reference_changed(source, workspace, profile, &candidate) {
                    report.diagnostics.push(QualityDiagnostic {
                        tool: QualityTool::Isort,
                        rule: "imports".into(),
                        message: "Imports are incorrectly sorted and/or formatted.".into(),
                        hint: None,
                        path: source.path.clone(),
                        severity: "error".into(),
                        location: QualityLocation {
                            line: 1,
                            column: 1,
                            end_line: None,
                            end_column: None,
                            offset: Some(0),
                            end_offset: None,
                        },
                        fix: None,
                    });
                }
            }
            Err(error) => {
                report.completion = CompletionStatus::Error;
                report.remaining.push(error.message);
            }
        }
    }
    report.remaining.push(
        "isort full-profile reference parity has not passed the complete original corpus".into(),
    );
    if report.completion == CompletionStatus::Complete {
        report.completion = CompletionStatus::Incomplete;
    }
    report.reference_exit_code = if report.completion != CompletionStatus::Complete {
        None
    } else {
        Some(if report.diagnostics.is_empty() { 0 } else { 1 })
    };
    report
}

/// Keep the private formatter's single-pass behavior while publishing a fixed point.
fn converged_format(
    source: &SourceSnapshot,
    workspace: &QualityWorkspace,
    profile: &FixedProfile,
) -> Result<(String, Vec<String>), QualityIssue> {
    const MAX_PASSES: usize = 8;
    let mut current = source.clone();
    let mut seen = std::collections::BTreeSet::from([current.bytes.clone()]);
    let mut remaining = Vec::new();
    for _ in 0..MAX_PASSES {
        let (candidate, pass_remaining) = format(&current, workspace, profile)?;
        remaining.extend(pass_remaining);
        // The public file API writes output only when its section checker reports a change.
        if !model::reference_changed(&current, workspace, profile, &candidate) {
            return Ok((current.text, remaining));
        }
        let bytes = super::encoding::encode(&current.path, &candidate, &current.encoding)?;
        if bytes == current.bytes {
            return Ok((candidate, remaining));
        }
        if !seen.insert(bytes.clone()) {
            return Err(QualityIssue::new(
                "isort-convergence",
                "Import sorting entered a byte cycle",
                Some(source.path.clone()),
            ));
        }
        current.text = candidate;
        current.sha256 = format!("{:x}", Sha256::digest(&bytes));
        current.bytes = bytes;
    }
    Err(QualityIssue::new(
        "isort-convergence",
        "Import sorting did not converge after eight passes",
        Some(source.path.clone()),
    ))
}

pub fn transform(workspace: &QualityWorkspace, profile: &FixedProfile) -> TransformResult {
    let mut result = TransformResult {
        completion: CompletionStatus::Complete,
        candidates: Vec::new(),
        issues: Vec::new(),
        remaining: Vec::new(),
    };
    for (index, source) in workspace.sources.iter().enumerate() {
        match converged_format(source, workspace, profile) {
            Ok((candidate, remaining)) => {
                result.remaining.extend(remaining);
                if candidate != source.text {
                    result.candidates.push((index, candidate));
                }
            }
            Err(error) => result.issues.push(error),
        }
    }
    if !result.issues.is_empty() {
        result.completion = CompletionStatus::Error;
    } else {
        result.completion = CompletionStatus::Incomplete;
    }
    result.remaining.push(
        "isort full-profile reference parity has not passed the complete original corpus".into(),
    );
    result
}
