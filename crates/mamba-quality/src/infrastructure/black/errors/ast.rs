//! AST error reasons are inferred from syntax nodes and fixed Python versions.
use super::targets;
use mamba_language::application::quality_syntax::{PythonTarget, SyntaxDocument};
use ruff_python_ast::{
    visitor::{self, Visitor},
    Expr, Stmt, StringLike, TypeParam, WithItem,
};
use ruff_text_size::Ranged;

pub(super) fn error(document: &SyntaxDocument<'_>) -> Option<(String, u32)> {
    if let Some(error) = template_lexical_error(document) {
        return Some(error);
    }
    let mut check = Check {
        document,
        errors: Vec::new(),
        parenthesized_tstring_concat: None,
    };
    check.visit_body(&document.parsed.syntax().body);
    check.aliases();
    if let Some(offset) = check.parenthesized_tstring_concat {
        check.add("invalid syntax. Perhaps you forgot a comma?", offset);
    }
    check.version_errors();
    check.errors.sort_by_key(|(_, offset)| *offset);
    check.errors.into_iter().next()
}

struct Check<'a, 'b> {
    document: &'a SyntaxDocument<'b>,
    errors: Vec<(String, u32)>,
    parenthesized_tstring_concat: Option<u32>,
}

fn enclosed_by_container(document: &SyntaxDocument<'_>, start: u32, end: u32) -> bool {
    let mut openings = Vec::new();
    let mut pairs = Vec::new();
    for (index, token) in document.tokens.iter().enumerate() {
        match token.kind.as_str() {
            "Lpar" => openings.push((index, "Rpar")),
            "Lsqb" => openings.push((index, "Rsqb")),
            "Lbrace" => openings.push((index, "Rbrace")),
            "Rpar" | "Rsqb" | "Rbrace"
                if openings
                    .last()
                    .is_some_and(|(_, closing)| *closing == token.kind) =>
            {
                let (opening, _) = openings.pop().unwrap();
                pairs.push((opening, index));
            }
            _ => {}
        }
    }
    pairs.into_iter().any(|(opening, closing)| {
        document.tokens[opening].end <= start && document.tokens[closing].start >= end
    })
}

impl Check<'_, '_> {
    fn add(&mut self, reason: impl Into<String>, offset: u32) {
        self.errors.push((reason.into(), offset));
    }

    fn version_errors(&mut self) {
        for issue in &self.document.issues {
            let reason = if issue.version_error
                && issue.message.contains("t-strings")
                && self.parenthesized_tstring_concat != Some(issue.start)
            {
                Some("invalid syntax")
            } else if issue.message.contains("Multiple exception types") {
                Some("multiple exception types must be parenthesized")
            } else if issue.version_error && issue.message.contains("default type") {
                let line_start = self.document.source[..issue.start as usize]
                    .rfind('\n')
                    .map_or(0, |index| index + 1);
                let prefix = &self.document.source[line_start..issue.start as usize];
                if prefix.contains(':') || prefix.contains('*') {
                    Some("invalid syntax")
                } else if prefix.trim_start().starts_with("def ") {
                    Some("expected '('")
                } else if prefix.trim_start().starts_with("type ") {
                    Some("invalid syntax. Maybe you meant '==' or ':=' instead of '='?")
                } else {
                    Some("invalid syntax")
                }
            } else if issue.message.contains("Parameter without a default") {
                Some("parameter without a default follows parameter with a default")
            } else {
                None
            };
            if let Some(reason) = reason {
                self.add(reason, issue.start);
            }
        }
    }

    fn aliases(&mut self) {
        let tokens = &self.document.tokens;
        for (index, token) in tokens.iter().enumerate() {
            if token.kind != "Except" {
                continue;
            }
            let end = tokens[index..]
                .iter()
                .position(|token| token.kind == "Newline")
                .map_or(tokens.len(), |end| index + end);
            let header = &tokens[index..end];
            let Some(as_index) = header.iter().position(|token| token.kind == "As") else {
                continue;
            };
            let payload = &header[as_index + 1..];
            let Some(first) = payload.iter().find(|token| !token.trivia) else {
                continue;
            };
            let Some(last) = payload.iter().rfind(|token| token.kind == "Colon") else {
                continue;
            };
            let text = &self.document.source[first.start as usize..last.start as usize];
            let parsed_text = format!("value = {text}\n");
            let Ok(parsed) = mamba_language::application::quality_syntax::parse(
                &parsed_text,
                PythonTarget::Py314,
                false,
            ) else {
                continue;
            };
            let Some(Stmt::Assign(assign)) = parsed.parsed.syntax().body.first() else {
                continue;
            };
            let expr = &assign.value;
            if expr.is_name_expr() && text.trim() == text.trim().trim_matches(['(', ')']) {
                continue;
            }
            let kind = if header.get(1).is_some_and(|token| token.kind == "Star") {
                "except*"
            } else {
                "except"
            };
            let reason = if self.document.target == PythonTarget::Py314 {
                if let Expr::Named(named) = expr.as_ref() {
                    if !named.target.is_name_expr() {
                        format!(
                            "cannot use assignment expressions with {}",
                            targets::name(&named.target)
                        )
                    } else {
                        format!("cannot use {kind} statement with named expression")
                    }
                } else {
                    format!("cannot use {kind} statement with {}", targets::name(expr))
                }
            } else {
                "invalid syntax".into()
            };
            self.errors
                .retain(|(_, offset)| *offset < token.start || *offset > last.end);
            let offset = if self.document.target == PythonTarget::Py314 {
                first.start + expr.start().to_u32().saturating_sub(8)
            } else {
                token.start
            };
            self.add(reason, offset);
        }
    }
}

fn template_lexical_error(document: &SyntaxDocument<'_>) -> Option<(String, u32)> {
    if document.target == PythonTarget::Py314
        || !document
            .issues
            .iter()
            .any(|issue| issue.version_error && issue.message.contains("t-strings"))
    {
        return None;
    }
    let mut projection = document.source.as_bytes().to_vec();
    for token in &document.tokens {
        if token.kind != "TStringStart" {
            continue;
        }
        for byte in &mut projection[token.start as usize..token.end as usize] {
            if matches!(*byte, b't' | b'T') {
                *byte = b' ';
            }
        }
    }
    let source = std::str::from_utf8(&projection).ok()?;
    let parsed =
        mamba_language::application::quality_syntax::parse(source, document.target, false).ok()?;
    let issue = parsed
        .issues
        .iter()
        .find(|issue| issue.message.contains("missing closing quote"))?;
    let line = parsed.line_column(issue.start).0;
    Some((
        format!("unterminated string literal (detected at line {line})"),
        issue.start,
    ))
}

impl<'ast> Visitor<'ast> for Check<'_, '_> {
    fn visit_with_item(&mut self, item: &'ast WithItem) {
        if let Some(target) = &item.optional_vars {
            if let Some(reason) = targets::assignment(target, false) {
                self.add(reason, target.start().to_u32());
            }
        }
        visitor::walk_with_item(self, item);
    }
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        match stmt {
            Stmt::Assign(stmt) => {
                for target in &stmt.targets {
                    if let Some(reason) = targets::assignment(target, true) {
                        self.add(reason, target.start().to_u32());
                    }
                }
            }
            Stmt::AnnAssign(stmt) => {
                let reason = if stmt.target.is_tuple_expr() {
                    Some("only single target (not tuple) can be annotated")
                } else if stmt.target.is_list_expr() {
                    Some("only single target (not list) can be annotated")
                } else if targets::invalid(&stmt.target).is_some() {
                    Some("illegal target for annotation")
                } else {
                    None
                };
                if let Some(reason) = reason {
                    self.add(reason, stmt.target.start().to_u32());
                }
            }
            Stmt::AugAssign(stmt) => {
                if targets::invalid(&stmt.target).is_some() {
                    self.add(
                        format!(
                            "'{}' is an illegal expression for augmented assignment",
                            targets::name(&stmt.target)
                        ),
                        stmt.target.start().to_u32(),
                    );
                }
            }
            Stmt::Delete(stmt) => {
                for target in &stmt.targets {
                    if let Some(target) = if target.is_starred_expr() {
                        Some(target)
                    } else {
                        targets::invalid(target)
                    } {
                        self.add(
                            format!("cannot delete {}", targets::name(target)),
                            target.start().to_u32(),
                        );
                    }
                }
            }
            Stmt::For(stmt) => {
                if let Some(reason) = targets::assignment(&stmt.target, false) {
                    self.add(reason, stmt.target.start().to_u32());
                }
            }
            Stmt::TypeAlias(stmt) if self.document.target == PythonTarget::Py313 => {
                if let Some(type_params) = &stmt.type_params {
                    for param in &type_params.type_params {
                        let default = match param {
                            TypeParam::TypeVar(param) => param.default.as_deref(),
                            TypeParam::TypeVarTuple(param) => param.default.as_deref(),
                            TypeParam::ParamSpec(param) => param.default.as_deref(),
                        };
                        if let Some(default) = default {
                            self.add(
                                "invalid syntax. Maybe you meant '==' or ':=' instead of '='?",
                                default.start().to_u32(),
                            );
                        }
                    }
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        if self.parenthesized_tstring_concat.is_none()
            && self.document.target != PythonTarget::Py314
        {
            if let Expr::TString(tstring) = expr {
                let string = StringLike::TString(tstring);
                if string.is_implicit_concatenated()
                    && enclosed_by_container(
                        self.document,
                        tstring.start().to_u32(),
                        tstring.end().to_u32(),
                    )
                {
                    self.parenthesized_tstring_concat = Some(tstring.start().to_u32());
                }
            }
        }
        if let Expr::Named(expr) = expr {
            if !expr.target.is_name_expr() {
                self.add(
                    format!(
                        "cannot use assignment expressions with {}",
                        targets::name(&expr.target)
                    ),
                    expr.target.start().to_u32(),
                );
            }
        }
        visitor::walk_expr(self, expr);
    }
}
