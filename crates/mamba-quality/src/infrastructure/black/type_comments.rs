//! CPython type-comment ownership used by Black's AST safety comparison.
//! Invalid type-comment syntax makes Black retry parsing without type comments.
use mamba_language::application::quality_syntax::SyntaxDocument;
use ruff_python_ast::{
    token::TokenKind,
    visitor::{self, Visitor},
    Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::{BTreeMap, BTreeSet};

struct Slot {
    key: String,
    end: u32,
    stop: u32,
    kind: Kind,
}

enum Kind {
    Assignment,
    Parameter,
    Header { own_line: bool },
}

struct Slots<'a> {
    document: &'a SyntaxDocument<'a>,
    next_statement: usize,
    slots: Vec<Slot>,
}

fn trivia(kind: TokenKind) -> bool {
    kind.is_trivia()
        || kind.is_any_newline()
        || matches!(kind, TokenKind::Indent | TokenKind::Dedent)
}

impl Slots<'_> {
    fn header(&mut self, key: String, statement: &Stmt, body: &[Stmt], own_line: bool) {
        let Some(first) = body.first() else { return };
        let end = self
            .document
            .parsed
            .tokens()
            .in_range(TextRange::new(statement.start(), first.start()))
            .iter()
            .rev()
            .find(|token| !trivia(token.kind()))
            .filter(|token| token.kind() == TokenKind::Colon)
            .map(|token| token.end().to_u32());
        if let Some(end) = end {
            self.slots.push(Slot {
                key,
                end,
                stop: first.start().to_u32(),
                kind: Kind::Header { own_line },
            });
        }
    }
}

impl<'a> Visitor<'a> for Slots<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        let index = self.next_statement;
        self.next_statement += 1;
        let key = format!("statement:{index}");
        match statement {
            Stmt::Assign(_) => self.slots.push(Slot {
                key,
                end: statement.end().to_u32(),
                stop: u32::MAX,
                kind: Kind::Assignment,
            }),
            Stmt::For(value) => self.header(key, statement, &value.body, false),
            Stmt::With(value) => self.header(key, statement, &value.body, false),
            Stmt::FunctionDef(function) => {
                self.header(key, statement, &function.body, true);
                for (argument, parameter) in function.parameters.iter().enumerate() {
                    self.slots.push(Slot {
                        key: format!("argument:{index}:{argument}"),
                        end: parameter.end().to_u32(),
                        stop: function.parameters.end().to_u32(),
                        kind: Kind::Parameter,
                    });
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, statement);
    }
}

fn payload(comment: &str) -> Option<&str> {
    comment
        .strip_prefix('#')?
        .trim_start_matches([' ', '\t'])
        .strip_prefix("type:")
        .map(|value| value.trim_start_matches([' ', '\t']))
}

fn owns(slot: &Slot, start: u32, document: &SyntaxDocument<'_>) -> bool {
    if start < slot.end || start >= slot.stop {
        return false;
    }
    let between = &document.source[slot.end as usize..start as usize];
    match slot.kind {
        Kind::Assignment => !between.contains(['\r', '\n']) && between.trim().is_empty(),
        Kind::Header { own_line } => {
            (own_line || !between.contains(['\r', '\n']))
                && document
                    .parsed
                    .tokens()
                    .in_range(TextRange::new(slot.end.into(), start.into()))
                    .iter()
                    .all(|token| trivia(token.kind()))
        }
        Kind::Parameter => document
            .parsed
            .tokens()
            .in_range(TextRange::new(slot.end.into(), start.into()))
            .iter()
            .all(|token| trivia(token.kind()) || token.kind() == TokenKind::Comma),
    }
}

pub(super) fn fingerprint(document: &SyntaxDocument<'_>) -> Vec<(String, String)> {
    let mut slots = Slots {
        document,
        next_statement: 0,
        slots: Vec::new(),
    };
    for statement in &document.parsed.syntax().body {
        slots.visit_stmt(statement);
    }
    let mut values = BTreeMap::new();
    let mut ignore_lines = BTreeSet::new();
    for token in document
        .parsed
        .tokens()
        .iter()
        .filter(|token| token.kind() == TokenKind::Comment)
    {
        let Some(value) = payload(&document.source[token.range()]) else {
            continue;
        };
        if value.strip_prefix("ignore").is_some_and(|tail| {
            !tail
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        }) {
            ignore_lines.insert(document.line_column(token.start().to_u32()).0);
            continue;
        }
        let Some(slot) = slots
            .slots
            .iter()
            .filter(|slot| owns(slot, token.start().to_u32(), document))
            .max_by_key(|slot| slot.end)
        else {
            return Vec::new();
        };
        if values
            .insert(slot.key.clone(), value.trim_end().to_string())
            .is_some()
        {
            return Vec::new();
        }
    }
    if !ignore_lines.is_empty() {
        values.insert("type_ignores".into(), ignore_lines.len().to_string());
    }
    values.into_iter().collect()
}
