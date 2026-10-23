//! C3 orders are cached by declaration identity and invalidated when bases change.
use super::*;
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::infrastructure::mypy) enum Error {
    Duplicate(u32),
    Cycle,
    Inconsistent,
}

impl Context {
    pub(in crate::infrastructure::mypy) fn class_mro(&self, root: u32) -> Result<Vec<u32>, Error> {
        if let Some(order) = self.classes.borrow().mro.get(&root) {
            return order.clone();
        }
        let mut orders = BTreeMap::<u32, Result<Vec<u32>, Error>>::new();
        let mut active = BTreeSet::new();
        let mut pending = vec![(root, false)];
        while let Some((id, finish)) = pending.pop() {
            if orders.contains_key(&id) || self.classes.borrow().mro.contains_key(&id) {
                continue;
            }
            let class = self.class(id);
            if finish {
                active.remove(&id);
                let mut seen = BTreeSet::new();
                let duplicate = class.bases.iter().copied().find(|base| !seen.insert(*base));
                let order = if let Some(duplicate) = duplicate {
                    Err(Error::Duplicate(duplicate))
                } else {
                    let mut sequences: Vec<VecDeque<u32>> = class
                        .bases
                        .iter()
                        .map(|base| {
                            orders
                                .get(base)
                                .cloned()
                                .or_else(|| self.classes.borrow().mro.get(base).cloned())
                                .and_then(Result::ok)
                                .unwrap_or_else(|| vec![*base])
                                .into()
                        })
                        .collect();
                    sequences.push(class.bases.clone().into());
                    merge(id, sequences)
                };
                orders.insert(id, order);
            } else {
                if !active.insert(id) {
                    return Err(Error::Cycle);
                }
                pending.push((id, true));
                pending.extend(class.bases.iter().rev().map(|&base| (base, false)));
            }
        }
        let result = orders
            .get(&root)
            .cloned()
            .unwrap_or_else(|| self.classes.borrow().mro[&root].clone());
        self.classes.borrow_mut().mro.extend(orders);
        result
    }

    pub(in crate::infrastructure::mypy) fn has_base(&self, root: u32, target: u32) -> bool {
        let mut pending = self.class(root).bases.clone();
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if id == target {
                return true;
            }
            if seen.insert(id) {
                pending.extend(self.class(id).bases.iter().copied());
            }
        }
        false
    }
}

fn merge(id: u32, mut sequences: Vec<VecDeque<u32>>) -> Result<Vec<u32>, Error> {
    let mut order = vec![id];
    loop {
        sequences.retain(|sequence| !sequence.is_empty());
        if sequences.is_empty() {
            return Ok(order);
        }
        let candidate = sequences
            .iter()
            .filter_map(|sequence| sequence.front())
            .find(|candidate| {
                sequences
                    .iter()
                    .all(|sequence| !sequence.iter().skip(1).any(|item| item == *candidate))
            })
            .copied()
            .ok_or(Error::Inconsistent)?;
        order.push(candidate);
        for sequence in &mut sequences {
            if sequence.front() == Some(&candidate) {
                sequence.pop_front();
            }
        }
    }
}
