//! IntFlag overlaps retain literal values, source order, aliases, and bit order.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, ExprName, Number, Stmt, StmtClassDef,
};
use num_bigint::BigInt;
use ruff_text_size::{Ranged, TextRange};
use std::collections::{BTreeMap, BTreeSet};

struct Flag {
    value: BigInt,
    display: String,
    assignments: Vec<(String, TextRange)>,
}
#[derive(Default)]
struct Flags(Vec<Flag>);
impl Flags {
    fn assignment(&mut self, name: &ExprName, value: BigInt, display: String) {
        let entry = if let Some(index) = self.0.iter().position(|flag| flag.value == value) {
            &mut self.0[index]
        } else {
            self.0.push(Flag {
                value,
                display,
                assignments: Vec::new(),
            });
            self.0.last_mut().expect("just inserted flag value")
        };
        entry.assignments.push((name.id.to_string(), name.range()));
    }
}
impl<'a> Visitor<'a> for Flags {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if let Stmt::Assign(assignment) = statement {
            let literal = match assignment.value.as_ref() {
                Expr::NumberLiteral(number) => match &number.value {
                    Number::Int(value) => value
                        .to_string()
                        .parse::<BigInt>()
                        .ok()
                        .map(|value| (value.clone(), value.to_string())),
                    _ => None,
                },
                Expr::BooleanLiteral(value) => Some((
                    BigInt::from(u8::from(value.value)),
                    if value.value { "True" } else { "False" }.into(),
                )),
                _ => None,
            };
            if let Some((value, display)) = literal {
                for target in &assignment.targets {
                    if let Expr::Name(name) = target {
                        self.assignment(name, value.clone(), display.clone());
                    }
                }
            }
        }
        walk_stmt(self, statement);
    }
    fn visit_expr(&mut self, _: &'a Expr) {}
}

fn overlaps(flags: &[Flag]) -> Vec<(usize, Vec<usize>)> {
    let mut bits = Vec::<BTreeSet<usize>>::new();
    let mut bit_indices = BTreeMap::new();
    for (index, flag) in flags.iter().enumerate() {
        for (bit, character) in flag.value.to_str_radix(2).chars().rev().enumerate() {
            if character != '1' {
                continue;
            }
            let slot = *bit_indices.entry(bit).or_insert_with(|| {
                bits.push(BTreeSet::new());
                bits.len() - 1
            });
            bits[slot].insert(index);
        }
    }
    let mut overlaps = Vec::<(usize, Vec<usize>)>::new();
    for bit in bits {
        let mut indices: Vec<_> = bit.into_iter().collect();
        indices.sort_by(|left, right| flags[*left].value.cmp(&flags[*right].value));
        let Some((&source, conflicts)) = indices.split_first() else {
            continue;
        };
        for conflict in conflicts {
            if let Some((_, sources)) = overlaps.iter_mut().find(|(index, _)| index == conflict) {
                sources.push(source);
            } else {
                overlaps.push((*conflict, vec![source]));
            }
        }
    }
    overlaps
}

impl Checker<'_> {
    pub(super) fn check_enum_flags(&mut self, statement: &StmtClassDef) {
        let mut flags = Flags::default();
        flags.visit_body(&statement.body);
        for (index, sources) in overlaps(&flags.0) {
            let flag = &flags.0[index];
            let sources = sources
                .into_iter()
                .map(|index| {
                    let source = &flags.0[index];
                    format!(
                        "<{}.{}: {}> ({} & {} = {})",
                        statement.name,
                        source.assignments[0].0,
                        source.display,
                        flag.display,
                        source.display,
                        &flag.value & &source.value
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            for (name, range) in &flag.assignments {
                self.emit(
                    "W0213",
                    "implicit-flag-alias",
                    &format!(
                        "Flag member <{}.{name}: {}> shares bit positions with {sources}",
                        statement.name, flag.display
                    ),
                    *range,
                );
            }
        }
    }
}
