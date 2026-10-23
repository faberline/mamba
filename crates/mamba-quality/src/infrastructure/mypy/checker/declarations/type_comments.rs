//! Annotation presence follows ast_serialize 0.12.1's native parser attachment.
//!
//! The token splitter and comment classification below are adapted from
//! ast_serialize 0.12.1, src/type_comment.rs and src/serialize_ast.rs.
//! Upstream: https://pypi.org/project/ast-serialize/0.12.1/
//!
//! The MIT License
//! Copyright (c) 2025-2026 Jukka Lehtosalo and contributors
//!
//! Permission is hereby granted, free of charge, to any person obtaining a
//! copy of this software and associated documentation files (the "Software"),
//! to deal in the Software without restriction, including without limitation
//! the rights to use, copy, modify, merge, publish, distribute, sublicense,
//! and/or sell copies of the Software, and to permit persons to whom the
//! Software is furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in
//! all copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
//! FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
//! DEALINGS IN THE SOFTWARE.
use super::*;
use quality_syntax::python_ast::token::TokenKind;
use ruff_source_file::LineIndex;
use ruff_text_size::TextSize;

#[derive(Clone, Copy)]
enum Kind {
    Regular,
    Function { arguments: usize, ellipsis: bool },
    Invalid,
}

pub(in crate::infrastructure::mypy::checker) struct TypeComments {
    lines: BTreeMap<u32, Kind>,
    comments: Vec<(TextSize, u32)>,
    colons: Vec<TextSize>,
    line_index: LineIndex,
}

pub(super) struct Annotations {
    pub(super) parameters: Vec<bool>,
    pub(super) returns: bool,
}

impl TypeComments {
    pub(in crate::infrastructure::mypy::checker) fn collect(
        document: &quality_syntax::SyntaxDocument<'_>,
    ) -> Self {
        let line_index = LineIndex::from_source_text(document.source);
        let mut result = Self {
            lines: BTreeMap::new(),
            comments: Vec::new(),
            colons: Vec::new(),
            line_index,
        };
        for token in document.parsed.tokens().iter() {
            match token.kind() {
                TokenKind::Colon => result.colons.push(token.start()),
                TokenKind::Comment => {
                    let line = result.line(token.start(), document.source);
                    result.comments.push((token.start(), line));
                    let text = &document.source[token.start().to_usize()..token.end().to_usize()];
                    for annotation in annotation_parts(text) {
                        result
                            .lines
                            .entry(line)
                            .or_insert_with(|| classify(annotation));
                    }
                }
                _ => {}
            }
        }
        result
    }

    pub(super) fn annotations(
        &self,
        function: &StmtFunctionDef,
        in_class: bool,
        source: &str,
    ) -> Annotations {
        let mut comments = vec![false; function.parameters.len()];
        let mut returns = false;
        if let Some(first_statement) = function.body.first() {
            let end = first_statement.start();
            let colon = self.colons.partition_point(|&start| start < end);
            if let Some(&start) = colon
                .checked_sub(1)
                .and_then(|index| self.colons.get(index))
            {
                let first = self
                    .comments
                    .partition_point(|&(offset, _)| offset <= start);
                let last = self.comments.partition_point(|&(offset, _)| offset < end);
                for &(_, line) in self.comments[first..last].iter().rev() {
                    match self.lines.get(&line) {
                        Some(Kind::Function {
                            arguments,
                            ellipsis,
                        }) => {
                            returns = true;
                            if !*ellipsis {
                                if *arguments == comments.len() {
                                    comments.fill(true);
                                } else if in_class
                                    && comments.len().checked_sub(1) == Some(*arguments)
                                {
                                    comments[1..].fill(true);
                                }
                            }
                            break;
                        }
                        Some(Kind::Invalid) => break,
                        _ => {}
                    }
                }
            }
        }
        let parameters = function
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                parameter.annotation().is_some()
                    || comments[index]
                    || matches!(
                        self.lines.get(&self.line(parameter.start(), source)),
                        Some(Kind::Regular)
                    )
            })
            .collect();
        Annotations {
            parameters,
            returns: function.returns.is_some() || returns,
        }
    }

    fn line(&self, offset: TextSize, source: &str) -> u32 {
        self.line_index.line_column(offset, source).line.get() as u32
    }
}

fn annotation_parts(mut comment: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    while let Some(after_hash) = comment.strip_prefix('#') {
        let text = after_hash.trim_start();
        let mypy = text.starts_with("mypy:");
        let Some(after_type) = text
            .strip_prefix("type:")
            .or_else(|| text.strip_prefix("mypy:"))
        else {
            break;
        };
        let after_type = after_type.trim_start();
        let ignore = after_type.strip_prefix("ignore").is_some_and(|suffix| {
            suffix.is_empty()
                || suffix.starts_with(|character: char| {
                    character.is_whitespace() || matches!(character, '[' | '#')
                })
        });
        if !ignore && (!mypy || after_type.starts_with("ignore")) {
            parts.push(
                after_type
                    .split_once('#')
                    .map_or(after_type, |(value, _)| value)
                    .trim_end(),
            );
        }
        if mypy && !after_type.starts_with("ignore") {
            break;
        }
        let Some(hash) = after_type.find('#') else {
            break;
        };
        comment = &after_type[hash..];
    }
    parts
}

fn expression(source: &str) -> Option<bool> {
    let wrapped = format!("({source})");
    let document =
        quality_syntax::parse(&wrapped, quality_syntax::PythonTarget::Py314, false).ok()?;
    if !document.parsed.errors().is_empty() {
        return None;
    }
    let [Stmt::Expr(statement)] = document.parsed.syntax().body.as_slice() else {
        return None;
    };
    Some(matches!(statement.value.as_ref(), Expr::EllipsisLiteral(_)))
}

fn classify(annotation: &str) -> Kind {
    if annotation.is_empty() {
        return Kind::Invalid;
    }
    if expression(annotation).is_some() {
        return Kind::Regular;
    }
    let Some((arguments, returns)) = function_parts(annotation) else {
        return Kind::Invalid;
    };
    if expression(&returns).is_none() {
        return Kind::Invalid;
    }
    let Some(arguments) = arguments
        .iter()
        .map(|argument| expression(argument))
        .collect::<Option<Vec<_>>>()
    else {
        return Kind::Invalid;
    };
    let ellipsis = matches!(arguments.as_slice(), [true]);
    Kind::Function {
        arguments: arguments.len(),
        ellipsis,
    }
}

// This keeps upstream's native-token nesting rules, including stars and trailing comma.
fn function_parts(comment: &str) -> Option<(Vec<String>, String)> {
    let document =
        quality_syntax::parse(comment, quality_syntax::PythonTarget::Py314, false).ok()?;
    let mut tokens = document.parsed.tokens().iter_with_context();
    let mut arguments = Vec::new();
    if tokens.next()?.kind() != TokenKind::Lpar {
        return None;
    }
    let mut start = tokens.peek()?.start();
    let mut end = start;
    let mut star_stripped = false;
    loop {
        let token = tokens.next()?;
        match token.kind() {
            TokenKind::Comma if tokens.nesting() == 1 => {
                arguments.push(comment[start.to_usize()..end.to_usize()].to_string());
                start = tokens.peek()?.start();
                end = start;
                star_stripped = false;
            }
            TokenKind::Star | TokenKind::DoubleStar if tokens.nesting() == 1 => {
                if star_stripped {
                    return None;
                }
                if start == token.start() {
                    start = tokens.peek()?.start();
                    end = start;
                    star_stripped = true;
                }
            }
            TokenKind::Rpar if tokens.nesting() == 0 => {
                if end != start {
                    arguments.push(comment[start.to_usize()..end.to_usize()].to_string());
                }
                break;
            }
            _ => end = token.end(),
        }
    }
    if tokens.next()?.kind() != TokenKind::Rarrow {
        return None;
    }
    let returns = &comment[tokens.next()?.start().to_usize()..];
    Some((arguments, returns.to_string()))
}
