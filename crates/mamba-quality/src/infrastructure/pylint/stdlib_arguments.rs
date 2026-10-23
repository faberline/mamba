//! Standard-library arguments keep source ownership when expanding dictionaries.
use super::{resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall};
impl Checker<'_> {
    pub(super) fn stdlib_argument(
        &self,
        call: &ExprCall,
        position: usize,
        name: &str,
    ) -> Option<(Expr, Context)> {
        let origin = Context::Scope(self.scopes.current_index());
        if let Some(value) = call.arguments.args.get(position) {
            return Some((value.clone(), origin));
        }
        if let Some(value) = call.arguments.keywords.iter().find(|value| {
            value
                .arg
                .as_ref()
                .is_some_and(|value| value.as_str() == name)
        }) {
            return Some((value.value.clone(), origin));
        }
        for keyword in &call.arguments.keywords {
            if keyword.arg.is_some() {
                continue;
            }
            let Some(value) = self.unpacked_value(&keyword.value, origin.clone()) else {
                continue;
            };
            for (key, expression) in value.entries.unwrap_or_default() {
                if matches!(key,Expr::StringLiteral(key) if key.value.to_str()==name) {
                    return Some((expression, value.origin));
                }
            }
        }
        None
    }
    pub(super) fn stdlib_constant(&self, value: &Expr, origin: Context) -> Option<Expr> {
        let values = self.operand_nodes(value, origin);
        let (first, _) = values.first()?;
        if !values.iter().all(|(value, _)| {
            matches!(
                value,
                Expr::StringLiteral(_)
                    | Expr::NumberLiteral(_)
                    | Expr::BooleanLiteral(_)
                    | Expr::NoneLiteral(_)
                    | Expr::BytesLiteral(_)
            )
        }) {
            return None;
        }
        let kind = super::values::Value::from_expression(first)
            .builtin_name()?
            .to_string();
        values
            .iter()
            .all(|(value, _)| {
                super::values::Value::from_expression(value).builtin_name() == Some(kind.as_str())
            })
            .then(|| first.clone())
    }
}
