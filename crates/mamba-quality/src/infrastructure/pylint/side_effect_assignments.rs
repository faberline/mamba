//! Built-in mutators retain the original implicit-None assignment warning.
use super::{operand_inference::Operand, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall, StmtAssign};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn check_side_effect_assignment(
        &mut self,
        statement: &StmtAssign,
        call: &ExprCall,
    ) -> bool {
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return false;
        };
        let operand = self.operand(&attribute.value);
        let kind = match &operand {
            Operand::Literal(name) => *name,
            Operand::Instance(class, _) if class.identity.0 == "builtins" => class.name.as_str(),
            _ => return false,
        };
        let name = attribute.attr.as_str();
        let implicit_none = match kind {
            "dict" => matches!(name, "clear" | "update"),
            "list" => matches!(
                name,
                "append" | "clear" | "extend" | "insert" | "remove" | "reverse" | "sort"
            ),
            "set" => matches!(
                name,
                "add"
                    | "clear"
                    | "difference_update"
                    | "discard"
                    | "intersection_update"
                    | "remove"
                    | "symmetric_difference_update"
                    | "update"
            ),
            _ => false,
        };
        if !implicit_none {
            return false;
        }
        let hint = match name {
            "reverse" => ", did you mean to use 'reversed(...)' instead?",
            "sort" => ", did you mean to use 'sorted(...)' instead?",
            _ => "",
        };
        self.emit(
            "E1111",
            "assignment-from-no-return",
            &format!("Assigning result of a function call, but '{name}' returns None{hint}"),
            statement.range(),
        );
        true
    }
}
