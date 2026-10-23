//! Candidate binding separates argument shape from full and erased type checks.
use super::*;
use crate::infrastructure::mypy::signatures::Kind;
mod mapping;

pub(super) fn pairs(
    context: &types::Context,
    call: &ExprCall,
    signature: &Signature,
    values: &[Type],
    literal_context: bool,
) -> Option<Vec<(Type, Type)>> {
    let parameters = &signature.parameters;
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
    let varargs = parameters
        .iter()
        .position(|parameter| parameter.kind == Kind::VariadicPositional);
    let kwargs = parameters
        .iter()
        .position(|parameter| parameter.kind == Kind::VariadicKeyword);
    let mut provided = vec![false; parameters.len()];
    let mut pairs = Vec::new();
    for (index, &actual) in values.iter().enumerate() {
        if index >= call.arguments.args.len()
            && call.arguments.keywords[index - call.arguments.args.len()]
                .arg
                .is_none()
        {
            mapping::mapping_pairs(context, signature, actual, &mut provided, &mut pairs)?;
            continue;
        }
        let parameter = if index < call.arguments.args.len() {
            positional.get(index).copied().or(varargs)
        } else {
            let name = call.arguments.keywords[index - call.arguments.args.len()]
                .arg
                .as_ref()?
                .as_str();
            parameters
                .iter()
                .position(|parameter| {
                    parameter.name == name
                        && matches!(
                            parameter.kind,
                            Kind::PositionalOrKeyword | Kind::KeywordOnly
                        )
                })
                .or(kwargs)
        }?;
        let target = &parameters[parameter];
        if provided[parameter]
            && !matches!(
                target.kind,
                Kind::VariadicPositional | Kind::VariadicKeyword
            )
        {
            return None;
        }
        provided[parameter] = true;
        let expected = target.ty.unwrap_or(Type::Any);
        let value = if index < call.arguments.args.len() {
            &call.arguments.args[index]
        } else {
            &call.arguments.keywords[index - call.arguments.args.len()].value
        };
        let actual = if literal_context && context.contains_literal(expected) {
            context.literal_expression(value).unwrap_or(actual)
        } else {
            actual
        };
        pairs.push((expected, actual));
    }
    (!parameters
        .iter()
        .enumerate()
        .any(|(index, parameter)| parameter.required && !provided[index]))
    .then_some(pairs)
}

pub(super) fn erased_accepts(context: &types::Context, expected: Type, actual: Type) -> bool {
    if context.accepts(expected, actual) {
        return true;
    }
    if let Some(id) = context
        .class_id(expected)
        .filter(|&id| context.class(id).protocol)
    {
        if context.accepts(
            context.applied_class(id, vec![Type::Any; context.class(id).generic.len()], false),
            actual,
        ) {
            return true;
        }
    }
    let expected_node = context.node(expected);
    if let Some((expected_id, actual_id)) = context.class_id(expected).zip(context.class_id(actual))
    {
        if !context.is_class_value(expected)
            && !context.is_class_value(actual)
            && context.nominal_accepts(expected_id, actual_id)
        {
            return true;
        }
    }
    let actual_node = context.node(actual);
    match (expected_node.as_deref(), actual_node.as_deref()) {
        (Some(types::Node::Container(types::Container::Iterable, _)), _) => {
            context.iterable_item(actual).is_some()
        }
        (Some(types::Node::Container(types::Container::Sequence, _)), _) => {
            context.sequence_item(actual).is_some()
        }
        (_, Some(types::Node::Union(items))) => items
            .iter()
            .any(|&actual| erased_accepts(context, expected, actual)),
        (Some(types::Node::Union(items)), _) => items
            .iter()
            .any(|&expected| erased_accepts(context, expected, actual)),
        (Some(types::Node::Variable(variable)), _) => {
            erased_accepts(context, variable.bound, actual)
        }
        (Some(types::Node::Container(types::Container::TypeObject, items)), _) => {
            context.is_class_value(actual)
                && context
                    .instance_type(actual)
                    .is_some_and(|item| erased_accepts(context, items[0], item))
        }
        (Some(types::Node::Container(kind, _)), Some(types::Node::Container(other, _))) => {
            kind == other
                || (*kind == types::Container::Sequence && *other == types::Container::List)
        }
        (
            Some(types::Node::Tuple(_) | types::Node::VariadicTuple(_)),
            Some(types::Node::Tuple(_) | types::Node::VariadicTuple(_)),
        ) => true,
        (Some(types::Node::Callable(_)), _) => context.callable_signature(actual).is_some(),
        _ => false,
    }
}
