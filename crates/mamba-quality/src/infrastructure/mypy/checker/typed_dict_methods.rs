//! Typed dictionary method checks share call binding and field-specific diagnostics.
use super::*;
use quality_syntax::python_ast::ExprCall;
use types::Type;

impl Checker<'_> {
    pub(super) fn check_typed_dict_method(&mut self, call: &ExprCall) -> bool {
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return false;
        };
        let Some(ty) = self.infer(&attribute.value) else {
            return false;
        };
        let Some((class, definition)) = self.context.typed_dict(ty) else {
            return false;
        };
        let name = attribute.attr.as_str();
        if !matches!(name, "get" | "pop" | "setdefault" | "update") {
            return false;
        }
        if name == "get" {
            let contextual = match call.arguments.args.as_slice() {
                [Expr::StringLiteral(key), ..] if call.arguments.args.len() > 1 => definition
                    .items
                    .iter()
                    .find(|item| item.name == key.value.to_str())
                    .map(|item| item.ty),
                _ => None,
            };
            let signature = self
                .context
                .typed_dict_get_signature(ty, contextual)
                .unwrap();
            self.check_overload_call(call, &signature);
            return true;
        }
        let mut signature = (*self.context.typed_dict_method(ty, name).unwrap()).clone();
        signature.parameters = signature
            .parameters
            .iter()
            .cloned()
            .map(|mut parameter| {
                parameter.ty = Some(Type::Any);
                parameter
            })
            .collect::<Vec<_>>()
            .into();
        signature.generic = std::sync::Arc::from([]);
        self.bind_call(call, &signature);
        if name == "update" {
            if let [value] = call.arguments.args.as_slice() {
                self.check_typed_dict_update(value, &definition);
            }
            return true;
        }
        let key = call.arguments.args.first().or_else(|| {
            call.arguments
                .keywords
                .iter()
                .find(|keyword| {
                    keyword
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "k")
                })
                .map(|keyword| &keyword.value)
        });
        let Some(key) = key else { return true };
        let default = call.arguments.args.get(1).or_else(|| {
            call.arguments
                .keywords
                .iter()
                .find(|keyword| {
                    keyword
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "default")
                })
                .map(|keyword| &keyword.value)
        });
        if name == "setdefault" && default.is_none() {
            return true;
        }
        if let Some(actual) = self.infer(key) {
            if !self.context.accepts(Type::Str, actual) {
                self.emit("typeddict-item", format!("Argument 1 to \"{name}\" of \"TypedDict\" has incompatible type \"{}\"; expected \"str\"", self.context.name(actual)), key);
            }
        }
        let Some(keys) = self.context.typed_dict_keys(key, &self.environment()) else {
            self.emit(
                "misc",
                "Expected TypedDict key to be string literal".into(),
                key,
            );
            return true;
        };
        for key_name in keys {
            let Some(item) = definition.items.iter().find(|item| item.name == key_name) else {
                self.emit(
                    "typeddict-item",
                    format!("TypedDict \"{}\" has no key \"{key_name}\"", class.name),
                    key,
                );
                return true;
            };
            if name == "pop" && (item.required || item.readonly) {
                self.emit(
                    "misc",
                    format!(
                        "Key \"{key_name}\" of TypedDict \"{}\" cannot be deleted",
                        class.name
                    ),
                    key,
                );
            }
            if name == "setdefault" && item.readonly {
                self.emit(
                    "typeddict-readonly-mutated",
                    format!("ReadOnly TypedDict key \"{key_name}\" TypedDict is mutated"),
                    key,
                );
            }
            if let Some(default) = default {
                if let Some(actual) = self.infer_expected(default, item.ty) {
                    if !self.context.accepts(item.ty, actual) {
                        self.emit("typeddict-item", format!("Argument 2 to \"{name}\" of \"TypedDict\" has incompatible type \"{}\"; expected \"{}\"", self.context.name(actual), self.context.name(item.ty)), default);
                    }
                }
            }
        }
        true
    }

    fn check_typed_dict_update(
        &mut self,
        value: &Expr,
        definition: &super::super::classes::typed_dict::Definition,
    ) {
        let Expr::Dict(dictionary) = value else {
            if let Some(actual) = self.infer(value) {
                if actual.is_any() {
                    return;
                }
                let mut expected = definition.clone();
                for item in &mut expected.items {
                    item.required = false;
                }
                let union_members = self.context.node(actual).and_then(|node| {
                    if let types::Node::Union(members) = node.as_ref() {
                        Some(members.clone())
                    } else {
                        None
                    }
                });
                if let Some(members) = union_members {
                    let sources: Vec<_> = members
                        .iter()
                        .filter_map(|&member| {
                            self.context
                                .typed_dict(member)
                                .map(|(_, definition)| definition)
                        })
                        .collect();
                    if !sources.is_empty() {
                        let mut alternatives: Vec<_> = sources
                            .iter()
                            .map(|source| {
                                let mut alternative = expected.clone();
                                for item in &mut alternative.items {
                                    item.required = source
                                        .items
                                        .iter()
                                        .any(|source| source.name == item.name && source.required);
                                }
                                alternative
                            })
                            .collect();
                        // These partials share the receiver's fields and types.
                        // Remove redundant partials in both source-order directions.
                        for _ in 0..2 {
                            let mut simplified = Vec::with_capacity(alternatives.len());
                            for alternative in alternatives {
                                if !simplified.iter().any(|expected| {
                                    self.context.typed_dict_accepts(expected, &alternative)
                                }) {
                                    simplified.push(alternative);
                                }
                            }
                            alternatives = simplified;
                            if alternatives.len() <= 1 {
                                break;
                            }
                            alternatives.reverse();
                        }
                        if members.iter().all(|&member| {
                            member.is_any()
                                || self.context.typed_dict(member).is_some_and(|(_, source)| {
                                    alternatives.iter().any(|alternative| {
                                        (!alternative.closed || source.closed)
                                            && self.context.typed_dict_accepts(alternative, &source)
                                    })
                                })
                        }) {
                            return;
                        }
                        let expected_types = alternatives
                            .iter()
                            .map(|alternative| {
                                let items = alternative
                                    .items
                                    .iter()
                                    .map(|item| {
                                        format!(
                                            "'{}'{}{}: {}",
                                            item.name,
                                            if item.required { "" } else { "?" },
                                            if item.readonly { "=" } else { "" },
                                            self.context.name(item.ty)
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!("TypedDict({{{items}}})")
                            })
                            .collect::<Vec<_>>()
                            .join(" | ");
                        self.emit(
                            "typeddict-item",
                            format!(
                                "Argument 1 to \"update\" of \"TypedDict\" has incompatible type \"{}\"; expected \"{expected_types}\"",
                                self.context.name(actual)
                            ),
                            value,
                        );
                        return;
                    }
                }
                if let Some((_, source)) = self.context.typed_dict(actual) {
                    for item in &mut expected.items {
                        item.required = source
                            .items
                            .iter()
                            .any(|source| source.name == item.name && source.required);
                    }
                    if self.context.typed_dict_accepts(&expected, &source) {
                        return;
                    }
                }
                let text = expected
                    .items
                    .iter()
                    .map(|item| {
                        format!(
                            "'{}'{}: {}",
                            item.name,
                            if item.required { "" } else { "?" },
                            self.context.name(item.ty)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                self.emit("typeddict-item", format!("Argument 1 to \"update\" of \"TypedDict\" has incompatible type \"{}\"; expected \"TypedDict({{{text}}})\"", self.context.name(actual)), value);
            }
            return;
        };
        let mut provided = Vec::new();
        let mut unknown = Vec::new();
        let mut actual_keys = Vec::new();
        for entry in &dictionary.items {
            let Some(key) = &entry.key else { continue };
            let Expr::StringLiteral(key) = key else {
                self.emit(
                    "misc",
                    "Expected TypedDict key to be string literal".into(),
                    key,
                );
                return;
            };
            let name = key.value.to_str();
            actual_keys.push(name.to_string());
            let Some(item) = definition.items.iter().find(|item| item.name == name) else {
                unknown.push(name.to_string());
                continue;
            };
            if item.readonly {
                self.emit(
                    "typeddict-readonly-mutated",
                    format!("ReadOnly TypedDict key \"{name}\" TypedDict is mutated"),
                    value,
                );
            }
            provided.push((name.to_string(), &entry.value));
        }
        if !unknown.is_empty() {
            let keys = |names: &[String]| {
                names
                    .iter()
                    .map(|name| format!("\"{name}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            if provided.is_empty() {
                for name in unknown {
                    self.emit(
                        "misc",
                        format!("Unexpected TypedDict key \"{name}\""),
                        value,
                    );
                }
            } else {
                let expected: Vec<_> = definition
                    .items
                    .iter()
                    .filter(|item| provided.iter().any(|(name, _)| name == &item.name))
                    .map(|item| item.name.clone())
                    .collect();
                let label = if expected.len() == 1 {
                    format!("key {}", keys(&expected))
                } else {
                    format!("keys ({})", keys(&expected))
                };
                self.emit(
                    "typeddict-item",
                    format!(
                        "Expected TypedDict {label} but found keys ({})",
                        keys(&actual_keys)
                    ),
                    value,
                );
            }
        }
        let mut partial = definition.clone();
        for field in &mut partial.items {
            field.required = false;
        }
        self.check_typed_dict_items("", &partial, &provided, value);
    }
}
