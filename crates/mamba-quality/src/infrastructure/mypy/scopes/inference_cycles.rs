//! Assignment cycles keep their first result while acyclic imports can refresh.
use super::*;
use mamba_language::application::quality_syntax::python_ast::StmtAssign;
use std::collections::BTreeSet;

pub(super) fn recursive_assignments(assignments: &[&StmtAssign]) -> Vec<bool> {
    let mut definitions = BTreeMap::new();
    for (index, assignment) in assignments.iter().enumerate() {
        for target in &assignment.targets {
            if let Expr::Name(name) = target {
                definitions.entry(name.id.as_str()).or_insert(index);
            }
        }
    }
    let edges: Vec<Vec<_>> = assignments
        .iter()
        .map(|assignment| {
            let mut names = BTreeSet::new();
            super::inference::dependencies(&assignment.value, &mut names);
            names
                .into_iter()
                .filter_map(|name| definitions.get(name).copied())
                .collect()
        })
        .collect();
    let mut state = vec![0; assignments.len()];
    let mut finished = Vec::new();
    for start in 0..assignments.len() {
        if state[start] != 0 {
            continue;
        }
        let mut stack = vec![(start, 0)];
        state[start] = 1;
        while let Some((index, cursor)) = stack.last_mut() {
            let Some(&next) = edges[*index].get(*cursor) else {
                state[*index] = 2;
                finished.push(*index);
                stack.pop();
                continue;
            };
            *cursor += 1;
            if state[next] == 0 {
                state[next] = 1;
                stack.push((next, 0));
            }
        }
    }
    let mut reverse = vec![Vec::new(); assignments.len()];
    for (index, neighbors) in edges.iter().enumerate() {
        for &neighbor in neighbors {
            reverse[neighbor].push(index);
        }
    }
    state.fill(0);
    let mut recursive = vec![false; assignments.len()];
    for start in finished.into_iter().rev() {
        if state[start] != 0 {
            continue;
        }
        let mut stack = vec![start];
        let mut component = Vec::new();
        state[start] = 1;
        while let Some(index) = stack.pop() {
            component.push(index);
            for &neighbor in &reverse[index] {
                if state[neighbor] == 0 {
                    state[neighbor] = 1;
                    stack.push(neighbor);
                }
            }
        }
        if component.len() > 1 || edges[start].contains(&start) {
            for index in component {
                recursive[index] = true;
            }
        }
    }
    recursive
}

#[cfg(test)]
mod tests {
    use super::*;
    use mamba_language::application::quality_syntax::{self, PythonTarget};

    #[test]
    fn all_three_binding_graphs_match_reachability() {
        let names = ["first", "second", "third"];
        for mask in 0..512u16 {
            let mut reaches = [[false; 3]; 3];
            let mut source = String::new();
            for (row, &name) in names.iter().enumerate() {
                let mut arguments = Vec::new();
                for (column, &argument) in names.iter().enumerate() {
                    if mask & (1 << (row * 3 + column)) != 0 {
                        reaches[row][column] = true;
                        arguments.push(argument);
                    }
                }
                source.push_str(&format!("{name} = function({})\n", arguments.join(", ")));
            }
            for middle in 0..3 {
                for from in 0..3 {
                    for to in 0..3 {
                        reaches[from][to] |= reaches[from][middle] && reaches[middle][to];
                    }
                }
            }
            let document = quality_syntax::parse(&source, PythonTarget::Py312, false).unwrap();
            assert!(document.is_valid());
            let assignments: Vec<_> = document
                .parsed
                .syntax()
                .body
                .iter()
                .filter_map(Stmt::as_assign_stmt)
                .collect();
            let expected: Vec<_> = (0..3).map(|index| reaches[index][index]).collect();
            assert_eq!(
                recursive_assignments(&assignments),
                expected,
                "graph {mask}"
            );
        }
    }
}
