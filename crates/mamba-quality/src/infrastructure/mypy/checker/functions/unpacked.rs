//! An expanded length argument retains the original starred container diagnostic.
use super::*;

impl Checker<'_> {
    pub(super) fn check_enumerate_mapping(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
    ) -> bool {
        if signature.definition_module.as_deref() != Some("builtins")
            || signature.name.as_deref() != Some("enumerate")
        {
            return false;
        }
        let keywords: Vec<_> = call
            .arguments
            .keywords
            .iter()
            .filter(|keyword| keyword.arg.is_none())
            .collect();
        if keywords.is_empty() {
            return false;
        }
        let solution = self.scopes.call_solution(
            call,
            signature,
            &self.environment(),
            self.expected_calls
                .get(&(call.start(), call.end()))
                .copied(),
        );
        for keyword in keywords {
            let Some(actual) = self.infer(&keyword.value) else {
                continue;
            };
            let Some((_, value)) = self.context.mapping_types(actual) else {
                continue;
            };
            for parameter in signature.parameters.iter().filter(|parameter| {
                matches!(
                    parameter.kind,
                    Kind::PositionalOrKeyword | Kind::KeywordOnly
                )
            }) {
                let Some(expected) = parameter
                    .ty
                    .map(|ty| self.context.substitute(ty, &solution.types))
                else {
                    continue;
                };
                if !self.context.accepts(expected, value) {
                    self.emit("arg-type", format!("Argument 1 to \"enumerate\" has incompatible type \"**{}\"; expected \"{}\"", self.context.name(actual), self.context.name(expected)), &keyword.value);
                }
            }
        }
        true
    }

    // Homogeneous **dict values can reach each still-unfilled keyword formal.
    // TypedDict and unknown mappings need their own key-sensitive binding model.
    pub(super) fn check_keyword_mapping_arguments(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
    ) {
        if signature.ellipsis
            || !signature.generic.is_empty()
            || self.context.callable_needs_unpack(signature)
        {
            return;
        }
        let parameters = &signature.parameters;
        // Primitive formals are independent of opaque peers. New mixed-shape
        // checks require one flat signature and never inspect a Node formal.
        let has_node = parameters
            .iter()
            .any(|parameter| matches!(parameter.ty, Some(types::Type::Node(_))));
        if has_node
            && (!signature.overloads.is_empty()
                || !parameters.iter().any(|parameter| {
                    parameter
                        .ty
                        .is_some_and(|ty| !matches!(ty, types::Type::Node(_)))
                }))
        {
            return;
        }
        let positional: Vec<_> = parameters
            .iter()
            .enumerate()
            .filter(|(_, parameter)| {
                matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword
                )
            })
            .map(|(index, _)| index)
            .collect();
        let mut provided = vec![false; parameters.len()];
        for (index, _) in call.arguments.args.iter().enumerate() {
            if let Some(&parameter) = positional.get(index) {
                provided[parameter] = true;
            } else if !parameters
                .iter()
                .any(|parameter| parameter.kind == Kind::VariadicPositional)
            {
                return;
            }
        }
        let mut mappings = Vec::new();
        for (index, keyword) in call.arguments.keywords.iter().enumerate() {
            if let Some(name) = &keyword.arg {
                let parameter = parameters.iter().position(|parameter| {
                    parameter.name == name.as_str()
                        && matches!(
                            parameter.kind,
                            Kind::PositionalOrKeyword | Kind::KeywordOnly
                        )
                });
                if let Some(parameter) = parameter {
                    if provided[parameter] {
                        return;
                    }
                    provided[parameter] = true;
                } else if !parameters
                    .iter()
                    .any(|parameter| parameter.kind == Kind::VariadicKeyword)
                {
                    return;
                }
                continue;
            }
            let Some(actual) = self.infer(&keyword.value) else {
                return;
            };
            let Some(node) = self.context.node(actual) else {
                return;
            };
            let types::Node::Container(types::Container::Dict, items) = node.as_ref() else {
                return;
            };
            let [types::Type::Str, item] = items.as_slice() else {
                return;
            };
            mappings.push((index, &keyword.value, actual, *item));
        }
        if mappings.is_empty() {
            return;
        }
        let target = signature
            .display_target()
            .map(|name| format!(" to {name}"))
            .unwrap_or_default();
        // Mypy checks formals in declaration order, then their actual arguments.
        for (index, parameter) in parameters.iter().enumerate() {
            if !matches!(
                parameter.kind,
                Kind::PositionalOrKeyword | Kind::KeywordOnly | Kind::VariadicKeyword
            ) || (parameter.name.is_empty() && parameter.kind != Kind::VariadicKeyword)
                || (provided[index] && parameter.kind != Kind::VariadicKeyword)
            {
                continue;
            }
            let Some(expected) = parameter
                .ty
                .filter(|ty| !matches!(ty, types::Type::Node(_)))
            else {
                continue;
            };
            for &(keyword_index, value, actual, item) in &mappings {
                if !self.context.accepts(expected, item) {
                    self.emit(
                        "arg-type",
                        format!(
                            "Argument {}{target} has incompatible type \"**{}\"; expected \"{}\"",
                            call.arguments.args.len() + keyword_index + 1,
                            self.context.name(actual),
                            self.context.assignment_type_name(expected)
                        ),
                        value,
                    );
                }
            }
        }
    }

    pub(super) fn unpacked_length_argument<'a>(
        &self,
        call: &'a ExprCall,
        signature: &Signature,
        argument: &Expr,
    ) -> Option<(&'a Expr, types::Type)> {
        if signature.definition_module.as_deref() != Some("builtins")
            || !matches!(signature.name.as_deref(), Some("len" | "enumerate"))
        {
            return None;
        }
        let value = call.arguments.args.iter().find_map(|value| {
            let Expr::Starred(starred) = value else {
                return None;
            };
            (starred.value.start() <= argument.start() && starred.value.end() >= argument.end())
                .then_some(starred.value.as_ref())
        })?;
        Some((value, self.infer(value)?))
    }
}
