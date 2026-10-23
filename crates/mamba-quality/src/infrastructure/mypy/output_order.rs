//! Diagnostic block order from pinned Mypy 2.4.0 dependency-SCC rules.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet, VecDeque};

const PRI_INDIRECT: u8 = 30;

/// Return only reported source IDs in Mypy's diagnostic block order.
///
/// The caller must supply the canonical names and native first-admission ranks.
/// A None priority means a proved, resolved direct edge with priority < 30.
/// It must not represent an unresolved target or uncertain edge admission.
/// The outer schedule follows a cold serial build; inner SCC rules stay layered.
pub(super) fn file_order(
    names: &[String],
    encounter: &[usize],
    dependencies: &[Vec<(usize, Option<u8>)>],
    reported: &[bool],
) -> Option<Vec<usize>> {
    validate(names, encounter, dependencies, reported)?;
    let vertices: Vec<_> = (0..names.len()).collect();
    let components = sorted_components(&vertices, encounter, dependencies, PRI_INDIRECT)?;
    let components = serial_components(components, dependencies)?;
    let mut result = Vec::with_capacity(names.len());
    for component in components {
        let mut visible: Vec<_> = component
            .iter()
            .copied()
            .filter(|&id| reported[id])
            .collect();
        if visible.len() <= 1 {
            result.extend(visible);
        } else if component.iter().any(|&id| names[id] == "builtins") {
            // build.py:4478-4499 overwrites this actual SCC's member order.
            let builtins = visible.iter().copied().find(|&id| names[id] == "builtins");
            visible.sort_unstable_by(|&left, &right| names[right].cmp(&names[left]));
            if let Some(builtins) = builtins {
                visible.retain(|&id| id != builtins);
                visible.push(builtins);
            }
            result.extend(visible);
        } else {
            // All internal priorities must be known when order is observable.
            let ordered = order_ascc(component, encounter, dependencies, PRI_INDIRECT)?;
            result.extend(ordered.into_iter().filter(|&id| reported[id]));
        }
    }
    Some(result)
}

// build.py:1390-1395,1475-1479,4642-4680. Cold serial SCCs use a FIFO queue.
fn serial_components(
    mut components: Vec<Vec<usize>>,
    dependencies: &[Vec<(usize, Option<u8>)>],
) -> Option<Vec<Vec<usize>>> {
    let mut owners = vec![0; dependencies.len()];
    for (component, members) in components.iter().enumerate() {
        for &member in members {
            owners[member] = component;
        }
    }
    let mut remaining = vec![0usize; components.len()];
    let mut reverse = vec![Vec::new(); components.len()];
    let mut component_edges = HashSet::new();
    // Components already have sorted_components order. Appending their IDs here
    // matches build.py:5155-5158 direct_dependents insertion order.
    for (source, members) in components.iter().enumerate() {
        for &member in members {
            for &(target, priority) in &dependencies[member] {
                if priority.is_some_and(|priority| priority >= PRI_INDIRECT) {
                    continue;
                }
                let target = owners[target];
                if source != target && component_edges.insert((source, target)) {
                    remaining[source] += 1;
                    reverse[target].push(source);
                }
            }
        }
    }
    let mut ready: VecDeque<_> = remaining
        .iter()
        .enumerate()
        .filter_map(|(component, &count)| (count == 0).then_some(component))
        .collect();
    let mut result = Vec::with_capacity(components.len());
    while let Some(component) = ready.pop_front() {
        result.push(std::mem::take(&mut components[component]));
        for &dependent in &reverse[component] {
            remaining[dependent] = remaining[dependent].checked_sub(1)?;
            if remaining[dependent] == 0 {
                ready.push_back(dependent);
            }
        }
    }
    (result.len() == components.len()).then_some(result)
}

fn validate(
    names: &[String],
    encounter: &[usize],
    dependencies: &[Vec<(usize, Option<u8>)>],
    reported: &[bool],
) -> Option<()> {
    if names.len() != encounter.len()
        || names.len() != dependencies.len()
        || names.len() != reported.len()
    {
        return None;
    }
    let mut seen_names = HashSet::with_capacity(names.len());
    let mut seen_ranks = HashSet::with_capacity(names.len());
    for (name, &rank) in names.iter().zip(encounter) {
        if !seen_names.insert(name.as_str()) || !seen_ranks.insert(rank) {
            return None;
        }
    }
    for edges in dependencies {
        let mut priorities = HashMap::with_capacity(edges.len());
        for &(target, priority) in edges {
            if target >= names.len() || !matches!(priority, None | Some(5 | 10 | 20 | 25 | 30)) {
                return None;
            }
            // Mypy has one priority per dependency. Equal repeats are harmless.
            if let Some(previous) = priorities.insert(target, priority) {
                if previous != priority {
                    return None;
                }
            }
        }
    }
    Some(())
}

// build.py:4704-4747. A larger number denotes a lower import priority.
fn order_ascc(
    mut vertices: Vec<usize>,
    encounter: &[usize],
    dependencies: &[Vec<(usize, Option<u8>)>],
    pri_max: u8,
) -> Option<Vec<usize>> {
    if vertices.len() <= 1 {
        return Some(vertices);
    }
    let members: HashSet<_> = vertices.iter().copied().collect();
    let mut lowest = None;
    let mut highest = None;
    for &vertex in &vertices {
        for &(target, priority) in &dependencies[vertex] {
            if members.contains(&target) {
                let priority = priority?;
                if priority < pri_max {
                    lowest = Some(lowest.map_or(priority, |old: u8| old.max(priority)));
                    highest = Some(highest.map_or(priority, |old: u8| old.min(priority)));
                }
            }
        }
    }
    let lowest = lowest?;
    if highest == Some(lowest) {
        vertices.sort_unstable_by_key(|&vertex| Reverse(encounter[vertex]));
        return Some(vertices);
    }
    // Exclude all edges of the lowest remaining priority, then recurse.
    let components = sorted_components(&vertices, encounter, dependencies, lowest)?;
    let mut result = Vec::with_capacity(vertices.len());
    for component in components {
        result.extend(order_ascc(component, encounter, dependencies, lowest)?);
    }
    Some(result)
}

// build.py:5133-5194 and graph_utils.py:57-161, without build-state mutation.
fn sorted_components(
    vertices: &[usize],
    encounter: &[usize],
    dependencies: &[Vec<(usize, Option<u8>)>],
    pri_max: u8,
) -> Option<Vec<Vec<usize>>> {
    // Compact local IDs avoid a full-domain allocation for every inner SCC.
    let local_ids: HashMap<_, _> = vertices
        .iter()
        .enumerate()
        .map(|(local, &source)| (source, local))
        .collect();
    let edges: Vec<Vec<usize>> = vertices
        .iter()
        .map(|&source| {
            dependencies[source]
                .iter()
                .filter_map(|&(target, priority)| {
                    let local = local_ids.get(&target).copied()?;
                    // Unknown direct edges are admitted only at the outer limit.
                    let included = priority.map_or(pri_max == PRI_INDIRECT, |p| p < pri_max);
                    included.then_some(local)
                })
                .collect()
        })
        .collect();
    let mut components = strongly_connected_components(&edges);
    let mut owners = vec![0; vertices.len()];
    let mut minimum = Vec::with_capacity(components.len());
    for (component, members) in components.iter().enumerate() {
        let mut rank = usize::MAX;
        for &member in members {
            owners[member] = component;
            rank = rank.min(encounter[vertices[member]]);
        }
        minimum.push(rank);
    }
    let mut remaining = vec![0usize; components.len()];
    let mut reverse = vec![Vec::new(); components.len()];
    let mut component_edges = HashSet::new();
    for (source, targets) in edges.iter().enumerate() {
        for &target in targets {
            let (source, target) = (owners[source], owners[target]);
            if source != target && component_edges.insert((source, target)) {
                remaining[source] += 1;
                reverse[target].push(source);
            }
        }
    }
    let mut ready: Vec<_> = remaining
        .iter()
        .enumerate()
        .filter_map(|(component, &count)| (count == 0).then_some(component))
        .collect();
    let mut result = Vec::with_capacity(components.len());
    while !ready.is_empty() {
        // Each batch is fixed before its dependents become the next batch.
        ready.sort_unstable_by_key(|&component| Reverse(minimum[component]));
        let mut next = Vec::new();
        for component in ready {
            let members = std::mem::take(&mut components[component]);
            result.push(members.into_iter().map(|member| vertices[member]).collect());
            for &dependent in &reverse[component] {
                remaining[dependent] = remaining[dependent].checked_sub(1)?;
                if remaining[dependent] == 0 {
                    next.push(dependent);
                }
            }
        }
        ready = next;
    }
    (result.len() == components.len()).then_some(result)
}

// graph_utils.py:11-54. Explicit frames preserve its boundary-stack DFS.
fn strongly_connected_components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct Frame {
        vertex: usize,
        next: usize,
    }

    let mut identified = vec![false; edges.len()];
    let mut stack = Vec::new();
    let mut index = vec![usize::MAX; edges.len()];
    let mut boundaries = Vec::new();
    let mut frames = Vec::new();
    let mut components = Vec::new();
    for start in 0..edges.len() {
        if index[start] != usize::MAX {
            continue;
        }
        index[start] = stack.len();
        stack.push(start);
        boundaries.push(index[start]);
        frames.push(Frame {
            vertex: start,
            next: 0,
        });
        while let Some(frame) = frames.last_mut() {
            let vertex = frame.vertex;
            if let Some(&target) = edges[vertex].get(frame.next) {
                frame.next += 1;
                if index[target] == usize::MAX {
                    index[target] = stack.len();
                    stack.push(target);
                    boundaries.push(index[target]);
                    frames.push(Frame {
                        vertex: target,
                        next: 0,
                    });
                } else if !identified[target] {
                    while boundaries
                        .last()
                        .is_some_and(|&bound| index[target] < bound)
                    {
                        boundaries.pop();
                    }
                }
            } else {
                frames.pop();
                if boundaries.last().copied() == Some(index[vertex]) {
                    boundaries.pop();
                    let component = stack.split_off(index[vertex]);
                    for &member in &component {
                        identified[member] = true;
                    }
                    components.push(component);
                }
            }
        }
    }
    components
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outer_serial_queue_preserves_fifo_when_next_layer_ranks_differ() {
        let names: Vec<_> = ["L1", "L2", "D1", "D2"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let encounter = [4, 3, 1, 2];
        let dependencies = vec![vec![], vec![], vec![(0, Some(5))], vec![(1, Some(5))]];
        // Inner SCC helpers retain the original layer rule.
        assert_eq!(
            sorted_components(&[0, 1, 2, 3], &encounter, &dependencies, PRI_INDIRECT),
            Some(vec![vec![0], vec![1], vec![3], vec![2]])
        );
        assert_eq!(
            file_order(&names, &encounter, &dependencies, &[true; 4]),
            Some(vec![0, 1, 2, 3])
        );
        // Silent leaf modules must still affect the serial readiness order.
        assert_eq!(
            file_order(
                &names,
                &encounter,
                &dependencies,
                &[false, false, true, true]
            ),
            Some(vec![2, 3])
        );
    }
}
