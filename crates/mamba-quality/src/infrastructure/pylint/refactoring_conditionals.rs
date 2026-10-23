//! Conditional recommendations preserve raw branch shapes and original suggestions.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{CmpOp, Expr, ExprIf, Stmt, StmtIf};
use ruff_text_size::Ranged;

fn assigned_names(statement: &Stmt) -> Vec<&str> {
    let Stmt::Assign(value) = statement else {
        return Vec::new();
    };
    let mut names = value
        .targets
        .iter()
        .filter_map(|target| {
            if let Expr::Name(name) = target {
                Some(name.id.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

fn boolean_value(statement: &Stmt) -> Option<bool> {
    let value = match statement {
        Stmt::Return(value) => value.value.as_deref()?,
        Stmt::Assign(value) => &value.value,
        _ => return None,
    };
    if let Expr::BooleanLiteral(value) = value {
        Some(value.value)
    } else {
        None
    }
}

impl Checker<'_> {
    pub(super) fn check_refactoring_if(&mut self, conditional: &StmtIf) {
        self.check_simplifiable_if(conditional);
        self.check_nested_if(conditional);
        self.check_dictionary_get(conditional.start().to_u32());
        self.check_min_max_if(conditional);
    }

    fn check_simplifiable_if(&mut self, conditional: &StmtIf) {
        let [left] = conditional.body.as_slice() else {
            return;
        };
        let [clause] = conditional.elif_else_clauses.as_slice() else {
            return;
        };
        if clause.test.is_some() {
            return;
        }
        let [right] = clause.body.as_slice() else {
            return;
        };
        let replacement = match (left, right) {
            (Stmt::Return(_), Stmt::Return(_)) => "'return bool(test)'",
            (Stmt::Assign(_), Stmt::Assign(_)) => {
                let names = assigned_names(left);
                if names.is_empty() || names != assigned_names(right) {
                    return;
                }
                "'var = bool(test)'"
            }
            _ => return,
        };
        if boolean_value(left) != Some(true) || boolean_value(right).is_none() {
            return;
        }
        self.emit(
            "R1703",
            "simplifiable-if-statement",
            &format!("The if statement can be replaced with {replacement}"),
            conditional.range(),
        );
    }

    pub(super) fn check_refactoring_if_expression(&mut self, conditional: &ExprIf) {
        let (Expr::BooleanLiteral(left), Expr::BooleanLiteral(right)) =
            (conditional.body.as_ref(), conditional.orelse.as_ref())
        else {
            return;
        };
        let replacement = match (left.value, right.value) {
            (true, false) if matches!(conditional.test.as_ref(), Expr::Compare(_)) => "'test'",
            (true, false) => "'bool(test)'",
            (false, true) => "'not test'",
            _ => return,
        };
        self.emit(
            "R1719",
            "simplifiable-if-expression",
            &format!("The if expression can be replaced with {replacement}"),
            conditional.range(),
        );
    }

    fn min_max_operand(&self, value: &Expr) -> String {
        if let Expr::StringLiteral(value) = value {
            value.value.to_str().to_owned()
        } else {
            super::display::expression(value, &self.source.text)
        }
    }

    fn check_min_max_if(&mut self, conditional: &StmtIf) {
        if !conditional.elif_else_clauses.is_empty() {
            return;
        }
        let [Stmt::Assign(assignment)] = conditional.body.as_slice() else {
            return;
        };
        let [target] = assignment.targets.as_slice() else {
            return;
        };
        if !matches!(target, Expr::Name(_) | Expr::Attribute(_)) {
            return;
        }
        let Expr::Compare(comparison) = conditional.test.as_ref() else {
            return;
        };
        if comparison.ops.len() != 1 || matches!(&comparison.operands[0], Expr::Subscript(_)) {
            return;
        }
        let mut operator = comparison.ops[0];
        let left = self.min_max_operand(&comparison.operands[0]);
        let right = self.min_max_operand(&comparison.operands[1]);
        let target = self.min_max_operand(target);
        let value = self.min_max_operand(&assignment.value);
        if target != left {
            if target != right {
                return;
            }
            operator = match operator {
                CmpOp::Lt => CmpOp::GtE,
                CmpOp::LtE => CmpOp::Gt,
                CmpOp::Gt => CmpOp::LtE,
                CmpOp::GtE => CmpOp::Lt,
                _ => return,
            };
        }
        if value != left && value != right {
            return;
        }
        let (rule, name, function) = match operator {
            CmpOp::Lt | CmpOp::LtE => ("R1731", "consider-using-max-builtin", "max"),
            CmpOp::Gt | CmpOp::GtE => ("R1730", "consider-using-min-builtin", "min"),
            _ => return,
        };
        self.emit(
            rule,
            name,
            &format!("Consider using '{target} = {function}({target}, {value})' instead of unnecessary if block"),
            conditional.range(),
        );
    }
}
