//! Overload markers retain lexical imports and their qualified aliases.
use super::*;
use mamba_language::application::quality_syntax::python_ast::Stmt;
use std::collections::BTreeSet;

pub(in crate::infrastructure::mypy) fn aliases(
    body: &[Stmt],
    inherited: BTreeSet<String>,
) -> BTreeSet<String> {
    let mut aliases = inherited;
    for import in crate::infrastructure::project::imports(body, "", false) {
        if matches!(import.module.as_str(), "typing" | "typing_extensions") {
            if import.member.as_deref() == Some("overload") {
                aliases.insert(import.local);
            } else if import.member.is_none() {
                aliases.insert(format!("{}.overload", import.local));
            }
        }
    }
    aliases
}

pub(in crate::infrastructure::mypy) fn overload(
    function: &StmtFunctionDef,
    aliases: &BTreeSet<String>,
) -> bool {
    function.decorator_list.iter().any(|decorator| {
        let name = match &decorator.expression {
            Expr::Name(name) => name.id.to_string(),
            Expr::Attribute(attribute) => {
                let Expr::Name(name) = attribute.value.as_ref() else {
                    return false;
                };
                format!("{}.{}", name.id, attribute.attr)
            }
            _ => return false,
        };
        aliases.contains(&name)
    })
}
