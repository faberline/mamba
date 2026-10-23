//! Physical import sections retain comments that do not belong to an import.
use super::{directives, line_end, line_start, model::Import, render};
use crate::domain::source::PythonVersion;
use mamba_language::application::quality_syntax::SyntaxDocument;
use std::collections::BTreeMap;

pub struct Block {
    pub start: usize,
    pub end: usize,
    pub indent: String,
    pub imports: Vec<Import>,
    pub residual: Vec<String>,
}

fn lines(text: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    normalized
        .split_terminator('\n')
        .map(str::to_owned)
        .collect()
}

fn attachable(line: &str) -> bool {
    line.starts_with('#')
        && !["\"\"\"", "'''"]
            .iter()
            .any(|ending| line.trim_end().ends_with(ending))
        && !line.contains("isort:imports-")
        && !line.contains("isort: imports-")
}

pub fn gap(block: &mut Block, text: &str) -> Vec<String> {
    let mut remaining = lines(text);
    let mut leading = Vec::new();
    while remaining
        .last()
        .is_some_and(|line| attachable(line.strip_prefix(&block.indent).unwrap_or(line)))
    {
        leading.push(remaining.pop().unwrap());
    }
    leading.reverse();
    block.residual.extend(
        remaining
            .into_iter()
            .map(|line| line.strip_prefix(&block.indent).unwrap_or(&line).to_owned()),
    );
    leading
}

pub fn preceding(document: &SyntaxDocument<'_>, start: usize) -> (usize, Vec<String>) {
    let source = document.source;
    let mut begin = start;
    let mut leading = Vec::new();
    while begin > 0 {
        let previous_end = source[..begin].trim_end_matches(['\r', '\n']).len();
        // A blank physical line separates a comment from its import.
        if begin - previous_end
            > if source[..begin].ends_with("\r\n") {
                2
            } else {
                1
            }
        {
            break;
        }
        let previous_start = line_start(source, previous_end);
        let line = &source[previous_start..previous_end];
        if !attachable(line) || line.contains("isort:") {
            break;
        }
        leading.push(line.to_owned());
        begin = previous_start;
    }
    leading.reverse();
    if leading
        .first()
        .is_some_and(|line| !line.starts_with("# pylint: disable-next"))
    {
        let prefix = &source[..begin];
        let line_number = lines(prefix).len();
        let prior_import = prefix.lines().any(|line| {
            ["import ", "from "]
                .iter()
                .any(|prefix| line.trim_start().starts_with(prefix))
        });
        if line_number <= 2 && !prior_import {
            return (start, Vec::new());
        }
    }
    (begin, leading)
}

fn section_marker(line: &str) -> Option<u8> {
    if !line.starts_with('#') {
        return None;
    }
    let (_, marker) = line
        .rsplit_once("isort:imports-")
        .or_else(|| line.rsplit_once("isort: imports-"))?;
    match marker
        .split_whitespace()
        .next()?
        .to_ascii_uppercase()
        .as_str()
    {
        "FUTURE" => Some(0),
        "STDLIB" => Some(1),
        "THIRDPARTY" => Some(2),
        "FIRSTPARTY" => Some(3),
        "LOCALFOLDER" => Some(4),
        _ => None,
    }
}

fn next_construct(tail: &[String], outside: &str) -> String {
    let all: Vec<_> = tail.iter().cloned().chain(lines(outside)).collect();
    for (index, line) in all.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if line.trim_start().starts_with('#')
            && all
                .get(index + 1)
                .is_some_and(|next| !next.trim().is_empty())
        {
            continue;
        }
        return line.clone();
    }
    String::new()
}

fn append_tail(
    output: &mut String,
    mut tail: Vec<String>,
    outside: &str,
    newline: &str,
    stub: bool,
) {
    if output.is_empty() {
        while tail.last().is_some_and(|line| line.trim().is_empty()) {
            tail.pop();
        }
        for line in tail {
            output.push_str(&line);
            output.push_str(newline);
        }
        return;
    }
    let mut blanks = Vec::new();
    while tail.first().is_some_and(|line| line.trim().is_empty()) {
        blanks.push(tail.remove(0));
    }
    while tail.last().is_some_and(|line| line.trim().is_empty()) {
        tail.pop();
    }
    if tail.is_empty() && outside.is_empty() {
        return;
    }
    let next = next_construct(&tail, outside);
    let count = if !stub
        && ["def ", "async def ", "cdef ", "cpdef ", "class ", "@"]
            .iter()
            .any(|prefix| next.starts_with(prefix))
    {
        2
    } else {
        1
    };
    let mut overflow = String::new();
    for index in 0..count.max(blanks.len()) {
        let form_feeds: String = blanks
            .get(index)
            .into_iter()
            .flat_map(|line| line.chars().filter(|character| *character == '\x0c'))
            .collect();
        if index < count {
            output.push_str(&form_feeds);
            output.push_str(newline);
        } else {
            overflow.push_str(&form_feeds);
        }
    }
    output.push_str(&overflow);
    for line in tail {
        output.push_str(&line);
        output.push_str(newline);
    }
}

pub fn output(
    block: Block,
    document: &SyntaxDocument<'_>,
    limit: usize,
    newline: &str,
    stub: bool,
    version: PythonVersion,
) -> (usize, usize, String) {
    output_with_rendered(block, document, limit, newline, stub, version, None)
}

pub(super) fn output_with_rendered(
    block: Block,
    document: &SyntaxDocument<'_>,
    limit: usize,
    newline: &str,
    stub: bool,
    version: PythonVersion,
    replacement: Option<String>,
) -> (usize, usize, String) {
    let source = document.source;
    let mut end = block.end;
    let mut residual = block.residual;
    if block.indent.is_empty() {
        while end < limit {
            let next = line_end(source, end).min(limit);
            let line = &source[end..next];
            let trimmed = line.trim();
            if trimmed.starts_with("# isort:") && section_marker(trimmed).is_none() {
                break;
            }
            residual.extend(lines(line));
            end = next;
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                break;
            }
        }
    } else {
        while end < limit {
            let next = line_end(source, end).min(limit);
            let line = &source[end..next];
            if !line.trim().is_empty() && !line.trim_start().starts_with('#') {
                break;
            }
            if !line.trim().is_empty() && !line.starts_with(&block.indent) {
                break;
            }
            if line.trim_start().starts_with("# isort:") && section_marker(line.trim()).is_none() {
                break;
            }
            residual.extend(
                lines(line)
                    .iter()
                    .map(|line| line.strip_prefix(&block.indent).unwrap_or(line).to_owned()),
            );
            end = next;
        }
    }
    let markers: Vec<_> = residual
        .iter()
        .filter_map(|line| section_marker(line))
        .collect();
    let mut placed = BTreeMap::new();
    for section in &markers {
        let imports = block
            .imports
            .iter()
            .filter(|import| import.section == *section)
            .cloned()
            .collect();
        placed.insert(
            *section,
            render::block(imports, &block.indent, newline, version),
        );
    }
    let imports = block
        .imports
        .into_iter()
        .filter(|import| !markers.contains(&import.section))
        .collect();
    let mut rendered =
        replacement.unwrap_or_else(|| render::block(imports, &block.indent, newline, version));
    let mut tail = Vec::new();
    for (index, line) in residual.iter().enumerate() {
        tail.push(line.clone());
        if let Some(group) = section_marker(line).and_then(|section| placed.get(&section)) {
            tail.extend(lines(group));
            if residual
                .get(index + 1)
                .is_some_and(|next| !next.trim().is_empty())
            {
                tail.push(String::new());
            }
        }
    }
    if block.indent.is_empty() {
        append_tail(&mut rendered, tail, &source[end..], newline, stub);
    } else {
        while tail.last().is_some_and(|line| line.trim().is_empty()) {
            tail.pop();
        }
        for line in tail {
            if !line.is_empty() && !line.starts_with(&block.indent) {
                rendered.push_str(&block.indent);
            }
            rendered.push_str(&line);
            rendered.push_str(newline);
        }
        let original_tail = &source[block.start..end];
        let whitespace_start = original_tail.trim_end().len();
        let whitespace = &original_tail[whitespace_start..];
        if !whitespace.is_empty() {
            rendered = rendered.trim_end().to_owned() + whitespace;
        }
        if end == source.len() && !source.ends_with(['\n', '\r']) {
            rendered.truncate(rendered.len().saturating_sub(newline.len()));
        }
        if end == block.end
            && end == limit
            && limit < source.len()
            && source[limit..].starts_with("import ")
        {
            rendered.push_str(newline);
        }
    }
    let (start, prefix) = directives::restart_prefix(document, block.start);
    (start, end, prefix + &rendered)
}
