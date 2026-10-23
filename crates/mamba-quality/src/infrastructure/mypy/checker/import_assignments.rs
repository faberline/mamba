//! Imports over a retained source function check imported value against local value.
use super::*;

impl Checker<'_> {
    pub(super) fn check_import_assignments(&mut self, statement: &Stmt) {
        for (name, local, imported) in self
            .scopes
            .import_assignments(std::slice::from_ref(statement))
        {
            if !self.context.accepts(local, imported) {
                self.emit(
                    "assignment",
                    format!(
                        "Incompatible import of \"{name}\" (imported name has type \"{}\", local name has type \"{}\")",
                        self.context.name(imported),
                        self.context.name(local),
                    ),
                    statement,
                );
            }
        }
    }
}
