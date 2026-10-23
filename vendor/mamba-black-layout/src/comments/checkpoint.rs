//! Restore nested comments before rendering another layout candidate.
use crate::comments::{Comments, SourceComment};
use ruff_python_ast::{
    AnyNodeRef,
    visitor::source_order::{SourceOrderVisitor, TraversalSignal},
};
use ruff_text_size::{Ranged, TextRange};

pub(crate) struct Checkpoint<'c, 'ast> {
    comments: &'c Comments<'ast>,
    states: Vec<(&'c SourceComment, bool)>,
}

impl<'c, 'ast> Checkpoint<'c, 'ast> {
    pub(crate) fn new(comments: &'c Comments<'ast>, node: AnyNodeRef<'ast>) -> Self {
        let mut result = Self {
            comments,
            states: Vec::new(),
        };
        result.enter_node(node);
        node.visit_source_order(&mut result);
        result
    }

    pub(crate) fn restore(&self) {
        for (comment, formatted) in &self.states {
            if *formatted {
                comment.mark_formatted();
            } else {
                comment.mark_unformatted();
            }
        }
    }

    pub(crate) fn has_inner_comment(&self, range: TextRange, opening: &[SourceComment]) -> bool {
        self.states.iter().any(|(comment, formatted)| {
            !formatted
                && range.contains_range(comment.range())
                && !opening
                    .iter()
                    .any(|candidate| std::ptr::eq(candidate, *comment))
        })
    }
}

impl<'ast> SourceOrderVisitor<'ast> for Checkpoint<'_, 'ast> {
    fn enter_node(&mut self, node: AnyNodeRef<'ast>) -> TraversalSignal {
        self.states.extend(
            self.comments
                .leading_dangling_trailing(node)
                .into_iter()
                .map(|comment| (comment, comment.is_formatted())),
        );
        TraversalSignal::Traverse
    }
}
