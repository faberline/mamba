//! Computed operands retain Python result kinds without executing expressions.
use super::{
    operand_inference::{Active, Operand},
    resolution::Context,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{ExprBinOp, ExprSubscript, Operator};
use ruff_text_size::Ranged;
impl Checker<'_> {
    pub(super) fn operand_binary(
        &self,
        value: &ExprBinOp,
        origin: Context,
        active: &mut Active,
    ) -> Operand {
        let left = self.operand_in(&value.left, origin.clone(), active);
        let right = self.operand_in(&value.right, origin, active);
        match (&left, &right) {
            (Operand::Literal(left), Operand::Literal(right))
                if matches!(*left, "int" | "bool" | "float" | "complex")
                    && matches!(*right, "int" | "bool" | "float" | "complex") =>
            {
                if *left == "complex" || *right == "complex" {
                    Operand::Literal("complex")
                } else if value.op == Operator::Div || *left == "float" || *right == "float" {
                    Operand::Literal("float")
                } else {
                    Operand::Literal("int")
                }
            }
            (Operand::Literal(left), Operand::Literal(right))
                if left == right
                    && matches!(*left, "str" | "bytes" | "list" | "tuple")
                    && value.op == Operator::Add =>
            {
                Operand::Literal(left)
            }
            (Operand::Literal(left), Operand::Literal("int" | "bool"))
                if matches!(*left, "str" | "bytes" | "list" | "tuple")
                    && value.op == Operator::Mult =>
            {
                Operand::Literal(left)
            }
            (Operand::Literal("int" | "bool"), Operand::Literal(right))
                if matches!(*right, "str" | "bytes" | "list" | "tuple")
                    && value.op == Operator::Mult =>
            {
                Operand::Literal(right)
            }
            _ => Operand::Unknown,
        }
    }
    pub(super) fn check_potential_index(&mut self, value: &ExprSubscript) {
        let elements = match value.value.as_ref() {
            mamba_language::application::quality_syntax::python_ast::Expr::List(value) => {
                &value.elts
            }
            mamba_language::application::quality_syntax::python_ast::Expr::Tuple(value) => {
                &value.elts
            }
            _ => return,
        };
        if !matches!(self.operand(&value.slice), Operand::Literal("int" | "bool")) {
            return;
        }
        let Some((index, origin)) = self
            .operand_nodes(&value.slice, Context::Scope(self.scopes.current_index()))
            .into_iter()
            .next()
        else {
            return;
        };
        let Some(index) = self.unpacking_integer(&index, origin) else {
            return;
        };
        let length = elements
            .iter()
            .map(|value| {
                if let mamba_language::application::quality_syntax::python_ast::Expr::Starred(
                    value,
                ) = value
                {
                    self.unpacked_value(&value.value, Context::Scope(self.scopes.current_index()))
                        .and_then(|value| value.elements)
                        .map_or(1, |values| values.len())
                } else {
                    1
                }
            })
            .sum::<usize>();
        if index >= 0 && usize::try_from(index).is_ok_and(|index| index >= length) {
            self.emit(
                "E0643",
                "potential-index-error",
                "Invalid index for iterable length",
                value.range(),
            );
        }
    }
}
