//! Comparison runs retain source-order boundaries and connected constant linking.
use super::{
    comparison_graph::{self, Graph, Symbol},
    display,
};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, Visitor},
    BoolOp, CmpOp, Expr, ExprBoolOp,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) enum Segment {
    Graph(Graph),
    Boundary(String),
}
#[derive(Default)]
struct Impure(bool);
impl<'a> Visitor<'a> for Impure {
    fn visit_expr(&mut self, value: &'a Expr) {
        if matches!(
            value,
            Expr::Await(_) | Expr::Call(_) | Expr::Named(_) | Expr::Yield(_) | Expr::YieldFrom(_)
        ) {
            self.0 = true;
        } else if !self.0 {
            walk_expr(self, value);
        }
    }
}
fn usable(value: &ExprBoolOp, source: &str) -> BTreeSet<String> {
    let mut counts = BTreeMap::<String, usize>::new();
    let mut impure = BTreeSet::new();
    for value in &value.values {
        let Expr::Compare(value) = value else {
            continue;
        };
        for operand in &value.operands {
            if matches!(operand, Expr::Name(_)) || comparison_graph::numeric_literal(operand) {
                continue;
            }
            let text = display::expression(operand, source);
            *counts.entry(text.clone()).or_default() += 1;
            let mut visitor = Impure::default();
            visitor.visit_expr(operand);
            if visitor.0 {
                impure.insert(text);
            }
        }
    }
    counts
        .into_iter()
        .filter_map(|(text, count)| (count == 1 || !impure.contains(&text)).then_some(text))
        .collect()
}
fn boundary(value: &Expr, source: &str) -> String {
    let text = display::expression(value, source);
    if matches!(
        value,
        Expr::If(_) | Expr::Lambda(_) | Expr::Yield(_) | Expr::YieldFrom(_)
    ) || matches!(value,Expr::BoolOp(value) if value.op==BoolOp::Or)
    {
        format!("({text})")
    } else {
        text
    }
}
fn add(value: &Expr, graph: &mut Graph, source: &str, usable: &BTreeSet<String>) -> bool {
    let Expr::Compare(value) = value else {
        return false;
    };
    if !value.ops.iter().all(|op| {
        matches!(
            op,
            CmpOp::Lt | CmpOp::LtE | CmpOp::Gt | CmpOp::GtE | CmpOp::Eq
        )
    }) {
        return false;
    }
    let Some(operands) = value
        .operands
        .iter()
        .map(|value| comparison_graph::operand(value, source, usable))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    if operands.windows(2).any(|pair| pair[0].same(&pair[1])) {
        return false;
    }
    let operands: Vec<_> = operands
        .into_iter()
        .map(|value| graph.intern(value))
        .collect();
    for (pair, op) in operands.windows(2).zip(&value.ops) {
        let (left, right, symbol) = match op {
            CmpOp::Lt => (pair[1], pair[0], Symbol::Strict),
            CmpOp::LtE => (pair[1], pair[0], Symbol::Inclusive),
            CmpOp::Gt => (pair[0], pair[1], Symbol::Strict),
            CmpOp::GtE => (pair[0], pair[1], Symbol::Inclusive),
            CmpOp::Eq => (pair[0], pair[1], Symbol::Equal),
            _ => unreachable!("processable comparison operator"),
        };
        graph.add(left, right, symbol);
    }
    graph.source_count += 1;
    true
}
fn components(graph: &Graph) -> Vec<usize> {
    let mut neighbors = graph.edges.clone();
    for (left, rights) in graph.edges.iter().enumerate() {
        for right in rights {
            neighbors[*right].insert(left);
        }
    }
    let mut components = vec![usize::MAX; graph.operands.len()];
    for start in 0..components.len() {
        if components[start] != usize::MAX {
            continue;
        }
        let mut pending = vec![start];
        while let Some(operand) = pending.pop() {
            if components[operand] != usize::MAX {
                continue;
            }
            components[operand] = start;
            pending.extend(&neighbors[operand]);
        }
    }
    components
}
fn link_constants(graph: &mut Graph) {
    let components = components(graph);
    let mut constants = graph.constants.clone();
    constants.sort_by(|left, right| {
        graph.operands[*left]
            .numeric
            .as_ref()
            .unwrap()
            .compare(graph.operands[*right].numeric.as_ref().unwrap())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    while let Some(largest) = constants.pop() {
        let smaller: BTreeSet<_> = constants.iter().copied().collect();
        for smaller in smaller {
            if components[largest] != components[smaller]
                || !graph.operands[smaller]
                    .numeric
                    .as_ref()
                    .unwrap()
                    .compare(graph.operands[largest].numeric.as_ref().unwrap())
                    .is_some_and(|order| order.is_lt())
            {
                continue;
            }
            graph.add(largest, smaller, Symbol::Strict);
            let shortcuts: Vec<_> = graph.edges[smaller]
                .iter()
                .copied()
                .filter(|adj| graph.operands[*adj].numeric.is_none())
                .collect();
            for adj in shortcuts {
                graph.edges[largest].remove(&adj);
            }
        }
    }
}
pub(super) fn segments(value: &ExprBoolOp, source: &str) -> Option<Vec<Segment>> {
    if value.op != BoolOp::And || value.values.len() < 2 {
        return None;
    }
    let usable = usable(value, source);
    let mut segments = Vec::new();
    let mut current = Graph::default();
    for value in &value.values {
        if add(value, &mut current, source, &usable) {
            continue;
        }
        if current.source_count > 0 {
            segments.push(Segment::Graph(std::mem::take(&mut current)));
        }
        segments.push(Segment::Boundary(boundary(value, source)));
    }
    if current.source_count > 0 {
        segments.push(Segment::Graph(current));
    }
    let mut found = false;
    let mut inequality = false;
    for segment in &segments {
        if let Segment::Graph(graph) = segment {
            found = true;
            if graph.operands.len() > 64 {
                return None;
            }
            inequality |= graph
                .symbols
                .values()
                .any(|symbol| *symbol != Symbol::Equal);
        }
    }
    if !found || !inequality {
        return None;
    }
    for segment in &mut segments {
        if let Segment::Graph(graph) = segment {
            link_constants(graph);
        }
    }
    Some(segments)
}
