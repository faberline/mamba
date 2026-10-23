//! Direct sys.platform conditions follow pinned semantic reachability.
use mamba_language::application::quality_syntax::python_ast::{CmpOp, Expr, ExprCall};

fn current() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("darwin")
    } else if cfg!(target_os = "linux") {
        Some("linux")
    } else {
        None
    }
}

fn sys_platform(expression: &Expr) -> bool {
    matches!(expression, Expr::Attribute(attribute)
        if attribute.attr.as_str() == "platform"
            && matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == "sys"))
}

// Mypy stores positional, starred and keyword values in one args sequence.
// consider_sys_platform checks that sequence, without checking argument kinds.
fn single_argument(call: &ExprCall) -> Option<&Expr> {
    match (
        call.arguments.args.as_slice(),
        call.arguments.keywords.as_slice(),
    ) {
        ([argument], []) => Some(match argument {
            Expr::Starred(starred) => starred.value.as_ref(),
            argument => argument,
        }),
        ([], [keyword]) => Some(&keyword.value),
        _ => None,
    }
}

fn strings_contain(expression: &Expr, platform: &str) -> Option<bool> {
    let items: &[Expr] = match expression {
        Expr::Tuple(tuple) => &tuple.elts,
        Expr::Set(set) => &set.elts,
        Expr::List(list) => &list.elts,
        _ => return None,
    };
    items.iter().try_fold(false, |found, item| {
        let Expr::StringLiteral(string) = item else {
            return None;
        };
        Some(found || string.value.to_str() == platform)
    })
}

pub(super) fn truth(expression: &Expr) -> Option<bool> {
    let platform = current()?;
    match expression {
        Expr::Compare(compare) => {
            let [left, right] = compare.operands.as_ref() else {
                return None;
            };
            let [op] = compare.ops.as_ref() else {
                return None;
            };
            if !sys_platform(left) {
                return None;
            }
            match op {
                CmpOp::Eq | CmpOp::NotEq => {
                    let Expr::StringLiteral(string) = right else {
                        return None;
                    };
                    let equal = platform == string.value.to_str();
                    Some(if *op == CmpOp::NotEq { !equal } else { equal })
                }
                CmpOp::In | CmpOp::NotIn => {
                    let contains = strings_contain(right, platform)?;
                    Some(if *op == CmpOp::NotIn {
                        !contains
                    } else {
                        contains
                    })
                }
                _ => None,
            }
        }
        Expr::Call(call) => {
            let Expr::Attribute(attribute) = call.func.as_ref() else {
                return None;
            };
            let Expr::StringLiteral(prefix) = single_argument(call)? else {
                return None;
            };
            if attribute.attr.as_str() != "startswith" || !sys_platform(&attribute.value) {
                return None;
            }
            Some(platform.starts_with(prefix.value.to_str()))
        }
        _ => None,
    }
}
