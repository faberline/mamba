//! Adjacent assignments retain original three-statement swap detection.
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

pub(super) fn swaps(body: &[Stmt]) -> BTreeSet<u32> {
    let mut values = Swaps::default();
    values.visit_body(body);
    values.reports
}
#[derive(Default)]
struct Swaps {
    reports: BTreeSet<u32>,
    used: BTreeSet<u32>,
}
fn names(value: &Stmt) -> Option<(&str, &str)> {
    let Stmt::Assign(value) = value else {
        return None;
    };
    let [Expr::Name(left)] = value.targets.as_slice() else {
        return None;
    };
    let Expr::Name(right) = value.value.as_ref() else {
        return None;
    };
    Some((left.id.as_str(), right.id.as_str()))
}
impl<'a> Visitor<'a> for Swaps {
    fn visit_body(&mut self, body: &'a [Stmt]) {
        for group in body.windows(3) {
            if group
                .iter()
                .any(|value| self.used.contains(&value.start().to_u32()))
            {
                continue;
            }
            let Some(first) = names(&group[0]) else {
                continue;
            };
            let Some(second) = names(&group[1]) else {
                continue;
            };
            let Some(third) = names(&group[2]) else {
                continue;
            };
            if first.0 == third.1 && second.0 == first.1 && third.0 == second.1 {
                self.reports.insert(group[0].start().to_u32());
                self.used
                    .extend(group.iter().map(|value| value.start().to_u32()));
            }
        }
        for value in body {
            self.visit_stmt(value);
        }
    }
    fn visit_stmt(&mut self, value: &'a Stmt) {
        walk_stmt(self, value);
    }
}
