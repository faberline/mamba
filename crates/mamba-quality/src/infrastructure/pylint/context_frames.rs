//! Call frames bind source nodes without executing functions or moving AST ownership.
use super::{
    callables::{Kind, Signature},
    resolution::Context,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone)]
pub(super) struct Bound {
    pub expression: Expr,
    pub origin: Context,
    pub frame: Option<Rc<Frame>>,
}
pub(super) struct Frame {
    pub arguments: BTreeMap<String, Bound>,
    pub locals: BTreeMap<String, Expr>,
    pub invalid: bool,
    pub functions: BTreeMap<String, Rc<Signature>>,
}
impl Checker<'_> {
    pub(super) fn context_frame(
        &self,
        signature: &Signature,
        call: &ExprCall,
        caller: &Context,
        origin: &Context,
        parent: Option<Rc<Frame>>,
    ) -> Rc<Frame> {
        let mut arguments = BTreeMap::new();
        if let Some(parameters) = &signature.parameter_ast {
            for parameter in parameters.iter() {
                if let Some(default) = parameter.default() {
                    arguments.insert(
                        parameter.name().to_string(),
                        Bound {
                            expression: default.clone(),
                            origin: origin.clone(),
                            frame: None,
                        },
                    );
                }
            }
        }
        let mut positional = Vec::new();
        for value in &call.arguments.args {
            if let Expr::Starred(value) = value {
                if let Some(unpacked) = self.unpacked_value(&value.value, caller.clone()) {
                    if let Some(elements) = unpacked.elements {
                        positional.extend(elements.into_iter().map(|expression| Bound {
                            expression,
                            origin: unpacked.origin.clone(),
                            frame: parent.clone(),
                        }));
                    }
                }
            } else {
                positional.push(Bound {
                    expression: value.clone(),
                    origin: caller.clone(),
                    frame: parent.clone(),
                });
            }
        }
        let invalid = !signature.variadic
            && positional.len()
                > signature
                    .parameters
                    .iter()
                    .filter(|parameter| parameter.kind != Kind::KeywordOnly)
                    .count();
        for (parameter, value) in signature
            .parameters
            .iter()
            .filter(|parameter| parameter.kind != Kind::KeywordOnly)
            .zip(positional)
        {
            arguments.insert(parameter.name.clone(), value);
        }
        for keyword in &call.arguments.keywords {
            if let Some(name) = &keyword.arg {
                arguments.insert(
                    name.to_string(),
                    Bound {
                        expression: keyword.value.clone(),
                        origin: caller.clone(),
                        frame: parent.clone(),
                    },
                );
            } else if let Some(unpacked) = self.unpacked_value(&keyword.value, caller.clone()) {
                if let Some(entries) = unpacked.entries {
                    for (key, expression) in entries {
                        if let Expr::StringLiteral(key) = key {
                            arguments.insert(
                                key.value.to_str().to_string(),
                                Bound {
                                    expression,
                                    origin: unpacked.origin.clone(),
                                    frame: parent.clone(),
                                },
                            );
                        }
                    }
                }
            }
        }
        Rc::new(Frame {
            arguments,
            locals: signature.result_bindings.clone(),
            invalid,
            functions: self
                .source_function(signature, origin)
                .map(|function| {
                    super::callables::collect(&function.body, self.module_name, false)
                        .into_iter()
                        .filter_map(|(name, target)| {
                            if let super::callables::Target::Function(signature) = target {
                                Some((name, signature))
                            } else {
                                None
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}
