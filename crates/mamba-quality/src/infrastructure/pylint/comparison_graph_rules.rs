//! Boolean comparison diagnostics retain graph cycles before chain suggestions.
use super::{
    comparison_graph::{Graph, Symbol},
    comparison_graph_build::{self, Segment},
    comparison_graph_cycles, comparison_graph_paths, comparison_graph_render, Checker,
};
use mamba_language::application::quality_syntax::python_ast::Expr;
use ruff_text_size::{Ranged, TextRange};

impl Checker<'_> {
    fn emit_comparison_cycles(&mut self, graph: &Graph, range: TextRange) -> bool {
        let mut cycles = comparison_graph_cycles::cycles(graph);
        if cycles.is_empty() {
            return false;
        }
        for cycle in &mut cycles {
            let first = cycle
                .iter()
                .position(|value| graph.operands[*value].numeric.is_none())
                .unwrap_or(0);
            cycle.rotate_left(first);
            let inclusive = cycle
                .iter()
                .copied()
                .zip(cycle.iter().copied().cycle().skip(1))
                .take(cycle.len())
                .all(|(left, right)| graph.symbols[&(left, right)] == Symbol::Inclusive);
            if !inclusive {
                self.emit(
                    "R1739",
                    "impossible-comparison",
                    "This comparison always evaluates to False",
                    range,
                );
                return true;
            }
        }
        for cycle in cycles {
            let suggestion = cycle
                .iter()
                .map(|value| graph.operands[*value].text.as_str())
                .collect::<Vec<_>>()
                .join(" == ");
            self.emit("R1738","chained-comparison-all-equal",&format!("Simplify chained comparison cycle to equality between the operands: {suggestion}"),range);
        }
        true
    }
    pub(super) fn check_comparison_graph(&mut self, value: &Expr) {
        let Expr::BoolOp(boolean) = value else {
            return;
        };
        let Some(mut segments) = comparison_graph_build::segments(boolean, &self.source.text)
        else {
            return;
        };
        for segment in &segments {
            if matches!(segment, Segment::Graph(graph) if self.emit_comparison_cycles(graph,boolean.range()))
            {
                return;
            }
        }
        let mut rendered = Vec::new();
        for segment in &mut segments {
            match segment {
                Segment::Boundary(text) => rendered.push(text.clone()),
                Segment::Graph(graph) => {
                    let paths = comparison_graph_paths::paths(graph);
                    rendered.extend(comparison_graph_render::paths(graph, &paths));
                }
            }
        }
        if rendered.len() < boolean.values.len() {
            let suggestion = comparison_graph_render::join(&rendered);
            self.emit(
                "R1716",
                "chained-comparison",
                &format!("Simplify chained comparison between the operands: {suggestion}"),
                boolean.range(),
            );
        }
    }
}
