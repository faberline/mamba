//! Comparison cycles retain canonical starts and distinct discovery paths.
use super::comparison_graph::Graph;
use std::collections::BTreeSet;

fn visit(
    graph: &Graph,
    vertex: usize,
    path: &mut Vec<usize>,
    visited: &mut BTreeSet<usize>,
    result: &mut Vec<Vec<usize>>,
) {
    if let Some(start) = path.iter().position(|value| *value == vertex) {
        let mut cycle = path[start..].to_vec();
        let first = cycle
            .iter()
            .enumerate()
            .min_by_key(|(_, value)| &graph.operands[**value].text)
            .unwrap()
            .0;
        cycle.rotate_left(first);
        if !result.contains(&cycle) {
            result.push(cycle);
        }
        return;
    }
    path.push(vertex);
    for next in &graph.edges[vertex] {
        if !visited.contains(next) {
            visit(graph, *next, path, visited, result);
            visited.insert(*next);
        }
    }
    path.pop();
}
pub(super) fn cycles(graph: &Graph) -> Vec<Vec<usize>> {
    let mut result = Vec::new();
    for root in 0..graph.operands.len() {
        visit(
            graph,
            root,
            &mut Vec::new(),
            &mut BTreeSet::new(),
            &mut result,
        );
    }
    result
}
