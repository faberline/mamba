//! Comparison vertices keep numeric identity and original insertion order.
use super::{
    display,
    simplifiable_numbers::{self, Numeric},
};
use mamba_language::application::quality_syntax::python_ast::{Expr, Number, UnaryOp};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(super) struct Operand {
    pub text: String,
    pub numeric: Option<Numeric>,
}
impl Operand {
    pub fn same(&self, other: &Self) -> bool {
        match (&self.numeric, &other.numeric) {
            (Some(left), Some(right)) => left.compare(right).is_some_and(|order| order.is_eq()),
            (None, None) => self.text == other.text,
            _ => false,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Symbol {
    Strict,
    Inclusive,
    Equal,
}
impl Symbol {
    pub fn text(self, reversed: bool) -> &'static str {
        match (self, reversed) {
            (Self::Strict, false) => ">",
            (Self::Strict, true) => "<",
            (Self::Inclusive, false) => ">=",
            (Self::Inclusive, true) => "<=",
            (Self::Equal, _) => "==",
        }
    }
}
#[derive(Default)]
pub(super) struct Graph {
    pub operands: Vec<Operand>,
    pub edges: Vec<BTreeSet<usize>>,
    pub symbols: BTreeMap<(usize, usize), Symbol>,
    pub indegree: Vec<usize>,
    pub frequency: BTreeMap<(usize, usize), usize>,
    pub constants: Vec<usize>,
    pub source_count: usize,
}
impl Graph {
    pub fn intern(&mut self, value: Operand) -> usize {
        let numeric = value.numeric.is_some();
        let index = if let Some(index) = self.operands.iter().position(|other| value.same(other)) {
            index
        } else {
            let index = self.operands.len();
            self.operands.push(value);
            self.edges.push(BTreeSet::new());
            self.indegree.push(0);
            index
        };
        if numeric {
            self.constants.push(index);
        }
        index
    }
    pub fn add(&mut self, left: usize, right: usize, symbol: Symbol) {
        self.edges[left].insert(right);
        self.symbols.insert((left, right), symbol);
        self.indegree[right] += 1;
        *self.frequency.entry((left, right)).or_default() += 1;
    }
}
pub(super) fn numeric_literal(value: &Expr) -> bool {
    match value {
        Expr::NumberLiteral(value) => matches!(value.value, Number::Int(_) | Number::Float(_)),
        Expr::BooleanLiteral(_) => true,
        Expr::UnaryOp(value) if matches!(value.op, UnaryOp::UAdd | UnaryOp::USub) => {
            numeric_literal(&value.operand)
        }
        _ => false,
    }
}
fn float(value: f64) -> String {
    let text = format!("{value:?}");
    if let Some((mantissa, exponent)) = text.split_once('e') {
        let exponent: i32 = exponent.parse().expect("Rust float exponent");
        format!("{mantissa}e{exponent:+03}")
    } else {
        text
    }
}
pub(super) fn operand(value: &Expr, source: &str, usable: &BTreeSet<String>) -> Option<Operand> {
    if numeric_literal(value) {
        let numeric = simplifiable_numbers::number(value)?;
        let text = match (&numeric, value) {
            (_, Expr::BooleanLiteral(_)) => display::expression(value, source),
            (Numeric::Integer(value), _) => value.to_string(),
            (Numeric::Float(value), _) => float(*value),
            _ => return None,
        };
        return Some(Operand {
            text,
            numeric: Some(numeric),
        });
    }
    let text = display::expression(value, source);
    if matches!(value, Expr::Name(_))
        || (usable.contains(&text) && !matches!(value, Expr::NoneLiteral(_)))
    {
        Some(Operand {
            text,
            numeric: None,
        })
    } else {
        None
    }
}
