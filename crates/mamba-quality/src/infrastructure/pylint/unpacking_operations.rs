//! Collection operations retain inferred values and synthesized-node locations.
use super::{
    resolution::Context,
    unpacking_values::{Active, Unpacked},
    values::Value,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    Expr, ExprBinOp, ExprSubscript, Operator, UnaryOp,
};

impl Checker<'_> {
    pub(super) fn unpacking_integer(&self, expression: &Expr, origin: Context) -> Option<i64> {
        if let Expr::UnaryOp(value) = expression {
            let operand = self.unpacking_integer(&value.operand, origin)?;
            return match value.op {
                UnaryOp::USub => operand.checked_neg(),
                UnaryOp::UAdd => Some(operand),
                UnaryOp::Invert => Some(!operand),
                _ => None,
            };
        }
        if let Expr::BinOp(value) = expression {
            let left = self.unpacking_integer(&value.left, origin.clone())?;
            let right = self.unpacking_integer(&value.right, origin)?;
            return match value.op {
                Operator::Add => left.checked_add(right),
                Operator::Sub => left.checked_sub(right),
                Operator::Mult => left.checked_mul(right),
                Operator::Pow => left.checked_pow(u32::try_from(right).ok()?),
                Operator::BitAnd => Some(left & right),
                Operator::BitOr => Some(left | right),
                Operator::BitXor => Some(left ^ right),
                _ => None,
            };
        }
        match self.slot_scalar(expression, origin) {
            Value::Number(
                mamba_language::application::quality_syntax::python_ast::Number::Int(value),
            ) => value.to_string().parse().ok(),
            Value::Boolean(value) => Some(i64::from(value)),
            _ => None,
        }
    }
    pub(super) fn unpacking_binary(
        &self,
        value: &ExprBinOp,
        result: &mut Unpacked,
        origin: Context,
        active: &mut Active,
    ) -> Option<()> {
        let left = self.unpacked_inner(&value.left, origin.clone(), active)?;
        let right = self.unpacked_inner(&value.right, origin.clone(), active)?;
        result.definition = None;
        if value.op == Operator::Add
            && left.kind == right.kind
            && matches!(left.kind, "tuple" | "list")
        {
            result.kind = left.kind;
            result.elements = Some([left.elements?, right.elements?].concat());
            result.iterable = true;
        } else if value.op == Operator::Mult
            && (matches!(left.kind, "tuple" | "list") || matches!(right.kind, "tuple" | "list"))
        {
            let (sequence, number) = if matches!(left.kind, "tuple" | "list") {
                (left, self.unpacking_integer(&value.right, origin)?)
            } else {
                (right, self.unpacking_integer(&value.left, origin)?)
            };
            let number = usize::try_from(number.max(0)).ok()?;
            result.kind = sequence.kind;
            let elements = sequence.elements?;
            result.elements = Some((0..number).flat_map(|_| elements.clone()).collect());
            result.iterable = true;
        } else if left.kind == "string" && right.kind == "string" && value.op == Operator::Add {
            result.iterable = true;
            result.kind = "string";
        } else if left.kind != "constant"
            || right.kind != "constant"
            || !matches!(
                self.slot_scalar(&value.left, origin.clone()),
                Value::Number(_) | Value::Boolean(_)
            )
            || !matches!(
                self.slot_scalar(&value.right, origin),
                Value::Number(_) | Value::Boolean(_)
            )
        {
            return None;
        }
        Some(())
    }
    pub(super) fn unpacking_subscript(
        &self,
        value: &ExprSubscript,
        mut result: Unpacked,
        origin: Context,
        active: &mut Active,
    ) -> Option<Unpacked> {
        let collection = self.unpacked_inner(&value.value, origin.clone(), active)?;
        let elements = collection.elements?;
        let length = i64::try_from(elements.len()).ok()?;
        if let Expr::Slice(slice) = value.slice.as_ref() {
            let bound = |value: Option<&Expr>| -> Option<Option<i64>> {
                match value {
                    None | Some(Expr::NoneLiteral(_)) => Some(None),
                    Some(value) => Some(Some(self.unpacking_integer(value, origin.clone())?)),
                }
            };
            let step = bound(slice.step.as_deref())?.unwrap_or(1);
            if step == 0 {
                return None;
            }
            let normalize = |value: Option<i64>, fallback: i64| {
                let minimum = if step < 0 { -1 } else { 0 };
                let maximum = if step < 0 { length - 1 } else { length };
                value
                    .map(|value| {
                        if value < 0 {
                            value.saturating_add(length)
                        } else {
                            value
                        }
                    })
                    .unwrap_or(fallback)
                    .clamp(minimum, maximum)
            };
            let mut position = normalize(
                bound(slice.lower.as_deref())?,
                if step < 0 { length - 1 } else { 0 },
            );
            let stop = normalize(
                bound(slice.upper.as_deref())?,
                if step < 0 { -1 } else { length },
            );
            let mut selected = Vec::new();
            while if step < 0 {
                position > stop
            } else {
                position < stop
            } {
                selected.push(elements.get(usize::try_from(position).ok()?)?.clone());
                let Some(next) = position.checked_add(step) else {
                    break;
                };
                position = next;
            }
            result.elements = Some(selected);
            result.definition = None;
            result.iterable = true;
            result.kind = collection.kind;
            Some(result)
        } else {
            let index = self.unpacking_integer(&value.slice, origin)?;
            let index = if index < 0 {
                index.checked_add(length)?
            } else {
                index
            };
            self.unpacked_inner(
                elements.get(usize::try_from(index).ok()?)?,
                collection.origin,
                active,
            )
        }
    }
}
