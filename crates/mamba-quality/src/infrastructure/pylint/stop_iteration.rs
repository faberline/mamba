//! StopIteration recommendations retain raw call forms and exception ancestry.
use super::{
    callables::Target, exception_values::Inferred, operand_inference::Operand, stop_sites::Site,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, Visitor},
    Expr, ExprCall, StmtRaise,
};
use ruff_text_size::{Ranged, TextRange};

#[derive(Default)]
struct Names(bool);
impl<'a> Visitor<'a> for Names {
    fn visit_expr(&mut self, value: &'a Expr) {
        if matches!(value, Expr::Name(name) if name.id.as_str() == "StopIteration") {
            self.0 = true;
        }
        walk_expr(self, value);
    }
}
impl Checker<'_> {
    fn suppresses_stop_argument(&self, value: &Expr) -> bool {
        if let Expr::Tuple(tuple) = value {
            return tuple
                .elts
                .iter()
                .any(|value| self.suppresses_stop_argument(value));
        }
        self.expression_target(value).is_some_and(
            |value| matches!(value.target, Target::Class(class) if class.name == "StopIteration"),
        )
    }
    fn ignores_stop(&self, site: &Site) -> bool {
        let mut names = Names::default();
        for value in &site.catches {
            names.visit_expr(value);
        }
        names.0 || site.suppressors.iter().any(|value| {
            let Expr::Call(call) = value else {
                return false;
            };
            self.expression_target(&call.func).is_some_and(|value| {
                matches!(value.target, Target::Class(class) if class.identity.0 == "contextlib" && class.name == "suppress")
            }) && call.arguments.args.iter().any(|value| self.suppresses_stop_argument(value))
        })
    }
    fn report_stop(&mut self, range: TextRange) {
        self.emit(
            "R1708",
            "stop-iteration-return",
            "Do not raise StopIteration in generator, use return statement instead",
            range,
        );
    }
    pub(super) fn check_stop_raise(&mut self, raised: &StmtRaise) {
        let Some(site) = self.stop_sites.0.get(&raised.start().to_u32()) else {
            return;
        };
        if self.ignores_stop(site) {
            return;
        }
        let Some(value) = &raised.exc else {
            return;
        };
        if let Inferred::Class(class, origin, _) = self.exception_value(value) {
            if self
                .lineage(class, origin)
                .iter()
                .any(|(class, _)| class.identity.0 == "builtins" && class.name == "StopIteration")
            {
                self.report_stop(raised.range());
            }
        }
    }
    pub(super) fn check_stop_call(&mut self, call: &ExprCall) {
        let Some(site) = self.stop_sites.0.get(&call.start().to_u32()) else {
            return;
        };
        if matches!(call.func.as_ref(), Expr::Attribute(_))
            || call.arguments.args.len() != 1
            || self.ignores_stop(site)
            || self.stdlib_name(&call.func).as_deref() != Some("builtins.next")
        {
            return;
        }
        let infinite = matches!(self.operand(&call.arguments.args[0]), Operand::Instance(class, _) if class.identity.0 == "itertools" && matches!(class.name.as_str(), "count" | "cycle"));
        if !infinite {
            self.report_stop(call.range());
        }
    }
}
