//! Comment directives retain their lexical scope.
use mamba_language::application::quality_syntax;
use quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeSet;
#[derive(Clone)]
pub(super) struct Directive {
    pub(super) line: u32,
    pub(super) inline: bool,
    pub(super) text: String,
    pub(super) end_line: u32,
    pub(super) start_line: u32,
    pub(super) offset: usize,
    pub(super) report_line: u32,
    module_end_line: u32,
    pub(super) problem: Option<(&'static str, &'static str, String)>,
    pub(super) skip: bool,
}

#[derive(Default)]
struct Suites {
    ranges: Vec<(TextRange, u32)>,
}
impl<'a> Visitor<'a> for Suites {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if let Stmt::If(value) = statement {
            if let Some(last) = value.body.last() {
                self.ranges.push((
                    TextRange::new(statement.start(), last.end()),
                    statement.start().to_u32(),
                ));
            }
            for clause in &value.elif_else_clauses {
                self.ranges.push((clause.range(), clause.start().to_u32()));
            }
            walk_stmt(self, statement);
            return;
        }
        let body = match statement {
            Stmt::FunctionDef(function) => Some(&function.body),
            Stmt::ClassDef(class) => Some(&class.body),
            _ => None,
        };
        if let Some(body) = body {
            let first = body
                .get(usize::from(super::docstring(body)))
                .map_or(statement.end().to_u32(), |statement| {
                    statement.start().to_u32()
                });
            self.ranges.push((statement.range(), first));
        } else if matches!(
            statement,
            Stmt::If(_)
                | Stmt::For(_)
                | Stmt::While(_)
                | Stmt::With(_)
                | Stmt::Try(_)
                | Stmt::Match(_)
        ) {
            self.ranges
                .push((statement.range(), statement.start().to_u32()));
        }
        walk_stmt(self, statement);
    }
}

pub(super) fn instructions(document: &quality_syntax::SyntaxDocument<'_>) -> Vec<Directive> {
    let mut suites = Suites::default();
    suites.visit_body(&document.parsed.syntax().body);
    document
        .tokens
        .iter()
        .filter(|token| token.kind == "Comment")
        .flat_map(|token| {
            let Some(text) = super::pragma_parser::comment(document.text(token)) else {
                return Vec::new();
            };
            let prefix = &document.source[..token.start as usize];
            let prefix = prefix.rsplit(['\r', '\n']).next().unwrap_or_default();
            let suite = suites
                .ranges
                .iter()
                .filter(|(range, _)| {
                    range.start().to_u32() <= token.start && token.end <= range.end().to_u32()
                })
                .min_by_key(|(range, _)| range.len());
            let report_line = document.line_column(token.start).0;
            let physical_start = document.source[..token.start as usize]
                .rfind('\n')
                .map_or(0, |index| index + 1);
            let previous = document.source[..physical_start]
                .trim_end_matches(['\r', '\n'])
                .rsplit('\n')
                .next()
                .unwrap_or_default();
            let line = report_line - u32::from(previous.trim_end().ends_with('\\'));
            let template = Directive {
                line,
                inline: !prefix.trim().is_empty(),
                text: String::new(),
                end_line: suite.map_or(u32::MAX, |(range, _)| {
                    document.line_column(range.end().to_u32()).0
                }),
                start_line: suite
                    .filter(|(_, first)| token.start < *first && prefix.trim().is_empty())
                    .map_or(document.line_column(token.start).0, |(range, _)| {
                        document.line_column(range.start().to_u32()).0
                    }),
                report_line,
                module_end_line: document.parsed.syntax().body.last().map_or(0, |statement| {
                    document.line_column(statement.end().to_u32()).0
                }),
                offset: token.start as usize,
                problem: None,
                skip: false,
            };
            super::pragma_events::entries(template, super::pragma_parser::parse(text))
        })
        .collect()
}

pub(super) fn suppressed(directives: &[Directive], rule: &str, name: &str, line: u32) -> bool {
    let mut disabled = BTreeSet::new();
    for instruction in directives.iter().filter(|instruction| {
        let first = if matches!(rule, "C0301" | "C0303" | "C0304" | "R1710") {
            instruction.line
        } else {
            instruction.start_line
        };
        first <= line && line <= instruction.end_line
    }) {
        let directive = instruction.text.trim();
        let (action, arguments) = match directive.split_once('=') {
            Some(value) => value,
            None => continue,
        };
        let targets: Vec<_> = arguments.split(',').map(str::trim).collect();
        let matches = targets
            .iter()
            .any(|target| matches!(*target, "all") || *target == name || *target == rule);
        if action.trim() == "disable-next" && instruction.line + 1 == line && matches {
            return true;
        }
        if instruction.inline {
            if instruction.line == line && action.trim() == "disable" && matches {
                return true;
            }
            continue;
        }
        if action.trim() == "disable" {
            disabled.extend(targets.iter().map(|target| (*target).to_owned()));
        }
        if action.trim() == "enable" {
            for target in targets {
                disabled.remove(target);
            }
        }
    }
    disabled.contains("all") || disabled.contains(rule) || disabled.contains(name)
}
pub(super) fn module_lines_pragma(directives: &[Directive]) -> Option<u32> {
    directives
        .iter()
        .filter_map(|value| {
            let (action, names) = value.text.trim().split_once('=')?;
            (action.trim() == "disable"
                && names
                    .split(',')
                    .map(str::trim)
                    .any(|name| matches!(name, "too-many-lines" | "C0302")))
            .then_some(value.line)
        })
        .next()
}

pub(super) fn active(directives: &[Directive], rule: &str, line: u32) -> bool {
    let mut enabled = super::message_registry::enabled(rule);
    if !enabled
        && directives
            .first()
            .is_some_and(|entry| line > entry.module_end_line)
    {
        return false;
    }
    for instruction in directives
        .iter()
        .filter(|entry| entry.line <= line && line <= entry.end_line)
    {
        let Some((action, arguments)) = instruction.text.split_once('=') else {
            continue;
        };
        if !arguments.split(',').any(|target| target == rule) {
            continue;
        }
        match action {
            "enable" => enabled = true,
            "disable" => enabled = false,
            "disable-next" if instruction.line + 1 == line => enabled = false,
            _ => {}
        }
    }
    enabled
}
