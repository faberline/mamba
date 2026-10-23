//! C3 validates base order independently of Pylint's attribute inference order.
use super::{callables::Target, classes::Class, resolution::Context, Checker};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
};

type Identity = (String, u32);
type Node = (Rc<Class>, Context);
type Order = Result<Vec<Node>, Error>;

#[derive(Clone, Copy)]
pub(super) enum Error {
    Duplicate,
    Inconsistent,
    Cycle,
}

impl Checker<'_> {
    pub(super) fn mro(&self, class: Rc<Class>, context: Context) -> Order {
        self.linearize((class, context), &mut BTreeSet::new(), &mut BTreeMap::new())
    }

    fn linearize(
        &self,
        node: Node,
        active: &mut BTreeSet<Identity>,
        cache: &mut BTreeMap<Identity, Order>,
    ) -> Order {
        let key = node.0.identity.clone();
        if let Some(order) = cache.get(&key) {
            return order.clone();
        }
        if !active.insert(key.clone()) {
            return Err(Error::Cycle);
        }
        let order = self.merge_bases(node, active, cache);
        active.remove(&key);
        cache.insert(key, order.clone());
        order
    }

    fn merge_bases(
        &self,
        node: Node,
        active: &mut BTreeSet<Identity>,
        cache: &mut BTreeMap<Identity, Order>,
    ) -> Order {
        let mut direct = Vec::new();
        for expression in &node.0.bases {
            if let Some(resolved) = self.declared_base(expression, node.1.clone()) {
                if let Target::Class(class) = resolved.target {
                    direct.push((class, resolved.context));
                }
            }
        }
        if node.0.bases.is_empty() && !(node.0.identity.0 == "builtins" && node.0.name == "object")
        {
            if let Some(object) = self.modules.builtin_class("object") {
                direct.push((object, Context::Module("builtins".into())));
            }
        }
        let mut seen = BTreeSet::new();
        if direct
            .iter()
            .any(|(class, _)| !seen.insert(class.identity.clone()))
        {
            return Err(Error::Duplicate);
        }
        let mut sequences = Vec::new();
        for base in &direct {
            sequences.push(VecDeque::from(self.linearize(
                base.clone(),
                active,
                cache,
            )?));
        }
        sequences.push(direct.into());
        let mut order = vec![node];
        loop {
            sequences.retain(|sequence| !sequence.is_empty());
            if sequences.is_empty() {
                return Ok(order);
            }
            let candidate = sequences
                .iter()
                .filter_map(|sequence| sequence.front())
                .find(|(class, _)| {
                    !sequences.iter().any(|sequence| {
                        sequence
                            .iter()
                            .skip(1)
                            .any(|(other, _)| other.identity == class.identity)
                    })
                })
                .cloned()
                .ok_or(Error::Inconsistent)?;
            for sequence in &mut sequences {
                if sequence
                    .front()
                    .is_some_and(|(class, _)| class.identity == candidate.0.identity)
                {
                    sequence.pop_front();
                }
            }
            order.push(candidate);
        }
    }
}
