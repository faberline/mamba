//! Typed dictionary expressions use field-specific contexts and original error spans.
use super::super::classes::typed_dict::Definition;
use super::*;
use quality_syntax::python_ast::{ExprCall, ExprDict, ExprSubscript};
use std::collections::BTreeSet;
use types::Type;

fn names(keys: &[String]) -> String {
    keys.iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Clone, Copy)]
enum TypedDictKey<'a> {
    Expression(&'a Expr),
    Named(&'a str),
}

#[derive(Clone, Copy)]
enum TypedDictValue<'a> {
    Expression(&'a Expr),
    Projected { ty: Type, source: &'a Expr },
}

fn value_slot<'a, 'e>(
    items: &'a mut Vec<(String, Vec<TypedDictValue<'e>>)>,
    key: &str,
) -> &'a mut Vec<TypedDictValue<'e>> {
    let index = items
        .iter()
        .position(|(name, _)| name == key)
        .unwrap_or_else(|| {
            items.push((key.into(), Vec::new()));
            items.len() - 1
        });
    &mut items[index].1
}

impl Checker<'_> {
    pub(super) fn check_typed_dict_literal(
        &mut self,
        dictionary: &ExprDict,
        expected: Type,
        context: &impl Ranged,
    ) -> Option<Type> {
        let (class, definition) = self.context.typed_dict(expected)?;
        if dictionary.items.iter().any(|item| item.key.is_none()) {
            if class.generic.is_empty() {
                let arguments: Vec<_> = dictionary
                    .items
                    .iter()
                    .map(|item| (item.key.as_ref().map(TypedDictKey::Expression), &item.value))
                    .collect();
                self.check_typed_dict_unpack(&arguments, &class.name, &definition, context);
            }
            return Some(expected);
        }
        let mut items = Vec::new();
        for item in &dictionary.items {
            let Some(key) = &item.key else {
                return Some(expected);
            };
            let Expr::StringLiteral(key) = key else {
                self.emit(
                    "misc",
                    "Expected TypedDict key to be string literal".into(),
                    key,
                );
                return Some(expected);
            };
            items.push((key.value.to_str().to_string(), &item.value));
        }
        self.check_typed_dict_items(&class.name, &definition, &items, context);
        Some(expected)
    }

    fn check_typed_dict_unpack(
        &mut self,
        arguments: &[(Option<TypedDictKey<'_>>, &Expr)],
        name: &str,
        definition: &Definition,
        context: &impl Ranged,
    ) {
        // Unknown, union, mapping, and generic inputs remain deferred. Known
        // open and closed TypedDict instances use the same ordered key merge.
        let mut unpacked = Vec::new();
        for &(_, source) in arguments.iter().filter(|(key, _)| key.is_none()) {
            let Some(actual) = self.infer(source) else {
                return;
            };
            let actual = self.context.expanded_alias_instance(actual);
            if self.context.is_class_value(actual) {
                return;
            }
            let Some((class, fields)) = self.context.typed_dict(actual) else {
                return;
            };
            if !class.generic.is_empty() {
                return;
            }
            unpacked.push(fields);
        }
        let mut unpacked = unpacked.into_iter();
        let mut items = Vec::new();
        let mut present = BTreeSet::new();
        let mut last_open = None;
        for &(key, source) in arguments {
            if let Some(key) = key {
                let key_name = match key {
                    TypedDictKey::Named(name) => name.to_string(),
                    TypedDictKey::Expression(key) => {
                        let Some(mut keys) = self.context.typed_dict_keys(key, &self.environment())
                        else {
                            self.emit(
                                "literal-required",
                                "Expected TypedDict key to be string literal".into(),
                                key,
                            );
                            return;
                        };
                        keys.sort();
                        keys.dedup();
                        let [name] = keys.as_slice() else {
                            self.emit(
                                "literal-required",
                                "Expected TypedDict key to be string literal".into(),
                                key,
                            );
                            return;
                        };
                        name.clone()
                    }
                };
                let values = value_slot(&mut items, &key_name);
                values.clear();
                values.push(TypedDictValue::Expression(source));
                present.insert(key_name);
            } else {
                let fields = unpacked.next().unwrap();
                if !fields.closed {
                    last_open = Some(source);
                }
                for field in fields.items {
                    let values = value_slot(&mut items, &field.name);
                    if field.required {
                        present.insert(field.name.clone());
                        // A required unpack replaces earlier projections, but mypy
                        // still checks an earlier explicit expression for this key.
                        let explicit = values
                            .first()
                            .copied()
                            .filter(|value| matches!(value, TypedDictValue::Expression(_)));
                        values.clear();
                        if let Some(explicit) = explicit {
                            values.push(explicit);
                        }
                    }
                    values.push(TypedDictValue::Projected {
                        ty: field.ty,
                        source,
                    });
                }
            }
        }
        if let Some(source) = last_open {
            if definition.closed {
                self.emit(
                    "typeddict-item",
                    "Cannot unpack item that may contain extra keys into a closed TypedDict".into(),
                    source,
                );
            }
            let absent: Vec<_> = definition
                .items
                .iter()
                .filter(|field| !field.required && !items.iter().any(|(key, _)| key == &field.name))
                .map(|field| field.name.clone())
                .collect();
            if !absent.is_empty() {
                let keys = if absent.len() == 1 {
                    format!("key {}", names(&absent))
                } else {
                    format!("keys ({})", names(&absent))
                };
                self.emit(
                    "typeddict-item",
                    format!("Non-required {keys} not explicitly found in any ** item"),
                    source,
                );
            }
        }
        let missing: Vec<_> = definition
            .items
            .iter()
            .filter(|field| field.required && !present.contains(&field.name))
            .map(|field| field.name.clone())
            .collect();
        let extra: Vec<_> = items
            .iter()
            .filter(|(key, _)| !definition.items.iter().any(|field| &field.name == key))
            .map(|(key, _)| key.clone())
            .collect();
        for (label, rule, keys) in [
            ("Extra", "typeddict-unknown-key", &extra),
            ("Missing", "typeddict-item", &missing),
        ] {
            if !keys.is_empty() {
                let keys = if keys.len() == 1 {
                    format!("key {}", names(keys))
                } else {
                    format!("keys ({})", names(keys))
                };
                self.emit(
                    rule,
                    format!("{label} {keys} for TypedDict \"{name}\""),
                    context,
                );
            }
        }
        // Match mypy's early stop only when actual map keys are a proper subset
        // of required keys. Optional source keys do not guarantee their presence.
        if definition
            .items
            .iter()
            .any(|field| field.required && !items.iter().any(|(key, _)| key == &field.name))
            && items.iter().all(|(key, _)| {
                definition
                    .items
                    .iter()
                    .any(|field| field.required && &field.name == key)
            })
        {
            return;
        }
        for field in &definition.items {
            for (_, values) in items.iter().filter(|(key, _)| key == &field.name) {
                for value in values {
                    let (actual, source) = match *value {
                        TypedDictValue::Expression(source) => {
                            (self.infer_expected(source, field.ty), source)
                        }
                        TypedDictValue::Projected { ty, source } => (Some(ty), source),
                    };
                    if let Some(actual) = actual {
                        if !self.context.accepts(field.ty, actual) {
                            self.emit("typeddict-item", format!("Incompatible types (expression has type \"{}\", TypedDict item \"{}\" has type \"{}\")", self.context.name(actual), field.name, self.context.name(field.ty)), source);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn check_typed_dict_items(
        &mut self,
        name: &str,
        definition: &Definition,
        items: &[(String, &Expr)],
        context: &impl Ranged,
    ) {
        let missing: Vec<_> = definition
            .items
            .iter()
            .filter(|item| item.required && !items.iter().any(|(name, _)| name == &item.name))
            .map(|item| item.name.clone())
            .collect();
        let extra: Vec<_> = items
            .iter()
            .filter(|(name, _)| !definition.items.iter().any(|item| &item.name == name))
            .map(|(name, _)| name.clone())
            .collect();
        for (label, rule, keys) in [
            ("Missing", "typeddict-item", &missing),
            ("Extra", "typeddict-unknown-key", &extra),
        ] {
            if keys.is_empty() {
                continue;
            }
            let key = if keys.len() == 1 {
                format!("key {}", names(keys))
            } else {
                format!("keys ({})", names(keys))
            };
            self.emit(
                rule,
                format!("{label} {key} for TypedDict \"{name}\""),
                context,
            );
        }
        if !missing.is_empty()
            && items.iter().all(|(name, _)| {
                definition
                    .items
                    .iter()
                    .any(|item| item.required && &item.name == name)
            })
        {
            return;
        }
        for field in &definition.items {
            for (_, value) in items.iter().filter(|(name, _)| name == &field.name) {
                if let Some(actual) = self.infer_expected(value, field.ty) {
                    if !self.context.accepts(field.ty, actual) {
                        self.emit("typeddict-item", format!("Incompatible types (expression has type \"{}\", TypedDict item \"{}\" has type \"{}\")", self.context.name(actual), field.name, self.context.name(field.ty)), *value);
                    }
                }
            }
        }
    }

    pub(super) fn check_typed_dict_call(&mut self, call: &ExprCall) -> bool {
        let Some(ty) = self
            .infer(&call.func)
            .filter(|&ty| self.context.is_class_value(ty))
        else {
            return false;
        };
        let Some((class, definition)) = self.context.typed_dict(ty) else {
            return false;
        };
        if let [Expr::Dict(dictionary)] = call.arguments.args.as_slice() {
            self.check_typed_dict_literal(dictionary, class.instance, call);
            return true;
        }
        if let [Expr::Call(dictionary)] = call.arguments.args.as_slice() {
            if matches!(dictionary.func.as_ref(), Expr::Name(name) if name.id.as_str() == "dict")
                && dictionary.arguments.args.is_empty()
            {
                let items: Option<Vec<_>> = dictionary
                    .arguments
                    .keywords
                    .iter()
                    .map(|keyword| Some((keyword.arg.as_ref()?.to_string(), &keyword.value)))
                    .collect();
                if let Some(items) = items {
                    self.check_typed_dict_items(&class.name, &definition, &items, call);
                }
                return true;
            }
        }
        if !call.arguments.args.is_empty() {
            self.emit(
                "misc",
                "Expected keyword arguments, {...}, or dict(...) in TypedDict constructor".into(),
                call,
            );
            return true;
        }
        if definition.closed
            && class.generic.is_empty()
            && call
                .arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none())
        {
            let arguments: Vec<_> = call
                .arguments
                .keywords
                .iter()
                .map(|keyword| {
                    (
                        keyword
                            .arg
                            .as_ref()
                            .map(|name| TypedDictKey::Named(name.as_str())),
                        &keyword.value,
                    )
                })
                .collect();
            self.check_typed_dict_unpack(&arguments, &class.name, &definition, call);
            return true;
        }
        let items: Option<Vec<_>> = call
            .arguments
            .keywords
            .iter()
            .map(|keyword| Some((keyword.arg.as_ref()?.to_string(), &keyword.value)))
            .collect();
        if let Some(items) = items {
            self.check_typed_dict_items(&class.name, &definition, &items, call);
        }
        true
    }

    fn typed_dict_index(
        &mut self,
        subscript: &ExprSubscript,
        store: bool,
    ) -> Option<(Definition, Vec<String>)> {
        let ty = self.infer(&subscript.value)?;
        let (class, definition) = self.context.typed_dict(ty)?;
        let Some(keys) = self
            .context
            .typed_dict_keys(&subscript.slice, &self.environment())
        else {
            self.emit(
                "literal-required",
                format!(
                    "TypedDict key must be a string literal; expected one of ({})",
                    names(
                        &definition
                            .items
                            .iter()
                            .map(|item| item.name.clone())
                            .collect::<Vec<_>>()
                    )
                ),
                subscript.slice.as_ref(),
            );
            return Some((definition, Vec::new()));
        };
        for key in &keys {
            if !definition.items.iter().any(|item| &item.name == key) {
                self.emit(
                    if store {
                        "typeddict-unknown-key"
                    } else {
                        "typeddict-item"
                    },
                    format!("TypedDict \"{}\" has no key \"{key}\"", class.name),
                    subscript.slice.as_ref(),
                );
                return Some((definition, Vec::new()));
            }
        }
        Some((definition, keys))
    }

    pub(super) fn check_typed_dict_index(&mut self, subscript: &ExprSubscript) {
        if subscript.ctx.is_store() {
            return;
        }
        if let Some((definition, keys)) = self.typed_dict_index(subscript, false) {
            if subscript.ctx.is_del() {
                let ty = self.infer(&subscript.value).unwrap();
                let (class, _) = self.context.typed_dict(ty).unwrap();
                for item in &definition.items {
                    if keys.contains(&item.name) && (item.required || item.readonly) {
                        self.emit(
                            "misc",
                            format!(
                                "Key \"{}\" of TypedDict \"{}\" cannot be deleted",
                                item.name, class.name
                            ),
                            subscript.slice.as_ref(),
                        );
                    }
                }
            }
        }
    }

    pub(super) fn check_typed_dict_assignment(&mut self, target: &Expr, value: &Expr) -> bool {
        let Expr::Subscript(subscript) = target else {
            return false;
        };
        let Some((definition, keys)) = self.typed_dict_index(subscript, true) else {
            return false;
        };
        for field in &definition.items {
            if !keys.contains(&field.name) {
                continue;
            }
            if field.readonly {
                self.emit(
                    "typeddict-readonly-mutated",
                    format!(
                        "ReadOnly TypedDict key \"{}\" TypedDict is mutated",
                        field.name
                    ),
                    target,
                );
            }
            if let Some(actual) = self.infer_expected(value, field.ty) {
                if !self.context.accepts(field.ty, actual) {
                    self.emit(
                        "typeddict-item",
                        format!(
                            "Value of \"{}\" has incompatible type \"{}\"; expected \"{}\"",
                            field.name,
                            self.context.name(actual),
                            self.context.name(field.ty)
                        ),
                        value,
                    );
                }
            }
        }
        true
    }
}
