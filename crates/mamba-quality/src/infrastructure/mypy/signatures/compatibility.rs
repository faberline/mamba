//! Callable subtyping compares the complete parent call domain with the child.
use super::*;

pub(in crate::infrastructure::mypy) fn accepts(
    context: &types::Context,
    parent: &Signature,
    child: &Signature,
    ignore_names: bool,
) -> bool {
    if let Some((kind, expected)) = parent.narrowing {
        let Some((actual_kind, actual)) = child.narrowing else {
            return false;
        };
        if actual_kind != kind
            || !context.accepts(expected, actual)
            || (kind == NarrowingKind::TypeIs && !context.accepts(actual, expected))
        {
            return false;
        }
    }
    if !context.accepts(
        parent.returns.unwrap_or(Type::Any),
        child.returns.unwrap_or(Type::Any),
    ) {
        return false;
    }
    parameters_accept(context, parent, child, ignore_names)
}

pub(in crate::infrastructure::mypy) fn parameters_accept(
    context: &types::Context,
    parent: &Signature,
    child: &Signature,
    ignore_names: bool,
) -> bool {
    parameters_match(context, parent, child, ignore_names, false)
}

pub(in crate::infrastructure::mypy) fn parameters_overlap(
    context: &types::Context,
    parent: &Signature,
    child: &Signature,
    ignore_names: bool,
) -> bool {
    parameters_match(context, parent, child, ignore_names, true)
}

fn parameters_match(
    context: &types::Context,
    parent: &Signature,
    child: &Signature,
    ignore_names: bool,
    require_none_overlap: bool,
) -> bool {
    if parent.ellipsis || child.ellipsis {
        return true;
    }
    let positional: Vec<_> = child
        .parameters
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
    let varargs = child
        .parameters
        .iter()
        .position(|parameter| parameter.kind == Kind::VariadicPositional);
    let kwargs = child
        .parameters
        .iter()
        .position(|parameter| parameter.kind == Kind::VariadicKeyword);
    let mut position = 0;
    let mut always_provided = vec![false; child.parameters.len()];
    for parameter in parent.parameters.iter() {
        let index = match parameter.kind {
            Kind::PositionalOnly | Kind::PositionalOrKeyword => {
                let index = positional.get(position).copied().or(varargs);
                position += 1;
                index
            }
            Kind::KeywordOnly => child
                .parameters
                .iter()
                .position(|child| {
                    matches!(child.kind, Kind::KeywordOnly | Kind::PositionalOrKeyword)
                        && child.name == parameter.name
                })
                .or(kwargs),
            Kind::VariadicPositional => varargs,
            Kind::VariadicKeyword => kwargs,
        };
        let Some(index) = index else { return false };
        let target = &child.parameters[index];
        if !ignore_names
            && parameter.kind == Kind::PositionalOrKeyword
            && target.kind != Kind::VariadicPositional
            && (target.kind == Kind::PositionalOnly || target.name != parameter.name)
        {
            return false;
        }
        if !context.accepts(
            target.ty.unwrap_or(Type::Any),
            parameter.ty.unwrap_or(Type::Any),
        ) || require_none_overlap
            && matches!(
                (target.ty, parameter.ty),
                (Some(Type::Object), Some(Type::None)) | (Some(Type::None), Some(Type::Object))
            )
        {
            return false;
        }
        always_provided[index] |= parameter.required;
    }
    child
        .parameters
        .iter()
        .enumerate()
        .all(|(index, parameter)| !parameter.required || always_provided[index])
}
