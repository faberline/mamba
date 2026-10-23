//! Original path extraction prioritizes symbolic vertices and repeated-edge counts.
use super::comparison_graph::Graph;
use std::collections::BTreeSet;

fn size(graph: &Graph, vertex: usize, cache: &mut [Option<(usize, usize)>]) -> (usize, usize) {
    if let Some(value) = cache[vertex] {
        return value;
    }
    let next: Vec<_> = graph.edges[vertex]
        .iter()
        .copied()
        .filter(|next| graph.frequency.get(&(vertex, *next)).copied().unwrap_or(0) > 0)
        .collect();
    let children: Vec<_> = next
        .into_iter()
        .map(|next| size(graph, next, cache))
        .collect();
    let symbolic = usize::from(graph.operands[vertex].numeric.is_none());
    let value = (
        symbolic + children.iter().map(|value| value.0).max().unwrap_or(0),
        1 + children.iter().map(|value| value.1).max().unwrap_or(0),
    );
    cache[vertex] = Some(value);
    value
}
fn longest(graph: &Graph, vertices: &BTreeSet<usize>, cache: &[Option<(usize, usize)>]) -> usize {
    *vertices
        .iter()
        .max_by_key(|vertex| {
            (
                cache[**vertex].expect("reachable path counted"),
                &graph.operands[**vertex].text,
            )
        })
        .expect("nonempty path candidates")
}
fn path(
    graph: &Graph,
    vertex: usize,
    roots: &mut BTreeSet<usize>,
    cache: &[Option<(usize, usize)>],
    result: &mut Vec<usize>,
) {
    result.push(vertex);
    let next: BTreeSet<_> = graph.edges[vertex]
        .iter()
        .copied()
        .filter(|next| graph.frequency.get(&(vertex, *next)).copied().unwrap_or(0) > 0)
        .collect();
    if next.is_empty() {
        return;
    }
    let selected = longest(graph, &next, cache);
    path(graph, selected, roots, cache, result);
    if next.len() >= 2 || graph.frequency[&(vertex, selected)] >= 2 {
        roots.insert(vertex);
    }
}
pub(super) fn paths(graph: &mut Graph) -> Vec<Vec<usize>> {
    let mut roots: BTreeSet<_> = graph
        .indegree
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (*value == 0).then_some(index))
        .collect();
    let mut result = BTreeSet::new();
    while !roots.is_empty() {
        let mut cache = vec![None; graph.operands.len()];
        for root in &roots {
            size(graph, *root, &mut cache);
        }
        let selected = longest(graph, &roots, &cache);
        roots.remove(&selected);
        let mut current = Vec::new();
        path(graph, selected, &mut roots, &cache, &mut current);
        for (i, right) in current.iter().enumerate() {
            for left in &current[..i] {
                let count = graph.frequency.entry((*left, *right)).or_default();
                *count = count.saturating_sub(1);
            }
        }
        let mut low = 0;
        let mut high = current.len() - 1;
        while low < high
            && graph.operands[current[low]].numeric.is_some()
            && graph.operands[current[low + 1]].numeric.is_some()
        {
            low += 1;
        }
        while high > low
            && graph.operands[current[high]].numeric.is_some()
            && graph.operands[current[high - 1]].numeric.is_some()
        {
            high -= 1;
        }
        let current = current[low..=high].to_vec();
        if current.len() > 1 {
            result.insert(current);
        }
    }
    result.into_iter().collect()
}
