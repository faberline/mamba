//! Invalid Python still has the original isort text-scanner behavior.
mod imports;
use super::{blocks, directives, line_end, literals, model, render};
use crate::domain::{
    model::QualityIssue,
    profile::FixedProfile,
    source::{QualityWorkspace, SourceSnapshot},
};
use imports::Kind;
use mamba_language::application::quality_syntax::SyntaxDocument;

struct Group {
    block: blocks::Block,
    kinds: Vec<Kind>,
}

fn output(
    group: Group,
    document: &SyntaxDocument<'_>,
    limit: usize,
    newline: &str,
    source: &SourceSnapshot,
    workspace: &QualityWorkspace,
) -> (usize, usize, String) {
    let version = workspace.projects[source.project].python_version;
    let mut section_groups = Vec::new();
    for section in 0..=4 {
        let mut text = String::new();
        for kind in [Kind::Eager, Kind::Cython, Kind::Lazy] {
            let imports: Vec<_> = group
                .block
                .imports
                .iter()
                .zip(&group.kinds)
                .filter(|(import, current)| import.section == section && **current == kind)
                .map(|(import, _)| import.clone())
                .collect();
            let rendered = render::block(imports, &group.block.indent, newline, version);
            for line in rendered.split_inclusive(newline) {
                if kind == Kind::Lazy
                    && ["import ", "from "]
                        .iter()
                        .any(|prefix| line.trim_start().starts_with(prefix))
                {
                    text.push_str(&group.block.indent);
                    text.push_str("lazy ");
                    text.push_str(line.trim_start());
                } else if kind == Kind::Cython {
                    if line.trim_start().starts_with("import ") && newline != "\r" {
                        text.push_str(&group.block.indent);
                        text.push('c');
                        text.push_str(line.trim_start());
                    } else {
                        text.push_str(&line.replacen(" import ", " cimport ", 1));
                    }
                } else {
                    text.push_str(line);
                }
            }
        }
        if !text.is_empty() {
            section_groups.push(text);
        }
    }
    let rendered = section_groups.join(newline);
    blocks::output_with_rendered(
        group.block,
        document,
        limit,
        newline,
        source.is_stub,
        version,
        Some(rendered),
    )
}

pub(super) fn format(
    source: &SourceSnapshot,
    workspace: &QualityWorkspace,
    profile: &FixedProfile,
    document: &SyntaxDocument<'_>,
) -> Result<(String, Vec<String>), QualityIssue> {
    format_with_changes(source, workspace, profile, document)
        .map(|(output, remaining, _)| (output, remaining))
}

pub(super) fn format_with_changes(
    source: &SourceSnapshot,
    workspace: &QualityWorkspace,
    profile: &FixedProfile,
    document: &SyntaxDocument<'_>,
) -> Result<(String, Vec<String>, bool), QualityIssue> {
    let text = &source.text;
    let newline = model::newline(text);
    let mut groups: Vec<Group> = Vec::new();
    let mut offset = 0;
    let mut quote = String::new();
    let mut edits = literals::edits(document, source.is_stub, &source.path)?;
    let mut changed = edits
        .iter()
        .any(|(start, end, output)| text[*start..*end].trim_end() != output.trim_end());
    while offset < text.len() {
        let start = offset;
        let end = line_end(text, start);
        let line = &text[start..end];
        offset = end;
        let was_quoted = !quote.is_empty();
        literals::scan_quotes(line, &mut quote);
        let trimmed = line.trim();
        let mut continuation = trimmed.split('#').next().unwrap_or_default().trim_end();
        if !was_quoted && (continuation.starts_with("raise") || continuation.starts_with("yield")) {
            while continuation.ends_with('\\') && offset < text.len() {
                let next = line_end(text, offset);
                continuation = text[offset..next]
                    .trim()
                    .split('#')
                    .next()
                    .unwrap_or_default()
                    .trim_end();
                offset = next;
            }
            continue;
        }
        if was_quoted || !quote.is_empty() || !imports::starts(trimmed) {
            continue;
        }
        let mut raw_end = end;
        let mut code = line.split('#').next().unwrap_or_default().trim_end();
        let mut parenthesis = code.contains('(') && !code.contains(')');
        while code.ends_with('\\') || parenthesis {
            if raw_end == text.len() {
                if parenthesis && !code.ends_with('\\') {
                    return Err(QualityIssue::new(
                        "ExistingSyntaxErrors",
                        "isort was told to sort imports within code that contains syntax errors: Parenthesis is not closed.",
                        Some(source.path.clone()),
                    ));
                }
                break;
            }
            let next = line_end(text, raw_end);
            code = text[raw_end..next]
                .split('#')
                .next()
                .unwrap_or_default()
                .trim_end();
            raw_end = next;
            parenthesis = (parenthesis || code.contains('(')) && !code.contains(')');
        }
        offset = raw_end;
        let indent = &line[..line.len() - line.trim_start().len()];
        if !directives::enabled(document, start, raw_end) {
            if indent.is_empty()
                && raw_end == text.len()
                && !text.ends_with(['\n', '\r'])
                && (line.contains("isort:skip") || line.contains("isort: skip"))
            {
                edits.push((raw_end, raw_end, newline.into()));
            }
            continue;
        }
        let Some((kind, mut import)) = imports::parse(text[start..raw_end].trim_start()) else {
            if indent.is_empty() && raw_end == text.len() && !text.ends_with(['\n', '\r']) {
                edits.push((raw_end, raw_end, newline.into()));
            }
            continue;
        };
        let join = groups.last().is_some_and(|group| {
            indent.len() >= group.block.indent.len()
                && (!group.kinds.contains(&Kind::Cython) && kind != Kind::Cython
                    || group.kinds.iter().all(|current| *current == kind))
                && text[group.block.end..start].lines().all(|line| {
                    line.trim().is_empty()
                        || (line.trim_start().starts_with('#')
                            && (group.block.indent.is_empty()
                                || line.starts_with(&group.block.indent)))
                })
        });
        if !join {
            let (begin, leading) = if indent.is_empty()
                && !groups
                    .last()
                    .is_some_and(|group| !group.block.indent.is_empty())
            {
                blocks::preceding(document, start)
            } else {
                (start, Vec::new())
            };
            import.leading = leading;
            groups.push(Group {
                block: blocks::Block {
                    start: begin,
                    end: start,
                    indent: indent.into(),
                    imports: Vec::new(),
                    residual: Vec::new(),
                },
                kinds: Vec::new(),
            });
        }
        let group = groups.last_mut().unwrap();
        if join {
            let previous_end = group.block.end;
            import.leading = blocks::gap(&mut group.block, &text[previous_end..start]);
        }
        group.block.end = raw_end;
        if import.from {
            import.section =
                model::section(&import.module, &workspace.projects[source.project], profile);
            group.block.imports.push(import);
            group.kinds.push(kind);
        } else {
            for (index, name) in import.names.iter().enumerate() {
                group.block.imports.push(model::Import {
                    module: name.name.clone(),
                    from: false,
                    names: vec![name.clone()],
                    leading: if index == 0 {
                        import.leading.clone()
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
                group.kinds.push(kind);
            }
        }
    }
    let mut limit = text.len();
    for group in groups.into_iter().rev() {
        let begin = group.block.start;
        let has_names = group
            .block
            .imports
            .iter()
            .any(|import| !import.names.is_empty());
        let indented = !group.block.indent.is_empty();
        let mut edit = output(group, document, limit, newline, source, workspace);
        if indented && edit.1 == text.len() && !text.ends_with(['\n', '\r']) {
            let suffix = &text[begin..];
            let trailing = &suffix[suffix.trim_end().len()..];
            edit.2 = edit.2.trim_end().to_owned() + trailing;
        }
        changed |= has_names && text[edit.0..edit.1].trim_end() != edit.2.trim_end();
        edits.push(edit);
        limit = begin;
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
    for (start, end, candidate) in edits.into_iter().rev() {
        output.replace_range(start..end, &candidate);
    }
    Ok((output, Vec::new(), changed))
}
