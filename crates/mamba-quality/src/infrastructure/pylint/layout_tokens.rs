//! Physical token ranges are indexed once for line-layout checks.
use mamba_language::application::quality_syntax::SyntaxDocument;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub(super) struct Lines {
    pub newlines: BTreeSet<usize>,
    pub semicolons: BTreeSet<u32>,
}
impl Lines {
    pub(super) fn new(document: Option<&SyntaxDocument<'_>>) -> Self {
        let Some(document) = document else {
            return Self::default();
        };
        let index = ruff_source_file::LineIndex::from_source_text(document.source);
        let mut last = BTreeMap::new();
        let mut newlines = BTreeSet::new();
        for token in &document.tokens {
            if token.kind == "Newline" {
                newlines.insert(token.start as usize);
            }
            if matches!(
                token.kind.as_str(),
                "Whitespace"
                    | "Comment"
                    | "Newline"
                    | "NonLogicalNewline"
                    | "TrailingTrivia"
                    | "EndOfFile"
                    | "Indent"
                    | "Dedent"
            ) {
                continue;
            }
            let line = index
                .line_column(ruff_text_size::TextSize::from(token.start), document.source)
                .line
                .get() as u32;
            last.insert(line, token.kind == "Semi");
        }
        Self {
            newlines,
            semicolons: last
                .into_iter()
                .filter_map(|(line, value)| value.then_some(line))
                .collect(),
        }
    }
}
