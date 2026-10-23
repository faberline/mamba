//! Black reports invalid formatted output as an internal parse error.
use super::{python_name, syntax};
use crate::domain::{
    model::QualityIssue,
    source::{QualityWorkspace, SourceSnapshot},
};
use crate::infrastructure::failure_logs::FailureLogs;
use mamba_language::application::quality_syntax::{PythonTarget, SyntaxDocument};
use std::io::Write;

pub(in crate::infrastructure::black) fn error(
    source: &SourceSnapshot,
    candidate: &str,
    target: PythonTarget,
    document: &SyntaxDocument<'_>,
    source_document: &SyntaxDocument<'_>,
    workspace: &QualityWorkspace,
    logs: &FailureLogs,
) -> QualityIssue {
    let mut formatted_source = source.clone();
    formatted_source.text = candidate.to_owned();
    let parse_message = skipped_interpolation_eof(&formatted_source, source_document)
        .unwrap_or_else(|| syntax(&formatted_source, document).message);
    let saved = (|| {
        let mut file = logs.file(workspace, "blk_", ".log")?;
        file.write_all(candidate.as_bytes())?;
        let (_, path) = file.keep().map_err(|error| error.error)?;
        Ok::<_, std::io::Error>(path)
    })();
    let path = match saved {
        Ok(path) => path,
        Err(error) => {
            return QualityIssue::new(
                "black-parsing-log",
                format!("Could not save Black's invalid formatted output: {error}"),
                Some(source.path.clone()),
            );
        }
    };
    QualityIssue::new(
        "black.parsing.InvalidInput",
        format!(
            "INTERNAL ERROR: Black 26.10.0 on Python (CPython) {} produced invalid code:\n{}\nPlease report a bug on https://github.com/psf/black/issues.  This invalid output might be helpful: {}",
            match target {
                PythonTarget::Py312 => "3.12.11",
                PythonTarget::Py313 => "3.13.5",
                PythonTarget::Py314 => "3.14.0",
            },
            parse_message,
            path.display()
        ),
        Some(source.path.clone()),
    )
}

fn skipped_interpolation_eof(
    source: &SourceSnapshot,
    document: &SyntaxDocument<'_>,
) -> Option<String> {
    let f_string_start = document
        .tokens
        .iter()
        .find(|token| matches!(token.kind.as_str(), "FStringStart" | "TStringStart"))?
        .start;
    let comment = document.tokens.iter().rev().find(|token| {
        token.kind == "Comment"
            && token.start >= f_string_start
            && is_fmt_skip_comment(document.text(token))
    })?;
    let comment_text = document.text(comment);
    let (line, faulty) = source
        .text
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let comment_start = line.rfind(comment_text)?;
            trailing_quoted_payload(&line[comment_start..], 0)?;
            Some((index as u32 + 1, line))
        })
        .last()?;
    let column = faulty.chars().count() as u32;
    Some(format!(
        "cannot parse for target version Python {}: {line}:{column}\n    {faulty}\n    {}^\nTokenError: Unexpected EOF in multi-line statement",
        python_name(document.target),
        " ".repeat(column.saturating_sub(1) as usize)
    ))
}

fn is_fmt_skip_comment(comment: &str) -> bool {
    let normalized = comment.replace("fmt:skip", "fmt: skip");
    normalized.contains("fmt: skip")
        && crate::infrastructure::black::comments::directive(&normalized)
}

fn trailing_quoted_payload(comment: &str, base: usize) -> Option<usize> {
    let bytes = comment.as_bytes();
    let close = (0..bytes.len())
        .rev()
        .find(|index| matches!(bytes[*index], b'\'' | b'"') && !is_escaped(bytes, *index))?;
    let quote = bytes[close];
    let open = (0..close)
        .rev()
        .find(|index| bytes[*index] == quote && !is_escaped(bytes, *index))?;
    (open + 1 < close).then_some(base + open + 1)
}

fn is_escaped(bytes: &[u8], index: usize) -> bool {
    let mut backslashes = 0;
    let mut cursor = index;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        backslashes += 1;
        cursor -= 1;
    }
    backslashes % 2 == 1
}
