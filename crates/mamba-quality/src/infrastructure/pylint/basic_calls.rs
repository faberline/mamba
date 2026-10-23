//! Builtin basic checks preserve syntax, shadowing, and protocol boundaries.
use super::{
    callables::Target, comparison_constants, display, resolution::Context, values::Value, Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    Expr, ExprCall, ExprDict, ExprSet, StmtWith,
};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn check_duplicate_dictionary(&mut self, dictionary: &ExprDict) {
        let mut seen = Vec::new();
        for item in &dictionary.items {
            let Some(value) = &item.key else {
                continue;
            };
            if !comparison_constants::constant(value) && !matches!(value, Expr::Attribute(_)) {
                continue;
            }
            let repeated = seen.iter().any(|previous| {
                comparison_constants::same(value, previous)
                    || (matches!((value, previous), (Expr::Attribute(_), Expr::Attribute(_)))
                        && display::expression(value, &self.source.text)
                            == display::expression(previous, &self.source.text))
            });
            if repeated {
                let text = match value {
                    Expr::NoneLiteral(_) => "%r".into(),
                    Expr::EllipsisLiteral(_) => "Ellipsis".into(),
                    Expr::Attribute(_) => super::string_format::python_string_repr(
                        &display::expression(value, &self.source.text),
                    ),
                    _ => display::expression(value, &self.source.text),
                };
                self.emit(
                    "W0109",
                    "duplicate-key",
                    &format!("Duplicate key {text} in dictionary"),
                    dictionary.range(),
                );
            }
            seen.push(value.clone());
        }
    }
    pub(super) fn check_duplicate_set(&mut self, set: &ExprSet) {
        let mut seen = Vec::new();
        for value in &set.elts {
            if !comparison_constants::constant(value) {
                continue;
            }
            if seen
                .iter()
                .any(|previous| comparison_constants::same(value, previous))
            {
                let text = if matches!(value, Expr::NoneLiteral(_)) {
                    "%r".into()
                } else if matches!(value, Expr::EllipsisLiteral(_)) {
                    "Ellipsis".into()
                } else {
                    display::expression(value, &self.source.text)
                };
                self.emit(
                    "W0130",
                    "duplicate-value",
                    &format!("Duplicate value {text} in set"),
                    set.range(),
                );
            }
            seen.push(value.clone());
        }
    }
    pub(super) fn check_confusing_with(&mut self, statement: &StmtWith) {
        for pair in statement.items.windows(2) {
            if matches!(pair[0].optional_vars.as_deref(), Some(Expr::Name(_)))
                && pair[1].optional_vars.is_none()
                && !matches!(pair[1].context_expr, Expr::Call(_))
            {
                self.emit(
                    "W0124",
                    "confusing-with-statement",
                    "Following \"as\" with another context manager looks like a tuple.",
                    statement.range(),
                );
            }
        }
    }
    pub(super) fn check_basic_builtin_call(&mut self, call: &ExprCall) {
        let Expr::Name(name) = call.func.as_ref() else {
            return;
        };
        if self.scopes.declared_in_frame_or_module(name.id.as_str()) {
            return;
        }
        match name.id.as_str() {
            "exec" => self.emit("W0122", "exec-used", "Use of exec", call.range()),
            "eval" => self.emit("W0123", "eval-used", "Use of eval", call.range()),
            "reversed" => self.check_reversed_argument(call),
            _ => {}
        }
    }
    fn check_reversed_argument(&mut self, call: &ExprCall) {
        let Some(argument) = call.arguments.args.first() else {
            return;
        };
        if matches!(argument, Expr::Starred(_)) {
            return;
        }
        let origin = Context::Scope(self.scopes.current_index());
        let unsupported = match self.expression_target_in(argument, origin.clone()) {
            Some(value) => match value.target {
                Target::Value(target) => match self.expand_value(
                    (*target).clone(),
                    &value.context,
                    &mut Default::default(),
                ) {
                    Value::String(_)
                    | Value::Bytes(_)
                    | Value::Mapping(_)
                    | Value::Sequence(_, "list" | "tuple") => false,
                    Value::Number(_)
                    | Value::Boolean(_)
                    | Value::Scalar(_)
                    | Value::Sequence(_, "set") => true,
                    _ => self
                        .slot_nodes(argument, origin.clone())
                        .iter()
                        .any(|(value, _)| matches!(value, Expr::EllipsisLiteral(_))),
                },
                Target::Instance(class, _) | Target::Class(class) => {
                    let context = if let Context::Module(_) = value.context {
                        value.context
                    } else {
                        origin.clone()
                    };
                    self.class_member(class.clone(), context.clone(), "__reversed__", true)
                        .is_none()
                        && ["__getitem__", "__len__"].iter().any(|name| {
                            self.class_member(class.clone(), context.clone(), name, true)
                                .is_none()
                        })
                }
                Target::Function(_) => true,
                _ => false,
            },
            None => {
                matches!(argument, Expr::Call(value) if matches!(value.func.as_ref(), Expr::Name(name) if name.id.as_str()=="iter" && !self.scopes.declared("iter")))
            }
        };
        if unsupported {
            self.emit(
                "E0111",
                "bad-reversed-sequence",
                "The first reversed() argument is not a sequence",
                call.range(),
            );
        }
    }
    pub(super) fn check_misplaced_format(&mut self, call: &ExprCall) {
        if let Expr::Attribute(attribute) = call.func.as_ref() {
            if attribute.attr.as_str() == "format"
                && matches!(attribute.value.as_ref(),Expr::Call(value) if matches!(value.func.as_ref(),Expr::Name(name) if name.id.as_str()=="print"))
            {
                self.emit(
                    "E0119",
                    "misplaced-format-function",
                    "format function is not called on str",
                    call.range(),
                );
            }
        }
    }
}
