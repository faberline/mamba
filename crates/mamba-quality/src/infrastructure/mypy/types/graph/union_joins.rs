//! A proven subset of the original Union visitor; unknowns retain legacy joins.
use super::super::Literal;
use super::*;
use std::collections::BTreeSet;

impl Context {
    pub(super) fn bounded_union_join(&self, mut left: Type, mut right: Type) -> Option<Type> {
        if !self.union_join_is_union(left) && !self.union_join_is_union(right) {
            return None;
        }
        // join.py218-254: whole Any wins in either orientation.
        if left.is_any() || right.is_any() {
            return Some(left.merge_any(right));
        }
        if self.union_join_truth(left)? != self.union_join_truth(right)? {
            // Admitted atoms carry default truth flags. true_or_false also
            // simplifies Union operands; it does not erase last-known values.
            if self.union_join_is_union(left) {
                left = self.union_join_simplify(vec![left])?;
            }
            if self.union_join_is_union(right) {
                right = self.union_join_simplify(vec![right])?;
            }
        }
        if self.union_join_is_union(left) && !self.union_join_is_union(right) {
            std::mem::swap(&mut left, &mut right);
        }
        if left == Type::None && right != Type::None {
            std::mem::swap(&mut left, &mut right);
        }
        if left == Type::Never && right != Type::Never {
            std::mem::swap(&mut left, &mut right);
        }
        if self.union_join_is_union(right) {
            // join.py271-275: retain t itself when s is a proper subtype.
            return if self.union_join_proper(left, right, false, &mut BTreeSet::new())? {
                Some(right)
            } else {
                self.union_join_simplify(vec![left, right])
            };
        }
        match right {
            // join.py280-292, strict Optional mode and bottom visitor.
            Type::None => self.union_join_simplify(vec![left, right]),
            Type::Never => Some(left),
            // Normalization may remove the last Union. Other visitors stay legacy.
            _ => None,
        }
    }

    fn union_join_is_union(&self, ty: Type) -> bool {
        matches!(self.node(ty).as_deref(), Some(Node::Union(_)))
    }

    fn union_join_container(&self, ty: Type) -> Option<(Container, Vec<Type>)> {
        match self.node(ty)?.as_ref() {
            Node::Container(kind @ (Container::List | Container::Sequence), args)
                if args.len() == 1 =>
            {
                Some((*kind, args.clone()))
            }
            Node::Container(Container::Dict, args) if args.len() == 2 => {
                Some((Container::Dict, args.clone()))
            }
            _ => None,
        }
    }

    fn union_join_scalar(ty: Type) -> bool {
        matches!(
            ty,
            Type::Int
                | Type::Float
                | Type::Complex
                | Type::Str
                | Type::Bytes
                | Type::Bool
                | Type::None
                | Type::Any
                | Type::AnySpecial
                | Type::Object
                | Type::Never
        )
    }

    fn union_join_truth(&self, ty: Type) -> Option<(bool, bool)> {
        match ty {
            Type::None => return Some((false, true)),
            Type::Never => return Some((false, false)),
            _ if Self::union_join_scalar(ty) => return Some((true, true)),
            _ => {}
        }
        match self.node(ty)?.as_ref() {
            Node::Literal(value) => {
                let truth = match value {
                    Literal::Int(value) => *value != 0,
                    Literal::Bool(value) => *value,
                    Literal::Str(value) => !value.is_empty(),
                    Literal::Bytes(value) => !value.is_empty(),
                };
                Some((truth, !truth))
            }
            // Instance last_known_value is distinct from LiteralType truth defaults.
            Node::LiteralValue(_) => Some((true, true)),
            Node::Union(items) => {
                let mut truth = (false, false);
                for &item in items {
                    let item = self.union_join_truth(item)?;
                    truth.0 |= item.0;
                    truth.1 |= item.1;
                }
                Some(truth)
            }
            _ => self.union_join_container(ty).map(|_| (true, true)),
        }
    }

    fn union_join_proper(
        &self,
        left: Type,
        right: Type,
        ignore_promotions: bool,
        active: &mut BTreeSet<(Type, Type, bool)>,
    ) -> Option<bool> {
        // subtypes.py197-233,485: universal reflexivity and bottom only.
        if self.any_origin_equal(left, right) {
            if self.union_join_container(left).is_some_and(|(kind, args)| {
                kind != Container::Sequence
                    && args.iter().any(|&item| self.union_join_has_never(item))
            }) {
                return None;
            }
            return Some(true);
        }
        if left == Type::Never {
            return Some(true);
        }
        let key = (left, right, ignore_promotions);
        if !active.insert(key) {
            return None;
        }
        let result = self.union_join_proper_inner(left, right, ignore_promotions, active);
        active.remove(&key);
        result
    }

    fn union_join_proper_inner(
        &self,
        left: Type,
        right: Type,
        ignore_promotions: bool,
        active: &mut BTreeSet<(Type, Type, bool)>,
    ) -> Option<bool> {
        if let Some(Node::Union(items)) = self.node(left).as_deref() {
            let mut unknown = false;
            for &item in items {
                match self.union_join_proper(item, right, ignore_promotions, active) {
                    Some(false) => return Some(false),
                    None => unknown = true,
                    Some(true) => {}
                }
            }
            return (!unknown).then_some(true);
        }
        if let Some(Node::Union(items)) = self.node(right).as_deref() {
            let mut unknown = false;
            for &item in items {
                match self.union_join_proper(left, item, ignore_promotions, active) {
                    Some(true) => return Some(true),
                    None => unknown = true,
                    Some(false) => {}
                }
            }
            // subtypes.py339-357 and typeops.py1123: only explicit bool literals
            // contract here, not last-known Instance values or unknown enums.
            if left == Type::Bool && self.union_join_bool_sum(items) {
                return Some(true);
            }
            return (!unknown).then_some(false);
        }
        let left_node = self.node(left);
        let right_node = self.node(right);
        let admitted = |ty, node: Option<&Node>| {
            Self::union_join_scalar(ty)
                || matches!(node, Some(Node::Literal(_) | Node::LiteralValue(_)))
                || self.union_join_container(ty).is_some()
        };
        if !admitted(left, left_node.as_deref()) || !admitted(right, right_node.as_deref()) {
            return None;
        }
        if left.is_any() || right.is_any() {
            // Proper subtype cannot use assignment compatibility through Any.
            return Some(false);
        }
        if let Some(Node::Literal(expected)) = right_node.as_deref() {
            return Some(match left_node.as_deref() {
                Some(Node::Literal(actual) | Node::LiteralValue(actual)) => actual == expected,
                _ => false,
            });
        }
        if let Some(Node::LiteralValue(value)) = right_node.as_deref() {
            return self.union_join_proper(left, value.base(), ignore_promotions, active);
        }
        if let Some(Node::Literal(value) | Node::LiteralValue(value)) = left_node.as_deref() {
            return self.union_join_proper(value.base(), right, ignore_promotions, active);
        }
        if right == Type::Object {
            return Some(true);
        }
        if left == Type::None || right == Type::None || right == Type::Never {
            return Some(false);
        }
        let left_container = self.union_join_container(left);
        let right_container = self.union_join_container(right);
        if let Some((Container::Sequence, expected)) = &right_container {
            let item = match (&left_container, left) {
                (Some((Container::List | Container::Sequence, args)), _) => Some(args[0]),
                (_, Type::Str) => Some(Type::Str),
                (_, Type::Bytes) => Some(Type::Int),
                _ => None,
            };
            return item.map_or(Some(false), |item| {
                self.union_join_proper(item, expected[0], ignore_promotions, active)
            });
        }
        if let (Some((left_kind, actual)), Some((right_kind, expected))) =
            (&left_container, &right_container)
        {
            if left_kind != right_kind {
                return Some(false);
            }
            let mut unknown = false;
            for (&actual, &expected) in actual.iter().zip(expected) {
                // Native Never has no original ambiguous flag. Invariant
                // empty-argument covariance is therefore not a proven rule.
                if self.union_join_has_never(actual) || self.union_join_has_never(expected) {
                    unknown = true;
                    continue;
                }
                for (left, right) in [(actual, expected), (expected, actual)] {
                    match self.union_join_proper(left, right, ignore_promotions, active) {
                        Some(false) => return Some(false),
                        None => unknown = true,
                        Some(true) => {}
                    }
                }
            }
            return (!unknown).then_some(true);
        }
        if left_container.is_some() || right_container.is_some() {
            return Some(false);
        }
        // Canonical scalar inheritance plus the original optional numeric promotions.
        Some(
            left == Type::Bool && right == Type::Int
                || !ignore_promotions
                    && matches!(
                        (left, right),
                        (Type::Bool | Type::Int, Type::Float | Type::Complex)
                            | (Type::Float, Type::Complex)
                    ),
        )
    }

    fn union_join_has_never(&self, ty: Type) -> bool {
        if ty == Type::Never {
            return true;
        }
        match self.node(ty).as_deref() {
            Some(Node::Union(items) | Node::Container(_, items)) => {
                items.iter().any(|&item| self.union_join_has_never(item))
            }
            _ => false,
        }
    }

    fn union_join_bool_sum(&self, items: &[Type]) -> bool {
        let mut flattened = Vec::new();
        for &item in items {
            self.union_join_flatten(item, &mut flattened);
        }
        [false, true].into_iter().all(|wanted| {
            flattened.iter().any(|&item| {
                matches!(
                    self.node(item).as_deref(),
                    Some(Node::Literal(Literal::Bool(value))) if *value == wanted
                )
            })
        })
    }

    fn union_join_flatten(&self, ty: Type, output: &mut Vec<Type>) {
        if let Some(Node::Union(items)) = self.node(ty).as_deref() {
            for &item in items {
                self.union_join_flatten(item, output);
            }
        } else {
            output.push(ty);
        }
    }

    pub(in crate::infrastructure::mypy::types) fn union_join_simplify(
        &self,
        inputs: Vec<Type>,
    ) -> Option<Type> {
        let mut items = Vec::new();
        for ty in inputs {
            self.union_join_flatten(ty, &mut items);
        }
        // Unknown atoms have no proven truth/extra-attribute representation.
        for &item in &items {
            self.union_join_truth(item)?;
        }
        // typeops.py696-773: two directional passes, stable last-known guard.
        for _ in 0..2 {
            let mut kept = Vec::new();
            for item in items {
                if item == Type::Never || kept.contains(&item) {
                    continue;
                }
                let mut duplicate = false;
                for &prior in &kept {
                    if let Some(Node::LiteralValue(value)) = self.node(prior).as_deref() {
                        if !matches!(
                            self.node(item).as_deref(),
                            Some(Node::LiteralValue(other)) if value == other
                        ) {
                            continue;
                        }
                    }
                    if self.union_join_proper(item, prior, true, &mut BTreeSet::new())? {
                        duplicate = true;
                        break;
                    }
                }
                if !duplicate {
                    kept.push(item);
                }
            }
            items = kept;
            if items.len() <= 1 {
                break;
            }
            items.reverse();
        }
        // typeops.py1123-1163 contracts explicit True/False at first occurrence.
        if self.union_join_bool_sum(&items) {
            let mut contracted = Vec::new();
            let mut inserted = false;
            for item in items {
                if matches!(
                    self.node(item).as_deref(),
                    Some(Node::Literal(Literal::Bool(_)))
                ) {
                    if !inserted {
                        contracted.push(Type::Bool);
                        inserted = true;
                    }
                } else {
                    contracted.push(item);
                }
            }
            items = contracted;
        }
        Some(self.union(items))
    }
}
