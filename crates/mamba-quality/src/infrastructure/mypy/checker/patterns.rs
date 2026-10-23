//! Match branches share capture declarations and retain separate subject refinements.
use super::*;
use quality_syntax::python_ast::{
    ExprContext, ExprName, Identifier, Pattern, Singleton, StmtMatch,
};
use types::{Container, Literal, Node, Type};
mod bindings;
mod classes;
mod mappings;
mod sequences;

#[derive(Clone)]
struct Capture {
    name: Identifier,
    ty: Type,
}

struct PatternResult {
    matched: Type,
    rest: Type,
    captures: BTreeMap<String, Capture>,
}

impl PatternResult {
    fn new(matched: Type, rest: Type) -> Self {
        Self {
            matched,
            rest,
            captures: BTreeMap::new(),
        }
    }
    fn capture(&mut self, name: &Identifier, ty: Type) {
        self.captures.insert(
            name.to_string(),
            Capture {
                name: name.clone(),
                ty,
            },
        );
    }
}

fn capture_expression(name: &Identifier) -> Expr {
    Expr::Name(ExprName {
        node_index: Default::default(),
        range: name.range(),
        id: name.id.clone(),
        ctx: ExprContext::Store,
    })
}

impl Checker<'_> {
    pub(super) fn check_match<'a>(&mut self, matched: &'a StmtMatch)
    where
        Self: Visitor<'a>,
    {
        self.visit_expr(&matched.subject);
        let subject = if let Expr::Named(named) = matched.subject.as_ref() {
            named.target.as_ref()
        } else {
            matched.subject.as_ref()
        };
        let key = scopes::Scopes::attribute_key(subject);
        let input = self.infer(subject).unwrap_or(Type::Any);
        let incoming = self.flow.clone();
        let initial = self.environment();
        let mut declarations: BTreeMap<String, Capture> = BTreeMap::new();
        for case in &matched.cases {
            self.visit_pattern(&case.pattern);
            let result = self.match_pattern(&case.pattern, input, true);
            if result.matched != Type::Never {
                self.merge_pattern_captures(&mut declarations, result.captures);
            }
        }
        for case in &matched.cases {
            self.declare_pattern_names(&case.pattern);
        }
        for capture in declarations.values_mut() {
            if self
                .annotations
                .iter()
                .rev()
                .find_map(|scope| scope.get(capture.name.as_str()))
                .is_some_and(|location| self.final_initializers.contains(location))
            {
                self.emit(
                    "misc",
                    format!("Cannot assign to final name \"{}\"", capture.name),
                    &capture.name,
                );
            }
            capture.ty = initial
                .get(capture.name.as_str())
                .copied()
                .unwrap_or(capture.ty);
            self.scopes
                .declare(&capture_expression(&capture.name), Some(capture.ty));
        }
        let baseline = self.environment();
        let mut remaining = incoming;
        let mut outcomes = Vec::new();
        for case in &matched.cases {
            if !remaining.reachable {
                break;
            }
            self.flow = remaining.clone();
            let current = key
                .as_ref()
                .and_then(|key| self.environment().get(key).copied())
                .or_else(|| self.infer(subject))
                .unwrap_or(input);
            let result = self.match_pattern(&case.pattern, current, false);
            let mut unmatched = remaining.clone();
            unmatched.reachable &= result.rest != Type::Never;
            self.refine_pattern_subject(&mut unmatched, subject, result.rest);
            self.flow.reachable &= result.matched != Type::Never;
            let mut branch = self.flow.clone();
            self.refine_pattern_subject(&mut branch, subject, result.matched);
            self.flow = branch;
            if self.flow.reachable {
                for (name, capture) in result.captures {
                    if let Some(declaration) = declarations.get(&name) {
                        if !self.context.accepts(declaration.ty, capture.ty) {
                            self.emit("misc", format!("Incompatible types in capture pattern (pattern captures type \"{}\", variable has type \"{}\")", self.context.revealed_name(capture.ty), self.context.revealed_name(declaration.ty)), &capture.name);
                            continue;
                        }
                    }
                    self.flow.narrowed.insert(name, capture.ty);
                }
            }
            if let Some(guard) = case.guard.as_deref().filter(|_| self.flow.reachable) {
                self.visit_expr(guard);
                let guarded = self.flow.clone();
                self.flow_condition(guard, true);
                self.copy_pattern_guard(&case.pattern, key.as_deref());
                self.visit_block(&case.body);
                outcomes.push(self.flow.clone());
                self.flow = guarded;
                self.flow_condition(guard, false);
                self.copy_pattern_guard(&case.pattern, key.as_deref());
                let guard_failed = self.flow.clone();
                self.merge_flow(&[unmatched, guard_failed], &baseline);
                remaining = self.flow.clone();
            } else {
                self.visit_block(&case.body);
                outcomes.push(self.flow.clone());
                remaining = unmatched;
            }
        }
        outcomes.push(remaining);
        self.merge_flow(&outcomes, &baseline);
        let mut merged = self.flow.clone();
        self.restore_pattern_subject(&mut merged, subject, input);
        self.flow = merged;
    }

    fn copy_pattern_guard(&mut self, pattern: &Pattern, subject: Option<&str>) {
        if let (Pattern::MatchAs(capture), Some(subject)) = (pattern, subject) {
            if capture.pattern.is_none() {
                if let Some(ty) = capture
                    .name
                    .as_ref()
                    .and_then(|name| self.flow.narrowed.get(name.as_str()))
                    .copied()
                {
                    self.flow.narrowed.insert(subject.into(), ty);
                    self.flow.reachable &= ty != Type::Never;
                }
            }
        }
    }

    fn merge_pattern_captures(
        &self,
        destination: &mut BTreeMap<String, Capture>,
        captures: BTreeMap<String, Capture>,
    ) {
        for (name, capture) in captures {
            destination
                .entry(name)
                .and_modify(|previous| {
                    previous.ty = self.context.union(vec![previous.ty, capture.ty])
                })
                .or_insert(capture);
        }
    }

    fn match_pattern(
        &mut self,
        pattern: &Pattern,
        input: Type,
        diagnostics: bool,
    ) -> PatternResult {
        if input == Type::Never {
            return PatternResult::new(Type::Never, Type::Never);
        }
        match pattern {
            Pattern::MatchAs(capture) => {
                let mut result = capture.pattern.as_deref().map_or_else(
                    || PatternResult::new(input, Type::Never),
                    |pattern| self.match_pattern(pattern, input, diagnostics),
                );
                if result.matched != Type::Never {
                    if let Some(name) = &capture.name {
                        result.capture(name, result.matched);
                    }
                }
                result
            }
            Pattern::MatchStar(star) => {
                let mut result = PatternResult::new(input, Type::Never);
                if let Some(name) = &star.name {
                    result.capture(
                        name,
                        self.context
                            .intern(Node::Container(Container::List, vec![input])),
                    );
                }
                result
            }
            Pattern::MatchOr(alternatives) => {
                let mut rest = input;
                let mut results = Vec::new();
                let mut captures = BTreeMap::new();
                for pattern in &alternatives.patterns {
                    let result = self.match_pattern(pattern, rest, diagnostics);
                    results.push(result.matched);
                    rest = result.rest;
                    if result.matched != Type::Never {
                        self.merge_pattern_captures(&mut captures, result.captures);
                    }
                }
                PatternResult {
                    matched: self.context.union(results),
                    rest,
                    captures,
                }
            }
            Pattern::MatchSingleton(singleton) => {
                let target = match singleton.value {
                    Singleton::None => Type::None,
                    Singleton::True => self.context.intern(Node::Literal(Literal::Bool(true))),
                    Singleton::False => self.context.intern(Node::Literal(Literal::Bool(false))),
                };
                PatternResult::new(
                    self.context.literal_condition(input, target, true, true),
                    self.context.literal_condition(input, target, false, true),
                )
            }
            Pattern::MatchValue(value) => {
                let target = self
                    .context
                    .literal_expression(&value.value)
                    .or_else(|| {
                        if let Expr::Name(name) = value.value.as_ref() {
                            return self
                                .final_constants
                                .get(name.id.as_str())
                                .and_then(|value| self.context.literal_expression(value));
                        }
                        if let Expr::Attribute(attribute) = value.value.as_ref() {
                            return self.infer(&attribute.value).and_then(|ty| {
                                self.context
                                    .class_pattern_value(ty, attribute.attr.as_str())
                            });
                        }
                        None
                    })
                    .or_else(|| self.infer(&value.value))
                    .unwrap_or(Type::Any);
                PatternResult::new(
                    self.pattern_literal(input, target, true),
                    self.pattern_literal(input, target, false),
                )
            }
            Pattern::MatchSequence(sequence) => self.match_sequence(sequence, input, diagnostics),
            Pattern::MatchMapping(mapping) => self.match_mapping(mapping, input, diagnostics),
            Pattern::MatchClass(class) => self.match_class(class, input, diagnostics),
        }
    }

    fn pattern_literal(&self, input: Type, target: Type, positive: bool) -> Type {
        if let Some(Node::Union(items)) = self.context.node(input).as_deref() {
            return self.context.union(
                items
                    .iter()
                    .map(|&ty| self.pattern_literal(ty, target, positive))
                    .collect(),
            );
        }
        if matches!(target, Type::Float | Type::Complex) {
            return if !positive
                || self.context.accepts(target, input)
                || matches!(input, Type::Any | Type::AnySpecial | Type::Object)
            {
                input
            } else {
                Type::Never
            };
        }
        let retain_bool = !positive
            && matches!(
                (
                    self.context.node(input).as_deref(),
                    self.context.node(target).as_deref()
                ),
                (
                    Some(Node::Literal(Literal::Bool(_))),
                    Some(Node::Literal(Literal::Int(_)))
                )
            );
        self.context
            .literal_condition(input, target, positive, retain_bool)
    }
}
