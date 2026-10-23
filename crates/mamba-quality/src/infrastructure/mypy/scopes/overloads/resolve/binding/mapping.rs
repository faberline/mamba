//! Double-star mappings can provide named parameters, but never positional-only ones.
use super::*;

pub(super) fn mapping_pairs(
    context: &types::Context,
    signature: &Signature,
    actual: Type,
    provided: &mut [bool],
    pairs: &mut Vec<(Type, Type)>,
) -> Option<()> {
    if actual.is_any() {
        for (index, parameter) in signature.parameters.iter().enumerate() {
            if matches!(
                parameter.kind,
                Kind::PositionalOrKeyword | Kind::KeywordOnly | Kind::VariadicKeyword
            ) {
                provided[index] = true;
                pairs.push((parameter.ty.unwrap_or(Type::Any), Type::Any));
            }
        }
        return Some(());
    }
    let (key, value) = context.mapping_types(actual)?;
    if !context.accepts(Type::Str, key) {
        return None;
    }
    for (index, parameter) in signature.parameters.iter().enumerate() {
        if !provided[index]
            && matches!(
                parameter.kind,
                Kind::PositionalOrKeyword | Kind::KeywordOnly | Kind::VariadicKeyword
            )
        {
            provided[index] = true;
            pairs.push((parameter.ty.unwrap_or(Type::Any), value));
        }
    }
    Some(())
}
