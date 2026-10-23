//! Nested block counts follow original direct parents and global leave ordering.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_except_handler, walk_match_case, walk_stmt, Visitor},
    ElifElseClause, ExceptHandler, MatchCase, Stmt, StmtIf,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Id(u8, u32);
#[derive(Clone, Copy)]
struct Context {
    parent: Id,
    function: bool,
    top: bool,
    elif: bool,
    range: TextRange,
}
#[derive(Default)]
struct Parents {
    parent: Option<Id>,
    function: Option<Id>,
    contexts: BTreeMap<Id, Context>,
}
impl Parents {
    fn add(&mut self, id: Id, range: TextRange, elif: bool) {
        let parent = self.parent.unwrap_or(Id(4, 0));
        self.contexts.insert(
            id,
            Context {
                parent,
                function: self.function.is_some(),
                top: self.function == Some(parent),
                elif,
                range,
            },
        );
    }
}
impl<'a> Visitor<'a> for Parents {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        let id = Id(0, statement.start().to_u32());
        if matches!(
            statement,
            Stmt::If(_) | Stmt::For(_) | Stmt::While(_) | Stmt::Try(_)
        ) {
            self.add(id, statement.range(), false);
        }
        let previous = (self.parent, self.function);
        self.parent = Some(id);
        if matches!(statement, Stmt::FunctionDef(_)) {
            self.function = Some(id);
        } else if matches!(statement, Stmt::ClassDef(_)) {
            self.function = None;
        }
        if let Stmt::If(conditional) = statement {
            self.visit_body(&conditional.body);
            for clause in &conditional.elif_else_clauses {
                if clause.test.is_some() {
                    let clause_id = Id(3, clause.start().to_u32());
                    self.add(
                        clause_id,
                        TextRange::new(clause.start(), conditional.end()),
                        true,
                    );
                    self.parent = Some(clause_id);
                }
                self.visit_body(&clause.body);
            }
        } else {
            walk_stmt(self, statement);
        }
        (self.parent, self.function) = previous;
    }
    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let previous = self.parent;
        self.parent = Some(Id(1, handler.start().to_u32()));
        walk_except_handler(self, handler);
        self.parent = previous;
    }
    fn visit_match_case(&mut self, case: &'a MatchCase) {
        let previous = self.parent;
        self.parent = Some(Id(2, case.start().to_u32()));
        walk_match_case(self, case);
        self.parent = previous;
    }
}

pub(super) struct Blocks {
    contexts: BTreeMap<Id, Context>,
    stack: Vec<(Id, TextRange)>,
}
impl Blocks {
    pub(super) fn new(body: &[Stmt]) -> Self {
        let mut parents = Parents::default();
        parents.visit_body(body);
        Self {
            contexts: parents.contexts,
            stack: Vec::new(),
        }
    }
    fn visit(&mut self, id: Id) -> Option<(TextRange, usize)> {
        let context = self.contexts.get(&id)?;
        if !context.function {
            return None;
        }
        let previous = self.stack.clone();
        if context.top {
            self.stack = vec![(id, context.range)];
        } else {
            while self
                .stack
                .last()
                .is_some_and(|(id, _)| *id != context.parent)
            {
                self.stack.pop();
            }
            if context.elif {
                self.stack.pop();
            }
            self.stack.push((id, context.range));
        }
        if previous.len() > self.stack.len() && previous.len() > 5 {
            Some((previous[0].1, previous.len()))
        } else {
            None
        }
    }
    fn finish(&mut self) -> Option<(TextRange, usize)> {
        let previous = std::mem::take(&mut self.stack);
        (previous.len() > 5).then(|| (previous[0].1, previous.len()))
    }
}

impl Checker<'_> {
    fn emit_nested_blocks(&mut self, message: Option<(TextRange, usize)>) {
        if let Some((range, depth)) = message {
            self.emit(
                "R1702",
                "too-many-nested-blocks",
                &format!("Too many nested blocks ({depth}/5)"),
                range,
            );
        }
    }
    pub(super) fn check_nested_block(&mut self, statement: &Stmt) {
        let message = self.nested_blocks.visit(Id(0, statement.start().to_u32()));
        self.emit_nested_blocks(message);
    }
    pub(super) fn check_nested_if(&mut self, statement: &StmtIf) {
        let message = self.nested_blocks.visit(Id(0, statement.start().to_u32()));
        self.emit_nested_blocks(message);
    }
    pub(super) fn check_nested_elif(&mut self, clause: &ElifElseClause) {
        let message = self.nested_blocks.visit(Id(3, clause.start().to_u32()));
        self.emit_nested_blocks(message);
    }
    pub(super) fn finish_nested_blocks(&mut self) {
        let message = self.nested_blocks.finish();
        self.emit_nested_blocks(message);
    }
}
