//! Runtime tuple indexing uses all five version-info fields.
use super::*;
use mamba_language::application::quality_syntax::python_ast::UnaryOp;

fn integer(expression: &Expr) -> Option<i64> {
    match expression {
        Expr::NumberLiteral(number) => match &number.value {
            Number::Int(value) => value.as_i64(),
            _ => None,
        },
        Expr::UnaryOp(unary) if unary.op == UnaryOp::USub => integer(&unary.operand)?.checked_neg(),
        Expr::UnaryOp(unary) if unary.op == UnaryOp::UAdd => integer(&unary.operand),
        Expr::BooleanLiteral(boolean) => Some(i64::from(boolean.value)),
        _ => None,
    }
}

impl Context {
    pub(in crate::infrastructure::mypy) fn system_version_index(
        &self,
        items: &[Type],
        expression: &Expr,
    ) -> Option<Type> {
        if let Some(index) = integer(expression) {
            let len = i64::try_from(items.len()).ok()?;
            let index = if index < 0 {
                len.checked_add(index)?
            } else {
                index
            };
            return items.get(usize::try_from(index).ok()?).copied();
        }
        let Expr::Slice(slice) = expression else {
            return Some(self.union(items.to_vec()));
        };
        let len = i64::try_from(items.len()).ok()?;
        let step = slice.step.as_deref().map(integer).unwrap_or(Some(1))?;
        if step == 0 {
            return None;
        }
        let bound = |expression: &Expr| {
            let bound = integer(expression)?;
            let bound = if bound < 0 {
                bound.saturating_add(len)
            } else {
                bound
            };
            Some(if step > 0 {
                bound.clamp(0, len)
            } else {
                bound.clamp(-1, len - 1)
            })
        };
        let mut index = slice
            .lower
            .as_deref()
            .map(bound)
            .unwrap_or(Some(if step > 0 { 0 } else { len - 1 }))?;
        let end = slice
            .upper
            .as_deref()
            .map(bound)
            .unwrap_or(Some(if step > 0 { len } else { -1 }))?;
        let mut result = Vec::new();
        while if step > 0 { index < end } else { index > end } {
            result.push(*items.get(usize::try_from(index).ok()?)?);
            let Some(next) = index.checked_add(step) else {
                break;
            };
            index = next;
        }
        Some(self.intern(Node::Tuple(result)))
    }
}
