//! Open modes and encodings use the pinned standard-library inference gates.
use super::{resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;
fn valid_mode(value: &Expr) -> bool {
    let Expr::StringLiteral(value) = value else {
        return false;
    };
    let value = value.value.to_str();
    let mut seen = BTreeSet::new();
    if value
        .chars()
        .any(|value| !"rwatb+Ux".contains(value) || !seen.insert(value))
    {
        return false;
    }
    if seen.contains(&'U') && seen.iter().any(|value| "wax".contains(*value)) {
        return false;
    }
    if seen.contains(&'t') && seen.contains(&'b') {
        return false;
    }
    [
        seen.contains(&'r') || seen.contains(&'U'),
        seen.contains(&'w'),
        seen.contains(&'a'),
        seen.contains(&'x'),
    ]
    .into_iter()
    .filter(|value| *value)
    .count()
        == 1
}
impl Checker<'_> {
    pub(super) fn check_open_arguments(&mut self, call: &ExprCall) {
        let name = match call.func.as_ref() {
            Expr::Name(value) => value.id.as_str(),
            Expr::Attribute(value) => value.attr.as_str(),
            _ => return,
        };
        if !matches!(name, "open" | "file" | "read_text" | "write_text") {
            return;
        }
        let pathlib = self
            .stdlib_name(&call.func)
            .is_some_and(|name| name.starts_with("pathlib."));
        let mode = self.stdlib_argument(call, usize::from(!pathlib), "mode");
        let mode = if let Some((value, origin)) = mode {
            let constant = self.stdlib_constant(&value, origin.clone()).or_else(|| {
                let Expr::Name(name) = value else {
                    return None;
                };
                let Context::Scope(index) = origin else {
                    return None;
                };
                let (value, owner) = self.scopes.parameter_default(name.id.as_str(), index)?;
                self.stdlib_constant(&value, Context::Scope(owner))
            });
            let Some(value) = constant else {
                return;
            };
            if matches!(name, "open" | "file") && !valid_mode(&value) {
                let text = if let Expr::StringLiteral(value) = &value {
                    value.value.to_str().to_string()
                } else {
                    super::display::expression(&value, "")
                };
                self.emit(
                    "W1501",
                    "bad-open-mode",
                    &format!("\"{text}\" is not a valid mode for open."),
                    call.range(),
                );
            }
            Some(value)
        } else {
            None
        };
        if matches!(mode,Some(Expr::StringLiteral(ref value)) if value.value.to_str().contains('b'))
        {
            return;
        }
        let position = if pathlib {
            match name {
                "read_text" => 0,
                "write_text" => 1,
                _ => 2,
            }
        } else {
            3
        };
        let encoding = self.stdlib_argument(call, position, "encoding");
        let missing = encoding.is_none_or(|(value, origin)| {
            matches!(
                self.stdlib_constant(&value, origin),
                Some(Expr::NoneLiteral(_))
            )
        });
        if missing {
            self.emit(
                "W1514",
                "unspecified-encoding",
                "Using open without explicitly specifying an encoding",
                call.range(),
            );
        }
    }
}
