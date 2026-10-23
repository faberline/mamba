//! Revealed applied classes retain the module and captured parameter spelling.
use super::super::signatures::{Kind, Parameter};
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;

impl Context {
    pub(super) fn revealed_class(&self, id: u32, arguments: &[Type], value: bool) -> String {
        let class = self.class(id);
        if !value {
            if let Some(items) = self.system_version_items(class.instance) {
                return format!(
                    "tuple[{}, fallback=sys._version_info]",
                    items
                        .into_iter()
                        .map(|ty| self.revealed_name(ty))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        let module = class
            .namespace
            .split_once(':')
            .map_or(class.namespace.as_str(), |(_, module)| module);
        let name = if module == "<builtins>" || module.starts_with("builtins.") {
            class.name.clone()
        } else if class.offset == 0
            && !class.namespace.contains(':')
            && module.rsplit('.').next() == Some(class.name.as_str())
        {
            module.to_string()
        } else {
            format!("{module}.{}", class.name)
        };
        let closure = self
            .classes
            .borrow()
            .embedded_typed_dict_closed
            .get(&id)
            .copied();
        if !value {
            if let Some(closed) = closure {
                let definition = class
                    .typed_dict
                    .as_ref()
                    .expect("embedded typed dictionary");
                let items = definition
                    .items
                    .iter()
                    .map(|item| {
                        let mut flags = String::new();
                        if !item.required {
                            flags.push('?');
                        }
                        if item.readonly {
                            flags.push('=');
                        }
                        format!("'{}'{flags}: {}", item.name, self.revealed_name(item.ty))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let closed = if closed { ", closed=True" } else { "" };
                return format!("TypedDict({name}, {{{items}}}{closed})");
            }
        }
        let instance = if arguments.is_empty() {
            name
        } else {
            format!(
                "{name}[{}]",
                self.display_generic_arguments(&class.generic, arguments, true)
            )
        };
        if value {
            format!("type[{instance}]")
        } else {
            instance
        }
    }

    pub(super) fn revealed_parameters(
        &self,
        parameters: &[Parameter],
        ellipsis: bool,
        target: PythonTarget,
    ) -> String {
        if ellipsis {
            return "...".into();
        }
        let mut arguments = Vec::new();
        let mut star = false;
        for parameter in parameters {
            if parameter.kind == Kind::KeywordOnly && !star {
                arguments.push("*".into());
                star = true;
            }
            let prefix = match parameter.kind {
                Kind::VariadicPositional => {
                    star = true;
                    "*"
                }
                Kind::VariadicKeyword => "**",
                _ => "",
            };
            let ty = self.revealed_name_for(parameter.ty.unwrap_or(Type::Any), target);
            let mut argument =
                if parameter.name.is_empty() || parameter.kind == Kind::PositionalOnly {
                    format!("{prefix}{ty}")
                } else {
                    format!("{prefix}{}: {ty}", parameter.name)
                };
            if !parameter.required
                && matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword | Kind::KeywordOnly
                )
            {
                argument.push_str(" =");
            }
            arguments.push(argument);
        }
        format!("[{}]", arguments.join(", "))
    }
}
