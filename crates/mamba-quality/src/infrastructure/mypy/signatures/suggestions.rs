//! Keyword suggestions use the locked original sequence matching thresholds.
use std::collections::{BTreeSet, HashMap};

pub(in crate::infrastructure::mypy) fn best_matches(
    current: &str,
    options: &BTreeSet<&str>,
) -> Vec<String> {
    let current: Vec<_> = current.chars().collect();
    if current.is_empty() {
        return vec![];
    }
    let mut options: Vec<_> = options
        .iter()
        .map(|&name| (name, name.chars().collect::<Vec<_>>()))
        .filter(|(_, value)| 8 * current.len().min(value.len()) > 3 * (current.len() + value.len()))
        .collect();
    if options.len() >= 50 {
        options.retain(|(_, value)| current.len().abs_diff(value.len()) <= 1);
    }
    let mut scored: Vec<_> = options
        .into_iter()
        .filter_map(|(name, value)| {
            let matches = matching_characters(&current, &value);
            let total = current.len() + value.len();
            (8 * matches > 3 * total).then_some((name, 2.0 * matches as f64 / total as f64))
        })
        .collect();
    scored.sort_by(|(left, score), (right, other)| {
        other.total_cmp(score).then_with(|| left.cmp(right))
    });
    scored
        .into_iter()
        .take(3)
        .map(|(name, _)| name.to_string())
        .collect()
}

fn matching_characters(left: &[char], right: &[char]) -> usize {
    let mut positions: HashMap<char, Vec<usize>> = HashMap::new();
    for (index, &value) in right.iter().enumerate() {
        positions.entry(value).or_default().push(index);
    }
    if right.len() >= 200 {
        positions.retain(|_, values| values.len() <= right.len() / 100 + 1);
    }
    let mut pending = vec![(0, left.len(), 0, right.len())];
    let mut matched = 0;
    while let Some((low, high, start, end)) = pending.pop() {
        let (mut best_left, mut best_right, mut size) = (low, start, 0);
        let mut previous: HashMap<usize, usize> = HashMap::new();
        for (i, &value) in left.iter().enumerate().take(high).skip(low) {
            let mut next = HashMap::new();
            for &j in positions.get(&value).into_iter().flatten() {
                if j < start {
                    continue;
                }
                if j >= end {
                    break;
                }
                let length = 1 + j
                    .checked_sub(1)
                    .and_then(|j| previous.get(&j))
                    .copied()
                    .unwrap_or(0);
                next.insert(j, length);
                if length > size {
                    (best_left, best_right, size) = (i + 1 - length, j + 1 - length, length);
                }
            }
            previous = next;
        }
        while best_left > low && best_right > start && left[best_left - 1] == right[best_right - 1]
        {
            best_left -= 1;
            best_right -= 1;
            size += 1;
        }
        while best_left + size < high
            && best_right + size < end
            && left[best_left + size] == right[best_right + size]
        {
            size += 1;
        }
        if size == 0 {
            continue;
        }
        matched += size;
        if low < best_left && start < best_right {
            pending.push((low, best_left, start, best_right));
        }
        if best_left + size < high && best_right + size < end {
            pending.push((best_left + size, high, best_right + size, end));
        }
    }
    matched
}
