//! Import comment ownership follows the pinned isort parser.
use super::model::{add_comment, Import};
use mamba_language::application::quality_syntax::SyntaxDocument;

pub fn collect(document: &SyntaxDocument<'_>, start: usize, end: usize, import: &mut Import) {
    let source = document.source;
    let first_end = super::line_end(source, start);
    let mut general = Vec::new();
    for token in document.tokens.iter().filter(|token| {
        token.kind == "Comment" && token.start as usize >= start && (token.start as usize) < end
    }) {
        let offset = token.start as usize;
        let comment = document.text(token);
        let prefix = &source[super::line_start(source, offset)..offset];
        let first_line = offset < first_end;
        let code = if first_line {
            document
                .tokens
                .iter()
                .rev()
                .find(|token| {
                    token.kind == "Import"
                        && token.start as usize >= start
                        && token.end as usize <= offset
                })
                .map_or("", |keyword| &source[keyword.end as usize..offset])
        } else {
            prefix
        };
        let code = code.replace(['(', ')', ',', '\\'], " ");
        let code = code.split_whitespace().collect::<Vec<_>>().join(" ");
        let nested = import.from && (!first_line || !code.contains(" as "));
        if let Some(name) = import
            .names
            .iter_mut()
            .find(|name| nested && code == super::render::binding(name))
        {
            name.comment.clear();
            add_comment(&mut name.comment, comment);
        } else {
            general.push(comment);
        }
    }
    if import.from {
        if let Some(alias) = import.names.iter_mut().find(|name| name.alias.is_some()) {
            for comment in general {
                add_comment(&mut alias.opening_comment, comment);
            }
        } else {
            for comment in general {
                add_comment(&mut import.comment, comment);
            }
        }
    } else if !import.names.is_empty() {
        let index = import
            .names
            .iter()
            .position(|name| name.alias.is_some())
            .unwrap_or(0);
        for comment in general {
            add_comment(&mut import.names[index].comment, comment);
        }
    }
}
