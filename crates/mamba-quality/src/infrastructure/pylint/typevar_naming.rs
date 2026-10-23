//! Legacy TypeVar calls retain literal keyword variance and assignment ownership.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall, StmtAssign};
use ruff_text_size::Ranged;
fn literal(value: &Expr) -> bool {
    matches!(
        value,
        Expr::StringLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::BytesLiteral(_)
    )
}
impl Checker<'_> {
    pub(super) fn check_typevar_assignment(
        &mut self,
        statement: &StmtAssign,
    ) -> std::collections::BTreeSet<u32> {
        let mut names = std::collections::BTreeSet::new();
        if !self.scopes.is_module() {
            return names;
        }
        if let (Some(Expr::Tuple(target)), Expr::Tuple(value)) =
            (statement.targets.first(), statement.value.as_ref())
        {
            for (target, value) in target.elts.iter().zip(&value.elts) {
                if self.typevar_target(target, value) {
                    names.insert(target.start().to_u32());
                }
            }
            return names;
        }
        if let Some(Expr::Name(first)) = statement.targets.first() {
            for target in &statement.targets {
                if self.typevar_named_target(target, &statement.value, first.id.as_str()) {
                    names.insert(target.start().to_u32());
                }
            }
        }
        names
    }
    fn typevar_target(&mut self, target: &Expr, value: &Expr) -> bool {
        let Expr::Name(name) = target else {
            return false;
        };
        self.typevar_named_target(target, value, name.id.as_str())
    }
    fn typevar_named_target(&mut self, target: &Expr, value: &Expr, assigned_name: &str) -> bool {
        let (Expr::Name(target), Expr::Call(call)) = (target, value) else {
            return false;
        };
        let Some(name) = self.stdlib_name(&call.func) else {
            return false;
        };
        let kind = match name.as_str() {
            "typing.TypeVar" | "typing_extensions.TypeVar" => "typevar",
            "typing.ParamSpec" | "typing_extensions.ParamSpec" => "paramspec",
            "typing.TypeVarTuple" | "typing_extensions.TypeVarTuple" => "typevartuple",
            _ => return false,
        };
        if self.check_generic_name(assigned_name, kind, target.range()) && kind == "typevar" {
            self.check_typevar_variance(assigned_name, call, target.range());
        }
        true
    }
    fn check_typevar_variance(
        &mut self,
        name: &str,
        call: &ExprCall,
        range: ruff_text_size::TextRange,
    ) {
        let mut covariant = false;
        let mut contravariant = false;
        let mut literal_name = None;
        for keyword in &call.arguments.keywords {
            if !literal(&keyword.value) {
                continue;
            }
            match keyword.arg.as_ref().map(|value| value.as_str()) {
                Some("covariant") => covariant = truth(&keyword.value),
                Some("contravariant") => contravariant = truth(&keyword.value),
                Some("name") => literal_name = Some(&keyword.value),
                _ => {}
            }
        }
        if literal_name.is_none_or(|value| matches!(value, Expr::NoneLiteral(_))) {
            literal_name = call.arguments.args.first().filter(|value| literal(value));
        }
        let detail = if covariant && contravariant {
            self.emit(
                "C0131",
                "typevar-double-variance",
                "TypeVar cannot be both covariant and contravariant",
                range,
            );
            Some(String::new())
        } else if covariant && !name.ends_with("_co") {
            Some(format!(
                ". \"{name}\" is covariant, use \"{}_co\" instead",
                name.strip_suffix("_contra").unwrap_or(name)
            ))
        } else if contravariant && !name.ends_with("_contra") {
            Some(format!(
                ". \"{name}\" is contravariant, use \"{}_contra\" instead",
                name.strip_suffix("_co").unwrap_or(name)
            ))
        } else if !covariant
            && !contravariant
            && (name.ends_with("_co") || name.ends_with("_contra"))
        {
            Some(format!(
                ". \"{name}\" is invariant, use \"{}\" instead",
                name.strip_suffix("_contra")
                    .or_else(|| name.strip_suffix("_co"))
                    .unwrap_or(name)
            ))
        } else {
            None
        };
        if let Some(detail) = detail {
            self.emit(
                "C0105",
                "typevar-name-incorrect-variance",
                &format!("Type variable name does not reflect variance{detail}"),
                range,
            );
        }
        if let Some(value) = literal_name.filter(|value| !matches!(value, Expr::NoneLiteral(_))) {
            let text = if let Expr::StringLiteral(value) = value {
                value.value.to_str().to_string()
            } else {
                super::display::expression(value, "")
            };
            if text != name {
                self.emit(
                    "C0132",
                    "typevar-name-mismatch",
                    &format!(
                        "TypeVar name \"{text}\" does not match assigned variable name \"{name}\""
                    ),
                    range,
                );
            }
        }
    }
}
fn truth(value: &Expr) -> bool {
    match super::values::Value::from_expression(value) {
        super::values::Value::Boolean(value) => value,
        super::values::Value::String(value) => !value.is_empty(),
        super::values::Value::Bytes(value) => !value.is_empty(),
        super::values::Value::Number(value) => match value {
            mamba_language::application::quality_syntax::python_ast::Number::Int(value) => {
                value.to_string() != "0"
            }
            mamba_language::application::quality_syntax::python_ast::Number::Float(value) => {
                value != 0.0
            }
            mamba_language::application::quality_syntax::python_ast::Number::Complex {
                real,
                imag,
            } => real != 0.0 || imag != 0.0,
        },
        _ => false,
    }
}
