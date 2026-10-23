//! Comparison suggestions keep literal equality boundaries and the original length limit.
use super::comparison_graph::{Graph, Symbol};

fn path(graph: &Graph, path: &[usize]) -> String {
    let symbols: Vec<_> = path
        .windows(2)
        .map(|pair| graph.symbols[&(pair[0], pair[1])])
        .collect();
    let direct = (graph.operands[path[0]].numeric.is_none()
        && path[1..]
            .iter()
            .all(|value| graph.operands[*value].numeric.is_some()))
        || symbols.iter().all(|symbol| *symbol == Symbol::Equal);
    let first = if direct {
        path[0]
    } else {
        path[path.len() - 1]
    };
    let mut rendered = graph.operands[first].text.clone();
    if direct {
        for (pair, symbol) in path.windows(2).zip(symbols) {
            rendered.push_str(&format!(
                " {} {}",
                symbol.text(false),
                graph.operands[pair[1]].text
            ));
        }
    } else {
        for (pair, symbol) in path.windows(2).zip(symbols).rev() {
            rendered.push_str(&format!(
                " {} {}",
                symbol.text(true),
                graph.operands[pair[0]].text
            ));
        }
    }
    rendered
}
pub(super) fn paths(graph: &Graph, paths: &[Vec<usize>]) -> Vec<String> {
    let mut result = Vec::new();
    for values in paths {
        let mut start = 0;
        for index in 1..values.len() - 1 {
            if graph.operands[values[index]].numeric.is_some()
                && (graph.symbols[&(values[index - 1], values[index])] == Symbol::Equal
                    || graph.symbols[&(values[index], values[index + 1])] == Symbol::Equal)
            {
                result.push(path(graph, &values[start..=index]));
                start = index;
            }
        }
        result.push(path(graph, &values[start..]));
    }
    result.sort();
    result
}
pub(super) fn join(parts: &[String]) -> String {
    let mut result = String::new();
    for part in parts {
        if !result.is_empty() && result.chars().count() >= 64 {
            result.push_str(" and ...");
            return result;
        }
        if !result.is_empty() {
            result.push_str(" and ");
        }
        result.push_str(part);
    }
    result
}
