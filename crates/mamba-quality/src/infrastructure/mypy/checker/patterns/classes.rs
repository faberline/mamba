//! Class patterns use runtime class ranges, declared positional names, and member types.
use super::*;
use quality_syntax::python_ast::PatternMatchClass;

impl Checker<'_> {
    pub(super) fn match_class(
        &mut self,
        class: &PatternMatchClass,
        input: Type,
        diagnostics: bool,
    ) -> PatternResult {
        let value = self.infer(&class.cls).unwrap_or(Type::Any);
        let Some(targets) = self.context.runtime_type_ranges(value) else {
            if diagnostics {
                self.emit(
                    "misc",
                    format!(
                        "Expected type in class pattern; found \"{}\"",
                        self.context.revealed_name_for(value, self.python_target)
                    ),
                    class,
                );
            }
            return PatternResult::new(Type::Never, input);
        };
        let module = self.scopes.qualified_name("");
        let matched = self.context.runtime_instance_narrow(
            input,
            &targets,
            true,
            module.trim_end_matches('.'),
        );
        let rest = self.context.runtime_instance_narrow(
            input,
            &targets,
            false,
            module.trim_end_matches('.'),
        );
        if matched == Type::Never {
            return PatternResult::new(Type::Never, input);
        }
        let target = self
            .context
            .union(targets.iter().map(|&(target, _)| target).collect());
        let mut result = PatternResult::new(matched, rest);
        let positional = &class.arguments.patterns;
        let self_match = matches!(
            target,
            Type::Int | Type::Float | Type::Str | Type::Bytes | Type::Bool
        ) || matches!(
            self.context.node(target).as_deref(),
            Some(
                Node::Container(
                    Container::List | Container::Set | Container::FrozenSet | Container::Dict,
                    _
                ) | Node::VariadicTuple(_)
            )
        );
        if self_match && !positional.is_empty() {
            if positional.len() > 1 && diagnostics {
                self.emit(
                    "misc",
                    "Too many positional patterns for class pattern".into(),
                    class,
                );
            }
            let mut inner = self.match_pattern(&positional[0], matched, diagnostics);
            inner.rest =
                self.ordered_pattern_rest(input, self.context.union(vec![rest, inner.rest]));
            return inner;
        }
        let mut pairs: Vec<(Option<String>, &Pattern)> = Vec::new();
        let mut positional_names = std::collections::BTreeSet::new();
        if !positional.is_empty() {
            let names = self
                .context
                .class_pattern_value(target, "__match_args__")
                .or_else(|| {
                    self.context
                        .class_field(target, "__match_args__")
                        .and_then(|field| field.ty)
                });
            let Some(names) = names else {
                if diagnostics {
                    self.emit(
                        "misc",
                        format!(
                            "Class \"{}\" doesn't define \"__match_args__\"",
                            self.context.revealed_name_for(target, self.python_target)
                        ),
                        class,
                    );
                }
                return PatternResult::new(Type::Never, input);
            };
            let names_node = self.context.node(names);
            let names = if let Some(Node::Tuple(items)) = names_node.as_deref() {
                if positional.len() > items.len() {
                    if diagnostics {
                        self.emit(
                            "misc",
                            "Too many positional patterns for class pattern".into(),
                            class,
                        );
                    }
                    return PatternResult::new(Type::Never, input);
                }
                items
                    .iter()
                    .map(|&item| match self.context.node(item).as_deref() {
                        Some(
                            Node::Literal(Literal::Str(name))
                            | Node::LiteralValue(Literal::Str(name)),
                        ) => Some(name.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![None; positional.len()]
            };
            for (name, pattern) in names.into_iter().zip(positional) {
                if let Some(name) = &name {
                    positional_names.insert(name.clone());
                }
                pairs.push((name, pattern));
            }
        }
        let mut keyword_names = std::collections::BTreeSet::new();
        let mut duplicate = false;
        for keyword in &class.arguments.keywords {
            let name = keyword.attr.to_string();
            let message = if positional_names.contains(&name) {
                Some(format!(
                    "Keyword \"{name}\" already matches a positional pattern"
                ))
            } else if !keyword_names.insert(name.clone()) {
                Some(format!("Multiple keyword patterns for \"{name}\""))
            } else {
                None
            };
            if let Some(message) = message {
                if diagnostics {
                    self.emit("misc", message, &keyword.pattern);
                }
                duplicate = true;
            }
            pairs.push((Some(name), &keyword.pattern));
        }
        if duplicate {
            return PatternResult::new(Type::Never, input);
        }
        for (name, pattern) in pairs {
            let member = name
                .as_deref()
                .and_then(|name| self.context.class_field(matched, name))
                .and_then(|field| field.ty);
            if member.is_none() && diagnostics && name.is_some() {
                self.emit(
                    "misc",
                    format!(
                        "Class \"{}\" has no attribute \"{}\"",
                        self.context.revealed_name_for(target, self.python_target),
                        name.as_deref().unwrap()
                    ),
                    pattern,
                );
            }
            let member = self.match_pattern(pattern, member.unwrap_or(Type::Any), diagnostics);
            if member.matched == Type::Never {
                result.matched = Type::Never;
            } else {
                self.merge_pattern_captures(&mut result.captures, member.captures);
                if member.rest != Type::Never {
                    result.rest = input;
                }
            }
        }
        result
    }
}
