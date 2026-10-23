//! Default exception handlers retain original raising and ancestry decisions.
use super::{classes::Class, exception_values::Inferred, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{ExceptHandler, Expr, Stmt, StmtTry};
use ruff_text_size::Ranged;
use std::rc::Rc;

type Exception = (Rc<Class>, Context);

fn raising(body: &[Stmt]) -> bool {
    body.iter()
        .any(|statement| matches!(statement, Stmt::Raise(_)))
}

impl Checker<'_> {
    fn caught_classes(&self, value: Option<&Expr>) -> Option<Vec<Exception>> {
        let Some(value) = value else {
            return Some(Vec::new());
        };
        if let Expr::Tuple(tuple) = value {
            return tuple
                .elts
                .iter()
                .map(|value| self.inferred_base(value, Context::Scope(self.scopes.current_index())))
                .collect();
        }
        self.inferred_base(value, Context::Scope(self.scopes.current_index()))
            .map(|value| vec![value])
    }

    fn check_immediate_rethrow(&mut self, attempt: &StmtTry) {
        let mut pending = None;
        for handler in &attempt.handlers {
            let ExceptHandler::ExceptHandler(handler) = handler;
            let classes = self.caught_classes(handler.type_.as_deref());
            if let Some((_, earlier)) = &pending {
                let Some(classes) = &classes else {
                    return;
                };
                if classes.is_empty() {
                    return;
                }
                let Some(earlier): &Option<Vec<Exception>> = earlier else {
                    return;
                };
                if classes.iter().any(|(class, _)| {
                    earlier.iter().any(|(old, context)| {
                        old.identity != class.identity
                            && self
                                .lineage(old.clone(), context.clone())
                                .iter()
                                .any(|(ancestor, _)| ancestor.identity == class.identity)
                    })
                }) {
                    pending = None;
                }
            }
            if matches!(handler.body.first(), Some(Stmt::Raise(raised)) if raised.exc.is_none()) {
                pending = Some((handler.range(), classes));
            }
        }
        if let Some((range, _)) = pending {
            self.emit(
                "W0706",
                "try-except-raise",
                "The except handler raises immediately",
                range,
            );
        }
    }

    pub(super) fn check_exception_handlers(&mut self, attempt: &StmtTry) {
        self.check_immediate_rethrow(attempt);
        let mut previous_classes: Vec<Exception> = Vec::new();
        for (index, handler) in attempt.handlers.iter().enumerate() {
            let ExceptHandler::ExceptHandler(handler) = handler;
            let Some(value) = &handler.type_ else {
                if !raising(&handler.body) {
                    self.emit(
                        "W0702",
                        "bare-except",
                        "No exception type(s) specified",
                        handler.range(),
                    );
                }
                if index + 1 < attempt.handlers.len() {
                    self.emit(
                        "E0701",
                        "bad-except-order",
                        "Bad except clauses order (empty except clause should always appear last)",
                        attempt.range(),
                    );
                }
                continue;
            };
            if let Expr::BoolOp(boolean) = value.as_ref() {
                self.emit(
                    "W0711",
                    "binary-op-exception",
                    &format!(
                        "Exception to catch is the result of a binary \"{}\" operation",
                        boolean.op
                    ),
                    handler.range(),
                );
                continue;
            }
            if let Expr::BinOp(binary) = value.as_ref() {
                let both_tuples = matches!(binary.left.as_ref(), Expr::Tuple(_))
                    && matches!(binary.right.as_ref(), Expr::Tuple(_));
                if !both_tuples
                    || binary.op
                        != mamba_language::application::quality_syntax::python_ast::Operator::Add
                {
                    let left = super::display::expression(&binary.left, &self.source.text);
                    let right = super::display::expression(&binary.right, &self.source.text);
                    let separator = if both_tuples { " + " } else { ", " };
                    self.emit("W0716", "wrong-exception-operation", &format!("Invalid exception operation. Did you mean '({left}{separator}{right})' instead?"), value.range());
                }
                continue;
            }
            let parts: Vec<_> = match value.as_ref() {
                Expr::Tuple(tuple) => tuple.elts.iter().collect(),
                value => vec![value],
            };
            let mut current_classes: Vec<Exception> = Vec::new();
            for part in parts {
                match self.exception_value(part) {
                    Inferred::Class(class, context, _) => {
                        if current_classes
                            .iter()
                            .any(|(seen, _)| seen.identity == class.identity)
                        {
                            continue;
                        }
                        if !self.standard_exception(class.clone(), context.clone())
                            && self.known_exception_bases(class.clone(), context.clone())
                        {
                            self.emit_catch_error(value, &class.name);
                        }
                        let lineage = self.lineage(class.clone(), context.clone());
                        for (previous, _) in &previous_classes {
                            if previous.identity != class.identity
                                && lineage
                                    .iter()
                                    .any(|(ancestor, _)| ancestor.identity == previous.identity)
                            {
                                self.emit(
                                    "E0701",
                                    "bad-except-order",
                                    &format!(
                                        "Bad except clauses order ({} is an ancestor class of {})",
                                        previous.name, class.name
                                    ),
                                    value.range(),
                                );
                            }
                        }
                        if class.identity.0 == "builtins"
                            && matches!(class.name.as_str(), "Exception" | "BaseException")
                            && !raising(&handler.body)
                        {
                            self.emit(
                                "W0718",
                                "broad-exception-caught",
                                &format!("Catching too general exception {}", class.name),
                                value.range(),
                            );
                        }
                        if previous_classes
                            .iter()
                            .any(|(previous, _)| previous.identity == class.identity)
                        {
                            self.emit(
                                "W0705",
                                "duplicate-except",
                                &format!(
                                    "Catching previously caught exception type {}",
                                    class.name
                                ),
                                value.range(),
                            );
                        }
                        current_classes.push((class, context));
                    }
                    Inferred::Scalar("NoneType") if !matches!(part, Expr::NoneLiteral(_)) => {}
                    Inferred::Scalar(_) | Inferred::Function(_) => self.emit_catch_error(
                        value,
                        &super::display::expression(part, &self.source.text),
                    ),
                    Inferred::Unknown => {}
                }
            }
            previous_classes.extend(current_classes);
        }
    }

    fn emit_catch_error(&mut self, value: &Expr, name: &str) {
        self.emit(
            "E0712",
            "catching-non-exception",
            &format!("Catching an exception which doesn't inherit from Exception: {name}"),
            value.range(),
        );
    }
}
