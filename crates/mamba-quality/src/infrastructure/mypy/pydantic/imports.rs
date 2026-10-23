//! Model and field names follow their defining modules and local import aliases.
use mamba_language::application::quality_syntax::python_ast::Stmt;
use std::collections::BTreeSet;

fn module_names(
    module: &str,
    local: &str,
    bases: &mut BTreeSet<String>,
    fields: &mut BTreeSet<String>,
) {
    if matches!(module, "pydantic" | "pydantic.main") {
        bases.insert(format!("{local}.BaseModel"));
    }
    if matches!(module, "pydantic" | "pydantic.fields") {
        fields.insert(format!("{local}.Field"));
    }
}

pub(super) fn symbols(statements: &[Stmt]) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut bases = BTreeSet::new();
    let mut fields = BTreeSet::new();
    for statement in statements {
        match statement {
            Stmt::ImportFrom(import) => {
                let Some(module) = import.module.as_ref() else {
                    continue;
                };
                for alias in &import.names {
                    let local = alias.asname.as_ref().unwrap_or(&alias.name).to_string();
                    match (module.as_str(), alias.name.as_str()) {
                        ("pydantic" | "pydantic.main", "BaseModel") => {
                            bases.insert(local);
                        }
                        ("pydantic" | "pydantic.fields", "Field") => {
                            fields.insert(local);
                        }
                        ("pydantic", "main" | "fields") => {
                            module_names(
                                &format!("pydantic.{}", alias.name),
                                &local,
                                &mut bases,
                                &mut fields,
                            );
                        }
                        _ => {}
                    }
                }
            }
            Stmt::Import(import) => {
                for alias in &import.names {
                    let local = alias.asname.as_ref().unwrap_or(&alias.name);
                    module_names(alias.name.as_str(), local.as_str(), &mut bases, &mut fields);
                }
            }
            _ => {}
        }
    }
    (bases, fields)
}
