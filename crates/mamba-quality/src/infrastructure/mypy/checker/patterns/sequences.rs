//! Sequence patterns retain fixed tuple positions and collect star captures as lists.
use super::*;
use quality_syntax::python_ast::PatternMatchSequence;

impl Checker<'_> {
    pub(super) fn match_sequence(
        &mut self,
        sequence: &PatternMatchSequence,
        input: Type,
        diagnostics: bool,
    ) -> PatternResult {
        if let Some(Node::Union(items)) = self.context.node(input).as_deref() {
            let mut matches = Vec::new();
            let mut rests = Vec::new();
            let mut captures = BTreeMap::new();
            for &item in items {
                let result = self.match_sequence(sequence, item, diagnostics);
                matches.push(result.matched);
                rests.push(result.rest);
                if result.matched != Type::Never {
                    self.merge_pattern_captures(&mut captures, result.captures);
                }
            }
            return PatternResult {
                matched: self.context.union(matches),
                rest: self.context.union(rests),
                captures,
            };
        }
        let star = sequence
            .patterns
            .iter()
            .position(|pattern| matches!(pattern, Pattern::MatchStar(_)));
        let required = sequence.patterns.len() - usize::from(star.is_some());
        let node = self.context.node(input);
        let fixed = if let Some(Node::Tuple(items)) = node.as_deref() {
            Some(items.as_slice())
        } else {
            None
        };
        let items = if let Some(items) = fixed {
            if items.len() < required || star.is_none() && items.len() != required {
                return PatternResult::new(Type::Never, input);
            }
            items.to_vec()
        } else {
            let item = if input.is_any() {
                Some(Type::Any)
            } else if input == Type::Object {
                Some(Type::Object)
            } else if matches!(input, Type::Str | Type::Bytes) {
                None
            } else {
                self.context.sequence_item(input)
            };
            let Some(item) = item else {
                return PatternResult::new(Type::Never, input);
            };
            vec![item; sequence.patterns.len()]
        };
        let star_length = if fixed.is_some() {
            items.len() - required
        } else {
            1
        };
        let mut new_items = Vec::new();
        let mut rest_items = Vec::new();
        let mut captures = BTreeMap::new();
        let mut can_match = true;
        for (index, pattern) in sequence.patterns.iter().enumerate() {
            let (start, count) = if let Some(star) = star {
                if index < star {
                    (index, 1)
                } else if index == star {
                    (star, star_length)
                } else {
                    (index + star_length - 1, 1)
                }
            } else {
                (index, 1)
            };
            let item = self.context.union(items[start..start + count].to_vec());
            let result = if Some(index) == star && count == 0 {
                let mut result = PatternResult::new(Type::Never, Type::Never);
                if let Pattern::MatchStar(star) = pattern {
                    if let Some(name) = &star.name {
                        result.capture(
                            name,
                            self.context
                                .intern(Node::Container(Container::List, vec![Type::Never])),
                        );
                    }
                }
                result
            } else {
                self.match_pattern(pattern, item, diagnostics)
            };
            if result.matched == Type::Never && count != 0 {
                can_match = false;
            }
            new_items.extend(std::iter::repeat_n(result.matched, count));
            rest_items.extend(std::iter::repeat_n(result.rest, count));
            self.merge_pattern_captures(&mut captures, result.captures);
        }
        let matched = if !can_match {
            Type::Never
        } else if fixed.is_some() {
            self.context.intern(Node::Tuple(new_items))
        } else if input == Type::Object {
            self.context
                .intern(Node::Container(Container::Sequence, vec![Type::Object]))
        } else {
            input
        };
        let rest = if fixed.is_some() {
            let conditional = rest_items
                .iter()
                .filter(|&&item| item != Type::Never)
                .count();
            if conditional == 0 {
                Type::Never
            } else if conditional == 1 {
                self.context.intern(Node::Tuple(
                    items
                        .iter()
                        .zip(rest_items)
                        .map(|(&initial, rest)| if rest == Type::Never { initial } else { rest })
                        .collect(),
                ))
            } else {
                input
            }
        } else if star.is_some() && sequence.patterns.len() == 1 {
            Type::Never
        } else {
            input
        };
        PatternResult {
            matched,
            rest,
            captures,
        }
    }
}
