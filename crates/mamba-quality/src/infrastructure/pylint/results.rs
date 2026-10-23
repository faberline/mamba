//! A function result retains argument values without executing the function.
use super::{
    callables::{Kind, Signature},
    resolution::Context,
    values::Value,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::ExprCall;
use std::collections::{BTreeMap, BTreeSet};

impl Checker<'_> {
    pub(super) fn call_result(
        &self,
        signature: &Signature,
        call: &ExprCall,
        caller: &Context,
        origin: &Context,
    ) -> Value {
        let mut values: BTreeMap<_, _> = signature
            .parameters
            .iter()
            .map(|parameter| {
                (
                    parameter.name.clone(),
                    parameter.default.clone().unwrap_or(Value::Unknown),
                )
            })
            .collect();
        let positional = signature
            .parameters
            .iter()
            .filter(|parameter| parameter.kind != Kind::KeywordOnly);
        for (parameter, argument) in positional.zip(&call.arguments.args) {
            values.insert(
                parameter.name.clone(),
                self.expand_value(
                    Value::from_expression(argument),
                    caller,
                    &mut BTreeSet::new(),
                ),
            );
        }
        for keyword in &call.arguments.keywords {
            if let Some(name) = &keyword.arg {
                values.insert(
                    name.to_string(),
                    self.expand_value(
                        Value::from_expression(&keyword.value),
                        caller,
                        &mut BTreeSet::new(),
                    ),
                );
            }
        }
        self.result_value(signature.result.clone(), &values, origin)
    }

    pub(super) fn result_value(
        &self,
        value: Value,
        arguments: &BTreeMap<String, Value>,
        origin: &Context,
    ) -> Value {
        match value {
            Value::Reference(names) if names.len() == 1 && arguments.contains_key(&names[0]) => {
                arguments[&names[0]].clone()
            }
            Value::Sequence(items, kind) => Value::Sequence(
                items
                    .into_iter()
                    .map(|item| self.result_value(item, arguments, origin))
                    .collect(),
                kind,
            ),
            Value::Mapping(items) => Value::Mapping(
                items
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            key.map(|key| self.result_value(key, arguments, origin)),
                            self.result_value(value, arguments, origin),
                        )
                    })
                    .collect(),
            ),
            Value::Spread(value) => {
                Value::Spread(Box::new(self.result_value(*value, arguments, origin)))
            }
            value => self.expand_value(value, origin, &mut BTreeSet::new()),
        }
    }
}
