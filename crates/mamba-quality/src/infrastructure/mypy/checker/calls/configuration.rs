//! Pydantic configuration errors follow the original plugin locations.
use super::*;
use quality_syntax::python_ast::StmtClassDef;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_model_configuration(
        &mut self,
        class: &StmtClassDef,
    ) {
        let mut local_alias_generator = false;
        let mut local_names = false;
        pydantic::configuration::visit(class, |key, value, lax_extra| {
            if !pydantic::configuration::tracked(key) {
                return;
            }
            if !pydantic::configuration::valid(key, value, lax_extra) {
                self.emit(
                    "pydantic-config",
                    format!("Invalid value for \"Config.{key}\""),
                    value,
                );
            }
            if key == "alias_generator" {
                local_alias_generator = !matches!(value, Expr::NoneLiteral(_));
            }
            if matches!(key, "populate_by_name" | "validate_by_name")
                && matches!(value, Expr::BooleanLiteral(boolean) if boolean.value)
            {
                local_names = true;
            }
        });
        let has_kwargs = class.arguments.as_ref().is_some_and(|arguments| {
            arguments.keywords.iter().any(|keyword| {
                keyword.arg.as_ref().is_some_and(|key| {
                    pydantic::configuration::tracked(key.as_str())
                        && pydantic::configuration::valid(key.as_str(), &keyword.value, false)
                })
            })
        });
        let has_namespace = class.body.iter().any(|statement| match statement {
            Stmt::ClassDef(config) => config.name.as_str() == "Config",
            Stmt::Assign(assignment) => matches!(assignment.targets.first(), Some(Expr::Name(name)) if name.id.as_str() == "model_config"),
            Stmt::AnnAssign(assignment) => matches!(assignment.target.as_ref(), Expr::Name(name) if name.id.as_str() == "model_config"),
            _ => false,
        });
        if has_kwargs && has_namespace {
            self.emit("misc", "Specifying config in two places is ambiguous, use either Config attribute or class kwargs".into(), class);
        }
        if local_alias_generator && !local_names {
            if let Some(statement) = class.body.last() {
                self.emit(
                    "pydantic-alias",
                    "Required dynamic aliases disallowed".into(),
                    statement,
                );
            }
        }
        for statement in &class.body {
            let Stmt::Assign(assignment) = statement else {
                continue;
            };
            if !matches!(assignment.targets.first(), Some(Expr::Name(name)) if name.id.as_str() == "model_config")
            {
                continue;
            }
            let Expr::Dict(dictionary) = assignment.value.as_ref() else {
                continue;
            };
            if let Some(expected) = self
                .context
                .pydantic_model_symbol("ConfigDict", self.python_target)
            {
                self.check_typed_dict_literal(dictionary, expected, assignment.value.as_ref());
            }
        }
    }
}
