//! Black output.diff: difflib unified hunks with five context lines and EOF markers.
use super::sequence_matcher::{self, Code, Tag};
const CONTEXT: usize = 5;

fn groups(mut codes: Vec<Code>) -> Vec<Vec<Code>> {
    if let Some(first) = codes.first_mut() {
        if first.tag == Tag::Equal {
            first.a_start = first.a_start.max(first.a_end.saturating_sub(CONTEXT));
            first.b_start = first.b_start.max(first.b_end.saturating_sub(CONTEXT));
        }
    }
    if let Some(last) = codes.last_mut() {
        if last.tag == Tag::Equal {
            last.a_end = last.a_end.min(last.a_start + CONTEXT);
            last.b_end = last.b_end.min(last.b_start + CONTEXT);
        }
    }
    let mut result = Vec::new();
    let mut group = Vec::new();
    for mut code in codes {
        if code.tag == Tag::Equal && code.a_end - code.a_start > CONTEXT * 2 {
            group.push(Code {
                a_end: code.a_start + CONTEXT,
                b_end: code.b_start + CONTEXT,
                ..code
            });
            result.push(std::mem::take(&mut group));
            code.a_start = code.a_start.max(code.a_end - CONTEXT);
            code.b_start = code.b_start.max(code.b_end - CONTEXT);
        }
        group.push(code);
    }
    if !group.is_empty() && !(group.len() == 1 && group[0].tag == Tag::Equal) {
        result.push(group);
    }
    result
}
fn range(start: usize, end: usize) -> String {
    match end - start {
        0 => format!("{start},0"),
        1 => (start + 1).to_string(),
        length => format!("{},{length}", start + 1),
    }
}
fn lines(output: &mut String, prefix: char, lines: &[&str]) {
    for line in lines {
        output.push(prefix);
        output.push_str(line);
        if !line.ends_with('\n') {
            output.push_str("\n\\ No newline at end of file\n");
        }
    }
}
pub(super) fn unified(source: &str, candidate: &str) -> String {
    unified_named(source, candidate, "src", "dst")
}

pub(super) fn unified_named(
    source: &str,
    candidate: &str,
    source_name: &str,
    candidate_name: &str,
) -> String {
    // Stringified AST lines contain escaped string data. Only LF separates nodes.
    let a: Vec<_> = source.split_inclusive('\n').collect();
    let b: Vec<_> = candidate.split_inclusive('\n').collect();
    let mut output = String::new();
    for group in groups(sequence_matcher::codes(&a, &b)) {
        if output.is_empty() {
            output.push_str(&format!("--- {source_name}\n+++ {candidate_name}\n"));
        }
        let first = group[0];
        let last = group[group.len() - 1];
        output.push_str(&format!(
            "@@ -{} +{} @@\n",
            range(first.a_start, last.a_end),
            range(first.b_start, last.b_end)
        ));
        for code in group {
            match code.tag {
                Tag::Equal => lines(&mut output, ' ', &a[code.a_start..code.a_end]),
                Tag::Delete => lines(&mut output, '-', &a[code.a_start..code.a_end]),
                Tag::Insert => lines(&mut output, '+', &b[code.b_start..code.b_end]),
                Tag::Replace => {
                    lines(&mut output, '-', &a[code.a_start..code.a_end]);
                    lines(&mut output, '+', &b[code.b_start..code.b_end]);
                }
            }
        }
    }
    output
}
