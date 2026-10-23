//! Native handling of the original isort literal and assignment directives.
mod errors;
mod malformed;
mod sections;
mod syntax;
mod value;
use super::{directives, line_end, model};
use crate::domain::model::QualityIssue;
use errors::Error;
use mamba_language::application::quality_syntax::{
    self,
    python_ast::{Expr, Stmt},
    PythonTarget, SyntaxDocument,
};
use ruff_text_size::Ranged;
use std::{collections::BTreeMap, path::Path};
use value::{evaluate, sorted};

pub(super) fn scan_quotes(line: &str, state: &mut String) {
    sections::has_open_quote(line, state);
}

fn collection(
    expression: &Expr,
    kind: &str,
    prefix: usize,
    indent: &str,
    raw: &str,
    newline: &str,
    target: PythonTarget,
) -> Result<String, Error> {
    let (open, close, elements, tuple) = match (kind, expression) {
        ("list" | "unique-list", Expr::List(value)) => ("[", "]", value.elts.as_slice(), false),
        ("tuple" | "unique-tuple", Expr::Tuple(value)) => ("(", ")", value.elts.as_slice(), true),
        ("set", Expr::Set(value)) => ("{", "}", value.elts.as_slice(), false),
        ("set", Expr::Call(call))
            if call.arguments.is_empty()
                && matches!(call.func.as_ref(), Expr::Name(name) if name.id == "set") =>
        {
            ("{", "}", &[][..], false)
        }
        ("dict", Expr::Dict(dictionary)) => {
            let mut pairs: Vec<(value::Value, value::Value)> = Vec::new();
            for item in &dictionary.items {
                let key = evaluate(item.key.as_ref().ok_or_else(|| {
                    Error::new("isort-literal", "Dictionary expansion is not literal data")
                })?)
                .map_err(|message| Error::new("isort-literal", message))?;
                let value = evaluate(&item.value)
                    .map_err(|message| Error::new("isort-literal", message))?;
                if let Some((_, previous)) = pairs
                    .iter_mut()
                    .find(|(previous, _)| previous.order == key.order)
                {
                    *previous = value;
                } else {
                    pairs.push((key, value));
                }
            }
            let ordered = sorted(
                pairs.iter().map(|(_, value)| value.clone()).collect(),
                false,
            )
            .map_err(|message| errors::ordering_error(message, target))?;
            let mut used = Vec::new();
            let elements = ordered
                .iter()
                .map(|value| {
                    let index = pairs
                        .iter()
                        .enumerate()
                        .find(|(index, (_, candidate))| {
                            !used.contains(index) && candidate.order == value.order
                        })
                        .unwrap()
                        .0;
                    used.push(index);
                    format!(
                        "{}: {}",
                        pairs[index].0.element_repr(),
                        value.element_repr()
                    )
                })
                .collect();
            return Ok(render(
                elements, "{", "}", prefix, indent, raw, newline, false,
            ));
        }
        _ => return Err(Error::mismatch(expression, kind)),
    };
    let values = sorted(
        elements
            .iter()
            .map(evaluate)
            .collect::<Result<_, _>>()
            .map_err(|message| Error::new("isort-literal", message))?,
        kind.starts_with("unique-") || kind == "set",
    )
    .map_err(|message| errors::ordering_error(message, target))?;
    Ok(render(
        values.iter().map(value::Value::element_repr).collect(),
        open,
        close,
        prefix,
        indent,
        raw,
        newline,
        tuple,
    ))
}

fn render(
    elements: Vec<String>,
    open: &str,
    close: &str,
    prefix: usize,
    indent: &str,
    raw: &str,
    newline: &str,
    tuple: bool,
) -> String {
    let comma = tuple && elements.len() == 1;
    let inner = elements.join(", ") + if comma { "," } else { "" };
    let single = format!("{open}{inner}{close}");
    let magic = raw.contains(['\r', '\n'])
        && raw.trim_end().ends_with(close)
        && raw
            .trim_end()
            .trim_end_matches(close)
            .trim_end()
            .ends_with(',');
    if !magic && prefix + single.chars().count() <= 88 {
        return single;
    }
    let mut output = format!("{open}{newline}");
    for element in elements {
        output.push_str(&format!("{indent}    {element},{newline}"));
    }
    output.push_str(indent);
    output.push_str(close);
    output
}

fn assignment(
    raw: &str,
    kind: &str,
    indent: &str,
    target: PythonTarget,
    stub: bool,
) -> Result<String, Error> {
    if kind == "assignments" {
        let mut assignments = BTreeMap::new();
        for line in raw
            .split_inclusive('\n')
            .filter(|line| !line.trim().is_empty())
        {
            let (name, value) = line
                .split_once(" = ")
                .ok_or_else(|| Error::assignments(raw))?;
            assignments.insert(name, value);
        }
        return Ok(assignments
            .into_iter()
            .map(|(name, value)| format!("{name} = {value}"))
            .collect());
    }
    let (variable, literal) = raw.split_once('=').ok_or_else(|| {
        Error::new(
            "ValueError",
            "not enough values to unpack (expected 2, got 1)",
        )
    })?;
    let variable = variable.trim();
    let literal = literal.trim_start();
    // Keep the temporary source alive while the AST borrows its ranges.
    let normalized = format!("value = {literal}");
    let document = quality_syntax::parse(&normalized, target, stub)
        .map_err(|message| Error::new("isort-literal", message))?;
    if !document.is_valid() {
        return Err(Error::parsing(raw, &syntax::reason(&document)));
    }
    let Some(Stmt::Assign(statement)) = document.parsed.syntax().body.first() else {
        return Err(Error::parsing(raw, "invalid syntax (<unknown>, line 1)"));
    };
    if document.parsed.syntax().body.len() != 1 {
        let statement = &document.parsed.syntax().body[1];
        let line = document.line_column(statement.start().to_u32()).0;
        return Err(Error::parsing(
            raw,
            &format!("invalid syntax (<unknown>, line {line})"),
        ));
    }
    let expression = statement.value.as_ref();
    if let Some(error) = malformed::error(expression, raw, &document) {
        return Err(error);
    }
    if let Expr::Dict(dictionary) = expression {
        for item in &dictionary.items {
            if let Some(key) = &item.key {
                let value =
                    evaluate(key).map_err(|message| Error::new("isort-literal", message))?;
                if let Some(kind) = value.unhashable_type() {
                    return Err(Error::parsing(
                        raw,
                        &errors::unhashable_type_error(kind, "dict key", target),
                    ));
                }
            }
        }
    }
    if let Expr::Set(set) = expression {
        for element in &set.elts {
            let value =
                evaluate(element).map_err(|message| Error::new("isort-literal", message))?;
            if let Some(kind) = value.unhashable_type() {
                return Err(Error::parsing(
                    raw,
                    &errors::set_element_type_error(kind, target),
                ));
            }
        }
    }
    let start = expression.range().start().to_usize() - "value = ".len();
    let end = expression.range().end().to_usize() - "value = ".len();
    let newline = model::newline(raw);
    let value = collection(
        expression,
        kind,
        indent.chars().count() + variable.chars().count() + 3,
        indent,
        &literal[start..],
        newline,
        target,
    )?;
    Ok(format!("{indent}{variable} = {value}{}", &literal[end..]))
}

pub fn edits(
    document: &SyntaxDocument<'_>,
    stub: bool,
    path: &Path,
) -> Result<Vec<(usize, usize, String)>, QualityIssue> {
    let mut edits = Vec::new();
    let source = document.source;
    for token in document
        .tokens
        .iter()
        .filter(|token| token.kind == "Comment")
    {
        let offset = token.start as usize;
        let Some(kind) = document
            .text(token)
            .strip_prefix("# isort: ")
            .filter(|kind| {
                matches!(
                    *kind,
                    "list"
                        | "unique-list"
                        | "tuple"
                        | "unique-tuple"
                        | "dict"
                        | "set"
                        | "assignments"
                )
            })
        else {
            continue;
        };
        let start_line = super::line_start(source, offset);
        let indent = &source[start_line..offset];
        if !indent.trim().is_empty() || !directives::enabled(document, start_line, start_line) {
            continue;
        }
        let start = line_end(source, offset);
        let mut end = start;
        while end < source.len() {
            let next = line_end(source, end);
            if source[end..next].trim().is_empty() {
                break;
            }
            end = next;
        }
        if start == end {
            continue;
        }
        let (code, preserved) = sections::collected(&source[start..end]);
        if code.is_empty() {
            continue;
        }
        let replacement = assignment(&code, kind, indent, document.target, stub)
            .map_err(|error| QualityIssue::new(error.code, error.message, Some(path.into())))?;
        edits.push((start, end, preserved + &replacement));
    }
    Ok(edits)
}
