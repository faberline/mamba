//! Receiver assignment checks preserve direct name and tuple target boundaries.
use super::{classes::MethodKind, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtAssign};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn check_receiver_assignment(&mut self, assignment: &StmtAssign) {
        let Some((receiver, kind)) = self.scopes.assignment_receiver().cloned() else {
            return;
        };
        if kind == MethodKind::Static {
            return;
        }
        let names: Vec<_> = assignment
            .targets
            .iter()
            .flat_map(|target| match target {
                Expr::Name(value) => vec![value.id.as_str()],
                Expr::Tuple(value) => value
                    .elts
                    .iter()
                    .filter_map(|value| {
                        if let Expr::Name(value) = value {
                            Some(value.id.as_str())
                        } else {
                            None
                        }
                    })
                    .collect(),
                _ => Vec::new(),
            })
            .collect();
        if names.contains(&receiver.as_str()) {
            self.emit(
                "W0642",
                "self-cls-assignment",
                &format!("Invalid assignment to {receiver} in method"),
                assignment.range(),
            );
        }
    }
}
