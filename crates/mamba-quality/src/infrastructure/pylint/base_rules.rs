//! Base diagnostics retain the original declaration span and Python text.
use super::{callables::Target, classes::Class, mro, resolution::Context, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtClassDef};
use ruff_text_size::Ranged;
use std::rc::Rc;

pub(super) fn representation(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut result = String::new();
    result.push(quote);
    for character in value.chars() {
        match character {
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            character if character == quote => {
                result.push('\\');
                result.push(character);
            }
            character => result.push(character),
        }
    }
    result.push(quote);
    result
}

impl Checker<'_> {
    pub(super) fn check_bases(&mut self, statement: &StmtClassDef) {
        let context = Context::Scope(self.scopes.current_index());
        let mut class = Class::from_statement(statement, self.module_name, self.is_package);
        self.modules.apply_class_stores(&mut class);
        self.build_attributes.apply(&mut class);
        let class = Rc::new(class);
        let range = self.declaration_range(statement.name.range());
        self.check_slots(statement, class.clone(), context.clone());
        for base in &class.bases {
            if let Some(resolved) = self.declared_base(base, context.clone()) {
                let invalid_builtin = matches!(&resolved.target, Target::Class(class) if class.identity.0 == "builtins" && super::defaults::fixed().invalid_base(&class.name));
                if invalid_builtin
                    || matches!(
                        resolved.target,
                        Target::Instance(_, _) | Target::Function(_)
                    )
                    || matches!(resolved.target, Target::Value(ref value) if !matches!(value.as_ref(), Value::Unknown))
                {
                    let source = match base {
                        Expr::StringLiteral(string) => representation(string.value.to_str()),
                        _ => self.source.text[base.range()].to_string(),
                    };
                    self.emit(
                        "E0239",
                        "inherit-non-class",
                        &format!(
                            "Inheriting {}, which is not a class.",
                            representation(&source)
                        ),
                        range,
                    );
                }
                if let Target::Class(ancestor) = resolved.target {
                    self.check_enum_base(statement, ancestor, resolved.context);
                }
            }
        }
        for base in &class.bases {
            if self.declared_base(base, context.clone()).is_some_and(|resolved| matches!(resolved.target, Target::Class(class) if class.identity.0 == "builtins" && class.name == "object")) {
                self.emit("R0205", "useless-object-inheritance", &format!("Class '{}' inherits from object, can be safely removed from bases in python3", statement.name), range);
            }
        }
        for base in &class.bases {
            if let Some(super::resolution::Resolved {
                target: Target::Class(ancestor),
                context: origin,
            }) = self.declared_base(base, context.clone())
            {
                if ancestor.decorators.iter().any(|decorator| {
                    matches!(
                        self.qualified_decorator(decorator, origin.clone())
                            .as_deref(),
                        Some("typing.final" | "typing_extensions.final")
                    )
                }) {
                    self.emit(
                        "W0240",
                        "subclassed-final-class",
                        &format!(
                            "Class '{}' is a subclass of a class decorated with typing.final: '{}'",
                            statement.name, ancestor.name
                        ),
                        range,
                    );
                }
            }
        }
        if let Err(error) = self.mro(class.clone(), context.clone()) {
            match error {
                mro::Error::Duplicate => self.emit(
                    "E0241",
                    "duplicate-bases",
                    &format!("Duplicate bases for class '{}'", statement.name),
                    range,
                ),
                mro::Error::Inconsistent => self.emit(
                    "E0240",
                    "inconsistent-mro",
                    &format!(
                        "Inconsistent method resolution order for class '{}'",
                        statement.name
                    ),
                    range,
                ),
                mro::Error::Cycle => {}
            }
        }
        self.check_declared_slots(statement, &class, context);
    }
}
