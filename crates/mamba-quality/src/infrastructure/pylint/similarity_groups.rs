//! Four-line rolling buckets retain original pairing and successive ranges.
use super::similarity_lines::Lines;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Chunk {
    pub file: usize,
    pub start: usize,
    pub end: usize,
}
pub(super) struct Similarity {
    pub count: usize,
    pub chunks: Vec<Chunk>,
}
type Buckets = BTreeMap<Vec<String>, Vec<usize>>;
fn buckets(lines: &Lines<'_>) -> Buckets {
    let mut result = BTreeMap::<_, Vec<_>>::new();
    for (index, window) in lines.stripped.windows(4).enumerate() {
        // Original sums the per-line hashes, so line order does not change a bucket.
        let mut key: Vec<_> = window.iter().map(|line| line.1.clone()).collect();
        key.sort();
        result.entry(key).or_default().push(index);
    }
    result
}
fn pair(
    first: &Lines<'_>,
    second: &Lines<'_>,
    a: &Buckets,
    b: &Buckets,
) -> Vec<(usize, usize, usize)> {
    let mut common: Vec<_> = a
        .iter()
        .filter_map(|(key, left)| b.get(key).map(|right| (left, right)))
        .collect();
    common.sort_by_key(|(left, _)| left[0]);
    let mut order = Vec::new();
    let mut counts = BTreeMap::new();
    for (left, right) in common {
        let pairs: Vec<_> = if left.len().saturating_mul(right.len()) > 500 {
            left.iter().copied().zip(right.iter().copied()).collect()
        } else {
            left.iter()
                .flat_map(|a| right.iter().map(move |b| (*a, *b)))
                .collect()
        };
        for key in pairs {
            order.push(key);
            counts.insert(key, 4);
        }
    }
    for key in &order {
        if !counts.contains_key(key) {
            continue;
        }
        let mut successor = (key.0 + 1, key.1 + 1);
        while counts.remove(&successor).is_some() {
            *counts.get_mut(key).unwrap() += 1;
            successor = (successor.0 + 1, successor.1 + 1);
        }
    }
    order
        .into_iter()
        .filter_map(|(a, b)| {
            let count = *counts.get(&(a, b))?;
            let code = |lines: &Lines<'_>, start: usize| {
                lines.stripped[start..start + count]
                    .iter()
                    .map(|line| line.1.as_str())
                    .filter(|line| line.chars().any(|ch| ch.is_alphanumeric() || ch == '_'))
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            };
            let effective = code(first, a)
                .iter()
                .zip(code(second, b))
                .filter(|(a, b)| **a == *b)
                .count();
            (effective > 4).then_some((a, b, count))
        })
        .collect()
}
pub(super) fn groups(lines: &[Lines<'_>]) -> Vec<Similarity> {
    let hashes: Vec<_> = lines.iter().map(buckets).collect();
    let mut result: Vec<Similarity> = Vec::new();
    for first in 0..lines.len() {
        for second in first + 1..lines.len() {
            for (a, b, count) in pair(
                &lines[first],
                &lines[second],
                &hashes[first],
                &hashes[second],
            ) {
                let left = Chunk {
                    file: first,
                    start: lines[first].stripped[a].0,
                    end: lines[first].end(a, count),
                };
                let right = Chunk {
                    file: second,
                    start: lines[second].stripped[b].0,
                    end: lines[second].end(b, count),
                };
                if !result.iter().any(|group| {
                    group.count == count
                        && (group.chunks.contains(&left) || group.chunks.contains(&right))
                }) {
                    result.push(Similarity {
                        count,
                        chunks: vec![left, right],
                    });
                }
            }
        }
    }
    result.sort_by_key(|group| std::cmp::Reverse(group.count));
    result
}
