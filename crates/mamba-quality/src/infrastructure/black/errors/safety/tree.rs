//! CPython field trees rendered by Black's sorted-field AST visitor.
use std::collections::BTreeMap;

pub(super) enum Value {
    Node(Node),
    List(Vec<Value>),
    String(String),
    Scalar(String, &'static str),
}

pub(super) struct Node {
    pub name: String,
    pub fields: BTreeMap<&'static str, Value>,
}

pub(super) fn node<const N: usize>(
    name: impl Into<String>,
    fields: [(&'static str, Value); N],
) -> Value {
    Value::Node(Node {
        name: name.into(),
        fields: fields.into_iter().collect(),
    })
}
pub(super) fn leaf(name: impl Into<String>) -> Value {
    node(name, [])
}
pub(super) fn none() -> Value {
    Value::Scalar("None".into(), "NoneType")
}
pub(super) fn integer(value: impl ToString) -> Value {
    Value::Scalar(value.to_string(), "int")
}
pub(super) fn string(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
pub(super) fn optional_string(value: Option<&str>) -> Value {
    value.map_or_else(none, string)
}
pub(super) fn constant(value: Value) -> Value {
    node("Constant", [("kind", none()), ("value", value)])
}

fn whitespace(c: char) -> bool {
    matches!(c, '\t'..='\r' | '\x1c'..='\x20' | '\u{85}' | '\u{a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}
fn normalized_string(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .split([
            '\n', '\r', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .map(|line| line.trim_matches(whitespace))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_matches(whitespace)
        .into()
}
fn repr(value: &str) -> String {
    use ruff_python_ast::str::TripleQuotes;
    ruff_python_literal::escape::UnicodeEscape::new_repr(value)
        .str_repr(TripleQuotes::No)
        .to_string()
        .expect("AST string fits memory")
}

fn render<'a>(value: &'a Value, parents: &mut Vec<&'a str>, lines: &mut Vec<String>) {
    let Value::Node(node) = value else {
        return;
    };
    let indent = "    ".repeat(parents.len());
    let child_indent = "    ".repeat(parents.len() + 1);
    lines.push(format!("{indent}{}(", node.name));
    if node.name != "TypeIgnore" {
        for (&field, value) in &node.fields {
            lines.push(format!("{child_indent}{field}="));
            match value {
                Value::Node(_) => {
                    parents.push(&node.name);
                    render(value, parents, lines);
                    parents.pop();
                }
                Value::List(items) => {
                    parents.push(&node.name);
                    for item in items {
                        render(item, parents, lines);
                    }
                    parents.pop();
                }
                Value::String(value) => {
                    let normalized = if node.name == "Constant"
                        && field == "value"
                        && parents.len() >= 2
                        && parents.last() == Some(&"Expr")
                    {
                        normalized_string(value)
                    } else if field == "type_comment" {
                        value.trim_end_matches(whitespace).to_owned()
                    } else {
                        value.clone()
                    };
                    lines.push(format!("{child_indent}{},  # str", repr(&normalized)));
                }
                Value::Scalar(value, kind) => {
                    lines.push(format!("{child_indent}{value},  # {kind}"))
                }
            }
        }
    }
    lines.push(format!("{indent})  # /{}", node.name));
}
pub(super) fn stringify(value: &Value) -> String {
    let mut lines = Vec::new();
    render(value, &mut Vec::new(), &mut lines);
    lines.join("\n")
}
