//! Collect semantic export events without evaluating Python values.
use super::{Event, Plan};
use crate::infrastructure::{mypy::classes::statements, project};
use mamba_language::application::quality_syntax::{
    python_ast::{
        visitor::{walk_except_handler, walk_expr, walk_pattern, walk_stmt, Visitor},
        ExceptHandler, Expr, FStringPartRef, InterpolatedStringElement, Pattern, Stmt,
    },
    PythonTarget,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

pub(super) fn plan(body: &[Stmt], target: PythonTarget) -> Plan {
    let mut collector = Collector {
        target,
        plan: Plan {
            globals: BTreeSet::new(),
            events: Vec::new(),
            gaps: Vec::new(),
        },
    };
    // semanal_pass1 truncates the module's file.defs after a statically false
    // assert. Nested suites do not use this top-level truncation rule.
    for statement in body {
        collector.visit_stmt(statement);
        if matches!(statement, Stmt::Assert(assertion)
            if statements::truth(&assertion.test, target) == Some(false))
        {
            break;
        }
    }
    collector.plan
}

struct Collector {
    target: PythonTarget,
    plan: Plan,
}

fn all_name(expression: &Expr) -> bool {
    matches!(expression, Expr::Name(name) if name.id.as_str() == "__all__")
}

fn str_expr(expression: &Expr) -> Option<String> {
    if let Some(value) = expression.as_string_literal_expr() {
        return Some(value.value.to_str().to_string());
    }
    let Expr::FString(string) = expression else {
        return None;
    };
    let mut literal = String::new();
    for part in string.value.iter() {
        match part {
            FStringPartRef::Literal(value) => literal.push_str(&value.value),
            FStringPartRef::FString(part) => {
                for element in &part.elements {
                    match element {
                        InterpolatedStringElement::Literal(value) => literal.push_str(&value.value),
                        InterpolatedStringElement::Interpolation(_) => return None,
                    }
                }
            }
        }
    }
    // fastparse folds a single nonempty JoinedStr constant to StrExpr. An
    // empty JoinedStr or any FormattedValue becomes a call, not a string node.
    (!literal.is_empty()).then_some(literal)
}

fn strings(expression: &Expr) -> Vec<String> {
    let items = match expression {
        Expr::List(list) => list.elts.as_slice(),
        Expr::Tuple(tuple) => tuple.elts.as_slice(),
        _ => return Vec::new(),
    };
    items.iter().filter_map(str_expr).collect()
}

fn always_matches(pattern: &Pattern) -> bool {
    match pattern {
        Pattern::MatchAs(capture) => capture.pattern.is_none(),
        Pattern::MatchOr(alternatives) => alternatives.patterns.iter().any(always_matches),
        _ => false,
    }
}

impl Collector {
    fn gap(&mut self, value: &impl Ranged, reason: &str) {
        self.plan.gaps.push((value.range(), reason.into()));
    }

    fn nested_global(&mut self, body: &[Stmt], value: &impl Ranged) {
        if declares_global_all(body) {
            self.gap(
                value,
                "nested global __all__ has no module semantic binding receipt",
            );
        }
    }
}

impl<'a> Visitor<'a> for Collector {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(function) => {
                self.plan.globals.insert(function.name.to_string());
                for decorator in &function.decorator_list {
                    self.visit_expr(&decorator.expression);
                }
                for parameter in function.parameters.iter_non_variadic_params() {
                    if let Some(default) = parameter.default.as_deref() {
                        if function.decorator_list.is_empty() {
                            self.visit_expr(default);
                        } else if mentions_all(default) {
                            self.gap(
                                default,
                                "decorated default __all__ lacks semantic phase metadata",
                            );
                        }
                    }
                }
                self.nested_global(&function.body, function);
            }
            Stmt::ClassDef(class) => {
                self.plan.globals.insert(class.name.to_string());
                for decorator in &class.decorator_list {
                    self.visit_expr(&decorator.expression);
                }
                if let Some(arguments) = &class.arguments {
                    for argument in &arguments.args {
                        self.visit_expr(argument);
                    }
                    for keyword in &arguments.keywords {
                        self.visit_expr(&keyword.value);
                    }
                }
                let symbols = project::symbols::Symbols::collect(&class.body, "", false);
                if declares_global_all(&class.body)
                    || (!symbols.bindings.contains_key("__all__")
                        && class.body.iter().any(statement_mentions_all))
                {
                    self.gap(
                        class,
                        "class-suite __all__ lacks resolved GDEF/MDEF metadata",
                    );
                }
            }
            Stmt::Assign(assignment) => {
                self.visit_expr(&assignment.value);
                for target in &assignment.targets {
                    self.visit_expr(target);
                }
                if matches!(assignment.targets.as_slice(), [target] if all_name(target)) {
                    self.plan
                        .events
                        .push(Event::Add(strings(&assignment.value)));
                }
            }
            Stmt::AnnAssign(assignment) => {
                if let Some(value) = &assignment.value {
                    self.visit_expr(value);
                }
                self.visit_expr(&assignment.target);
                if all_name(&assignment.target) {
                    if let Some(value) = &assignment.value {
                        self.plan.events.push(Event::Add(strings(value)));
                    }
                }
            }
            Stmt::AugAssign(assignment) => {
                // OperatorAssignmentStmt reads its lvalue; it does not introduce
                // a fresh global merely because Ruff marks this target Store.
                if !matches!(assignment.target.as_ref(), Expr::Name(_)) {
                    self.visit_expr(&assignment.target);
                }
                self.visit_expr(&assignment.value);
                if all_name(&assignment.target) {
                    self.plan
                        .events
                        .push(Event::Add(strings(&assignment.value)));
                }
            }
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                let imports = project::imports(std::slice::from_ref(statement), "", false);
                for import in imports {
                    if import.member.as_deref() == Some("*") {
                        self.plan.events.push(Event::Star(statement.clone()));
                    } else {
                        self.plan.globals.insert(import.local);
                        if import.member.as_deref() == Some("__all__")
                            && import.alias.as_deref() == Some("__all__")
                        {
                            self.plan.events.push(Event::CopyAll(statement.clone()));
                        }
                    }
                }
            }
            Stmt::If(conditional) => {
                self.visit_expr(&conditional.test);
                let mut stopped = statements::truth(&conditional.test, self.target) == Some(true);
                if statements::truth(&conditional.test, self.target) != Some(false) {
                    self.visit_body(&conditional.body);
                }
                for clause in &conditional.elif_else_clauses {
                    let truth = clause
                        .test
                        .as_ref()
                        .and_then(|test| statements::truth(test, self.target));
                    if let Some(test) = &clause.test {
                        // Mypy visits elif expressions even when their suite was
                        // marked unreachable by an earlier true condition.
                        self.visit_expr(test);
                    }
                    if !stopped && truth != Some(false) {
                        self.visit_body(&clause.body);
                    }
                    stopped |= truth == Some(true) || clause.test.is_none();
                }
            }
            Stmt::For(looped) => {
                self.visit_expr(&looped.iter);
                self.visit_expr(&looped.target);
                self.visit_body(&looped.body);
                self.visit_body(&looped.orelse);
            }
            Stmt::Match(matched) => {
                self.visit_expr(&matched.subject);
                let mut stopped = false;
                for case in &matched.cases {
                    self.visit_pattern(&case.pattern);
                    let truth = case
                        .guard
                        .as_ref()
                        .and_then(|guard| statements::truth(guard, self.target));
                    if let Some(guard) = &case.guard {
                        self.visit_expr(guard);
                    }
                    if !stopped && truth != Some(false) {
                        self.visit_body(&case.body);
                    }
                    stopped |= always_matches(&case.pattern)
                        && (case.guard.is_none() || truth == Some(true));
                }
            }
            _ => walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Lambda(_)
            | Expr::Generator(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_) => {
                if mentions_all(expression) {
                    self.gap(
                        expression,
                        "nested expression __all__ lacks resolved lexical metadata",
                    );
                }
                return;
            }
            Expr::Name(name) if name.ctx.is_store() => {
                self.plan.globals.insert(name.id.to_string());
            }
            _ => {}
        }
        walk_expr(self, expression);
        let Expr::Call(call) = expression else { return };
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return;
        };
        if !all_name(&attribute.value) {
            return;
        }
        // Mypy combines positional/starred and keyword values in args. These
        // export rules do not validate argument kinds, arity or runtime values.
        let first = call
            .arguments
            .args
            .first()
            .map(|value| match value {
                Expr::Starred(starred) => starred.value.as_ref(),
                value => value,
            })
            .or_else(|| {
                call.arguments
                    .keywords
                    .first()
                    .map(|keyword| &keyword.value)
            });
        let Some(first) = first else { return };
        match attribute.attr.as_str() {
            "append" => self.plan.events.push(Event::Add(
                str_expr(first).map(|value| vec![value]).unwrap_or_default(),
            )),
            "extend" => self.plan.events.push(Event::Add(strings(first))),
            "remove" => {
                if let Some(value) = str_expr(first) {
                    self.plan.events.push(Event::Remove(value));
                }
            }
            _ => {}
        }
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        let name = match pattern {
            Pattern::MatchAs(value) => value.name.as_ref(),
            Pattern::MatchStar(value) => value.name.as_ref(),
            Pattern::MatchMapping(value) => value.rest.as_ref(),
            _ => None,
        };
        if let Some(name) = name {
            self.plan.globals.insert(name.to_string());
        }
        walk_pattern(self, pattern);
    }

    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(value) = handler;
        if let Some(name) = &value.name {
            self.plan.globals.insert(name.to_string());
        }
        walk_except_handler(self, handler);
    }
}

#[derive(Default)]
struct MentionsAll(bool);

impl<'a> Visitor<'a> for MentionsAll {
    fn visit_expr(&mut self, expression: &'a Expr) {
        if all_name(expression) {
            self.0 = true;
        }
        walk_expr(self, expression);
    }

    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Global(global) if global.names.iter().any(|name| name.as_str() == "__all__") => {
                self.0 = true;
            }
            Stmt::FunctionDef(function) => {
                self.0 |=
                    function.name.as_str() == "__all__" || declares_global_all(&function.body);
                for decorator in &function.decorator_list {
                    self.visit_expr(&decorator.expression);
                }
                for parameter in function.parameters.iter_non_variadic_params() {
                    if let Some(default) = parameter.default.as_deref() {
                        self.visit_expr(default);
                    }
                }
                return;
            }
            Stmt::ClassDef(class) if class.name.as_str() == "__all__" => self.0 = true,
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                self.0 |= project::imports(std::slice::from_ref(statement), "", false)
                    .iter()
                    .any(|import| import.local == "__all__");
            }
            _ => {}
        }
        walk_stmt(self, statement);
    }
}

fn mentions_all(expression: &Expr) -> bool {
    let mut visitor = MentionsAll::default();
    visitor.visit_expr(expression);
    visitor.0
}

fn statement_mentions_all(statement: &Stmt) -> bool {
    let mut visitor = MentionsAll::default();
    visitor.visit_stmt(statement);
    visitor.0
}

#[derive(Default)]
struct GlobalAll(bool);

impl<'a> Visitor<'a> for GlobalAll {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if matches!(statement, Stmt::Global(global)
            if global.names.iter().any(|name| name.as_str() == "__all__"))
        {
            self.0 = true;
        }
        walk_stmt(self, statement);
    }
}

fn declares_global_all(body: &[Stmt]) -> bool {
    let mut visitor = GlobalAll::default();
    visitor.visit_body(body);
    visitor.0
}
