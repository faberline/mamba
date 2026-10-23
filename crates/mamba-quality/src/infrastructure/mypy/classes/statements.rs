//! Class declarations use pinned semantic reachability, before runtime flow checks.
use mamba_language::application::quality_syntax::{
    python_ast::{BoolOp, ExceptHandler, Expr, Stmt, UnaryOp},
    PythonTarget,
};

mod platform;
mod version;

pub(in crate::infrastructure::mypy) fn collect(body: &[Stmt], target: PythonTarget) -> Vec<&Stmt> {
    let mut result = Vec::new();
    append(body, target, false, &mut result);
    result
}

pub(in crate::infrastructure::mypy) fn collect_installed(
    body: &[Stmt],
    target: PythonTarget,
) -> Vec<&Stmt> {
    let mut result = Vec::new();
    append(body, target, true, &mut result);
    result
}

fn append<'a>(
    body: &'a [Stmt],
    target: PythonTarget,
    expand_try: bool,
    result: &mut Vec<&'a Stmt>,
) {
    for statement in body {
        if let Stmt::If(conditional) = statement {
            let condition = truth(&conditional.test, target);
            if condition != Some(false) {
                append(&conditional.body, target, expand_try, result);
            }
            if condition == Some(true) {
                continue;
            }
            for clause in &conditional.elif_else_clauses {
                let condition = clause.test.as_ref().and_then(|test| truth(test, target));
                if condition != Some(false) {
                    append(&clause.body, target, expand_try, result);
                }
                if condition == Some(true) || clause.test.is_none() {
                    break;
                }
            }
        } else if let Stmt::Try(tried) = statement {
            if expand_try {
                append(&tried.body, target, true, result);
                for handler in &tried.handlers {
                    let ExceptHandler::ExceptHandler(handler) = handler;
                    append(&handler.body, target, true, result);
                }
                append(&tried.orelse, target, true, result);
                append(&tried.finalbody, target, true, result);
            } else {
                result.push(statement);
            }
        } else {
            result.push(statement);
        }
    }
}

pub(in crate::infrastructure::mypy) fn truth(
    expression: &Expr,
    target: PythonTarget,
) -> Option<bool> {
    match expression {
        Expr::Name(name) => special(name.id.as_str()),
        Expr::Attribute(attribute) => special(attribute.attr.as_str()),
        Expr::UnaryOp(unary) if unary.op == UnaryOp::Not => {
            truth(&unary.operand, target).map(|value| !value)
        }
        Expr::BoolOp(boolean) => {
            let mut unknown = false;
            for expression in &boolean.values {
                match truth(expression, target) {
                    Some(value) if value == (boolean.op == BoolOp::Or) => return Some(value),
                    Some(_) => {}
                    None => unknown = true,
                }
            }
            (!unknown).then_some(boolean.op == BoolOp::And)
        }
        Expr::Compare(compare) => {
            version::truth(compare, target).or_else(|| platform::truth(expression))
        }
        _ => platform::truth(expression),
    }
}

fn special(name: &str) -> Option<bool> {
    match name {
        "PY2" => Some(false),
        "PY3" | "MYPY" | "TYPE_CHECKING" => Some(true),
        _ => None,
    }
}
