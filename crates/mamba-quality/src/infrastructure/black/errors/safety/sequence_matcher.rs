//! difflib's no-junk longest-match rules, including automatic popular-line removal.
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Tag {
    Equal,
    Replace,
    Delete,
    Insert,
}
#[derive(Clone, Copy)]
pub(super) struct Code {
    pub tag: Tag,
    pub a_start: usize,
    pub a_end: usize,
    pub b_start: usize,
    pub b_end: usize,
}
struct Matcher<'a> {
    a: &'a [&'a str],
    b: &'a [&'a str],
    positions: HashMap<&'a str, Vec<usize>>,
}
impl<'a> Matcher<'a> {
    fn new(a: &'a [&'a str], b: &'a [&'a str]) -> Self {
        let mut positions: HashMap<&str, Vec<usize>> = HashMap::new();
        for (index, &line) in b.iter().enumerate() {
            positions.entry(line).or_default().push(index);
        }
        if b.len() >= 200 {
            let limit = b.len() / 100 + 1;
            positions.retain(|_, values| values.len() <= limit);
        }
        Self { a, b, positions }
    }
    fn longest(
        &self,
        a_start: usize,
        a_end: usize,
        b_start: usize,
        b_end: usize,
    ) -> (usize, usize, usize) {
        let (mut best_a, mut best_b, mut best_size) = (a_start, b_start, 0);
        let mut previous: HashMap<usize, usize> = HashMap::new();
        for i in a_start..a_end {
            let mut current = HashMap::new();
            for &j in self.positions.get(self.a[i]).into_iter().flatten() {
                if j < b_start {
                    continue;
                }
                if j >= b_end {
                    break;
                }
                let size = if j == 0 {
                    1
                } else {
                    previous.get(&(j - 1)).copied().unwrap_or(0) + 1
                };
                current.insert(j, size);
                if size > best_size {
                    (best_a, best_b, best_size) = (i + 1 - size, j + 1 - size, size);
                }
            }
            previous = current;
        }
        // No explicit junk predicate exists. Popular lines can extend a selected match.
        while best_a > a_start && best_b > b_start && self.a[best_a - 1] == self.b[best_b - 1] {
            best_a -= 1;
            best_b -= 1;
            best_size += 1;
        }
        while best_a + best_size < a_end
            && best_b + best_size < b_end
            && self.a[best_a + best_size] == self.b[best_b + best_size]
        {
            best_size += 1;
        }
        (best_a, best_b, best_size)
    }
    fn blocks(&self) -> Vec<(usize, usize, usize)> {
        let mut queue = vec![(0, self.a.len(), 0, self.b.len())];
        let mut blocks = Vec::new();
        while let Some((a_start, a_end, b_start, b_end)) = queue.pop() {
            let (i, j, size) = self.longest(a_start, a_end, b_start, b_end);
            if size == 0 {
                continue;
            }
            blocks.push((i, j, size));
            if a_start < i && b_start < j {
                queue.push((a_start, i, b_start, j));
            }
            if i + size < a_end && j + size < b_end {
                queue.push((i + size, a_end, j + size, b_end));
            }
        }
        blocks.sort_unstable();
        let mut merged: Vec<(usize, usize, usize)> = Vec::new();
        for (i, j, size) in blocks {
            if let Some(previous) = merged.last_mut() {
                if previous.0 + previous.2 == i && previous.1 + previous.2 == j {
                    previous.2 += size;
                    continue;
                }
            }
            merged.push((i, j, size));
        }
        merged.push((self.a.len(), self.b.len(), 0));
        merged
    }
}
pub(super) fn codes(a: &[&str], b: &[&str]) -> Vec<Code> {
    let (mut i, mut j) = (0, 0);
    let mut codes = Vec::new();
    for (a_start, b_start, size) in Matcher::new(a, b).blocks() {
        let tag = match (i < a_start, j < b_start) {
            (true, true) => Some(Tag::Replace),
            (true, false) => Some(Tag::Delete),
            (false, true) => Some(Tag::Insert),
            (false, false) => None,
        };
        if let Some(tag) = tag {
            codes.push(Code {
                tag,
                a_start: i,
                a_end: a_start,
                b_start: j,
                b_end: b_start,
            });
        }
        (i, j) = (a_start + size, b_start + size);
        if size != 0 {
            codes.push(Code {
                tag: Tag::Equal,
                a_start,
                a_end: i,
                b_start,
                b_end: j,
            });
        }
    }
    codes
}
