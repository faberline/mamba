//! Generator suggestions preserve raw call names and asynchronous exclusions.
use super::{display, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, ExprCall, ExprListComp, Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeMap;

#[derive(Default)]
struct Awaits(Vec<ruff_text_size::TextRange>);
impl<'a> Visitor<'a> for Awaits {
    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Await(value) = expression {
            self.0.push(value.range());
        }
        walk_expr(self, expression);
    }
}

fn asynchronous(comprehension: &ExprListComp) -> bool {
    if comprehension.generators.iter().any(|value| value.is_async) {
        return true;
    }
    let Some(first) = comprehension.generators.first() else {
        return false;
    };
    let outer = first.iter.range();
    let mut awaits = Awaits::default();
    awaits.visit_expr(&comprehension.elt);
    for generator in &comprehension.generators {
        awaits.visit_expr(&generator.iter);
        for condition in &generator.ifs {
            awaits.visit_expr(condition);
        }
    }
    awaits
        .0
        .iter()
        .any(|range| range.start() < outer.start() || range.end() > outer.end())
}

#[derive(Default)]
struct Loops {
    asynchronous: bool,
    candidates: BTreeMap<u32, TextRange>,
}
impl<'a> Visitor<'a> for Loops {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(function) => {
                let previous = self.asynchronous;
                self.asynchronous = function.is_async;
                self.visit_body(&function.body);
                self.asynchronous = previous;
                return;
            }
            Stmt::ClassDef(class) => {
                let previous = self.asynchronous;
                self.asynchronous = false;
                self.visit_body(&class.body);
                self.asynchronous = previous;
                return;
            }
            Stmt::For(value) if !value.is_async && !self.asynchronous => {
                if let (Expr::Name(target), [Stmt::Expr(expression)]) =
                    (value.target.as_ref(), value.body.as_slice())
                {
                    if let Expr::Yield(yielded) = expression.value.as_ref() {
                        if matches!(yielded.value.as_deref(), Some(Expr::Name(name)) if name.id.as_str() == target.id.as_str())
                        {
                            self.candidates
                                .insert(yielded.start().to_u32(), value.range());
                        }
                    }
                }
            }
            _ => {}
        }
        walk_stmt(self, statement);
    }
}
pub(super) fn yield_from_ranges(body: &[Stmt]) -> BTreeMap<u32, TextRange> {
    let mut loops = Loops::default();
    loops.visit_body(body);
    loops.candidates
}

impl Checker<'_> {
    pub(super) fn check_generator_recommendation(&mut self, call: &ExprCall) {
        if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "set")
            && matches!(call.arguments.args.first(), Some(Expr::ListComp(_)))
        {
            self.emit(
                "R1718",
                "consider-using-set-comprehension",
                "Consider using a set comprehension",
                call.range(),
            );
        }
        if call.arguments.args.len() != 1 {
            return;
        }
        let (Expr::Name(name), Expr::ListComp(comprehension)) =
            (call.func.as_ref(), &call.arguments.args[0])
        else {
            return;
        };
        if !matches!(
            name.id.as_str(),
            "any" | "all" | "sum" | "max" | "min" | "list" | "tuple"
        ) || asynchronous(comprehension)
        {
            return;
        }
        let rendered = display::expression(&call.arguments.args[0], &self.source.text);
        let mut inside = rendered[1..rendered.len() - 1].to_owned();
        if !call.arguments.keywords.is_empty() {
            let keywords = call
                .arguments
                .keywords
                .iter()
                .map(|keyword| {
                    let value = display::expression(&keyword.value, &self.source.text);
                    keyword
                        .arg
                        .as_ref()
                        .map_or_else(|| format!("**{value}"), |name| format!("{name}={value}"))
                })
                .collect::<Vec<_>>()
                .join(", ");
            inside = format!("({inside}), {keywords}");
        }
        let (rule, symbol, prefix) = if matches!(name.id.as_str(), "any" | "all") {
            ("R1729", "use-a-generator", "Use")
        } else {
            ("R1728", "consider-using-generator", "Consider using")
        };
        self.emit(
            rule,
            symbol,
            &format!("{prefix} a generator instead '{}({inside})'", name.id),
            call.range(),
        );
    }
    pub(super) fn check_yield_from(
        &mut self,
        value: &mamba_language::application::quality_syntax::python_ast::ExprYield,
    ) {
        if let Some(range) = self.yield_from_ranges.get(&value.start().to_u32()).copied() {
            self.emit(
                "R1737",
                "use-yield-from",
                "Use 'yield from' directly instead of yielding each element one by one",
                range,
            );
        }
    }
}
