//! Rewritten dictionary calls retain dictionary entry diagnostics and positions.
use super::*;
use quality_syntax::python_ast::ExprDict;
use types::{Container, Node, Type};

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_dictionary_items(
        &mut self,
        dictionary: &ExprDict,
        expected_key: Type,
        expected_value: Type,
    ) {
        for (index, item) in dictionary.items.iter().enumerate() {
            if let Some(key) = &item.key {
                if let (Some(key), Some(value)) = (
                    self.infer_expected(key, expected_key),
                    self.infer_expected(&item.value, expected_value),
                ) {
                    if !self.context.accepts(expected_key, key)
                        || !self.context.accepts(expected_value, value)
                    {
                        self.emit("dict-item", format!("Dict entry {index} has incompatible type \"{}\": \"{}\"; expected \"{}\": \"{}\"", self.context.name(key), self.context.name(value), self.context.name(expected_key), self.context.name(self.context.expanded_alias_instance(expected_value))), item);
                    }
                }
                continue;
            }
            if matches!(item.value, Expr::Dict(_)) {
                let expected = self.context.intern(Node::Container(
                    Container::Dict,
                    vec![expected_key, expected_value],
                ));
                self.infer_expected(&item.value, expected);
                continue;
            }
            let Some(actual) = self.infer(&item.value) else {
                continue;
            };
            let Some(protocol) = self.context.builtin_value_class_instance(
                "_typeshed.SupportsKeysAndGetItem",
                self.python_target,
            ) else {
                continue;
            };
            let Some(owner) = self.context.class_id(protocol) else {
                continue;
            };
            let expected =
                self.context
                    .applied_class(owner, vec![expected_key, expected_value], false);
            if !self.context.accepts(expected, actual) {
                self.emit(
                    "dict-item",
                    format!(
                        "Unpacked dict entry {index} has incompatible type \"{}\"; expected \"{}\"",
                        self.context.name(actual),
                        self.context.name(expected),
                    ),
                    &item.value,
                );
            }
        }
    }

    pub(super) fn check_dictionary_expression_call(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
    ) -> bool {
        let Some(dictionary) = self.scopes.dictionary_expression(call, signature) else {
            return false;
        };
        let expected = self
            .expected_value
            .filter(|(range, _)| *range == call.range())
            .map(|(_, ty)| ty)
            .or_else(|| {
                self.expected_calls
                    .get(&(call.start(), call.end()))
                    .copied()
            });
        if let Some(expected) = expected {
            if let Expr::Dict(dictionary) = &dictionary {
                for (index, item) in dictionary.items.iter().enumerate() {
                    if item.key.is_none() {
                        if matches!(item.value, Expr::Dict(_)) {
                            self.infer_expected(&item.value, expected);
                            continue;
                        }
                        if let (Some((key, value)), Some(actual)) = (
                            self.context.mapping_types(expected),
                            self.infer(&item.value),
                        ) {
                            let expected = self
                                .context
                                .intern(Node::Container(Container::Dict, vec![key, value]));
                            if !self.context.accepts(expected, actual) {
                                self.emit("dict-item", format!("Unpacked dict entry {index} has incompatible type \"{}\"; expected \"{}\"", self.context.name(actual), self.context.name(expected)), &item.value);
                            }
                        }
                        return true;
                    }
                }
            }
            self.infer_expected(&dictionary, expected);
        }
        true
    }
}
