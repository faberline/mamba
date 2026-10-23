//! Mapping patterns infer values and keep invariant mapping keys unchanged.
use super::*;
use quality_syntax::python_ast::PatternMatchMapping;

impl Checker<'_> {
    pub(super) fn match_mapping(
        &mut self,
        mapping: &PatternMatchMapping,
        input: Type,
        diagnostics: bool,
    ) -> PatternResult {
        if let Some(Node::Union(items)) = self.context.node(input).as_deref() {
            let mut matches = Vec::new();
            let mut rests = Vec::new();
            let mut captures = BTreeMap::new();
            for &item in items {
                let result = self.match_mapping(mapping, item, diagnostics);
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
        let typed = self.context.typed_dict(input);
        let node = self.context.node(input);
        let arguments = match node.as_deref() {
            Some(Node::Container(Container::Dict | Container::Mapping, arguments)) => {
                Some(arguments.as_slice())
            }
            _ => None,
        };
        let mut result = PatternResult::new(input, input);
        for (key, pattern) in mapping.keys.iter().zip(&mapping.patterns) {
            let item = if input.is_any() {
                Some(Type::Any)
            } else if let Some((_, typed)) = &typed {
                if let Expr::StringLiteral(key) = key {
                    typed
                        .items
                        .iter()
                        .find(|item| item.name == key.value.to_str())
                        .map(|item| item.ty)
                        .or(Some(Type::Object))
                } else {
                    Some(Type::Object)
                }
            } else {
                arguments.and_then(|arguments| arguments.get(1).copied())
            };
            if item.is_none() {
                result.matched = Type::Never;
            }
            let matched = self.match_pattern(pattern, item.unwrap_or(Type::Object), diagnostics);
            if matched.matched == Type::Never {
                result.matched = Type::Never;
            } else {
                self.merge_pattern_captures(&mut result.captures, matched.captures);
            }
        }
        if let Some(rest) = &mapping.rest {
            let items =
                arguments.map_or_else(|| vec![Type::Object, Type::Object], <[Type]>::to_vec);
            result.capture(
                rest,
                self.context.intern(Node::Container(Container::Dict, items)),
            );
        }
        if mapping.keys.is_empty() {
            if typed.is_some() || arguments.is_some() {
                result.rest = Type::Never;
            } else if input == Type::Object {
                result.matched = self.context.intern(Node::Container(
                    Container::Mapping,
                    vec![Type::Any, Type::Any],
                ));
            } else if !input.is_any() {
                result.matched = Type::Never;
            }
        }
        result
    }
}
