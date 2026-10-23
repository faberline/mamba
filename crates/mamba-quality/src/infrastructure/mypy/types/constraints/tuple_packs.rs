//! Tuple constraints bind one variadic middle while preserving fixed ends.
use super::*;

impl Context {
    pub(super) fn callable_tuple_constraints(
        &self,
        expected: &super::super::super::signatures::Signature,
        actual: &super::super::super::signatures::Signature,
        variance: Variance,
        contextual: bool,
        bounds: &mut BTreeMap<Type, Bounds>,
    ) -> bool {
        use super::super::super::signatures::Kind;
        if expected.ellipsis
            || actual.ellipsis
            || !expected.parameters.iter().any(|parameter| {
                parameter
                    .ty
                    .and_then(|ty| self.unpack_target(ty))
                    .is_some_and(|ty| self.is_tuple_variable(ty))
            })
        {
            return false;
        }
        let domain = |signature: &super::super::super::signatures::Signature| {
            signature
                .parameters
                .iter()
                .filter(|parameter| parameter.kind != Kind::VariadicKeyword)
                .map(|parameter| {
                    let ty = parameter.ty.unwrap_or(Type::Any);
                    if parameter.kind == Kind::VariadicPositional
                        && self.unpack_target(ty).is_none()
                    {
                        self.intern(Node::Unpack(self.intern(Node::VariadicTuple(ty))))
                    } else {
                        ty
                    }
                })
                .collect::<Vec<_>>()
        };
        self.tuple_pack_constraints(
            &domain(expected),
            &domain(actual),
            variance.reverse(),
            contextual,
            bounds,
        )
    }

    pub(super) fn tuple_pack_constraints(
        &self,
        expected: &[Type],
        actual: &[Type],
        variance: Variance,
        contextual: bool,
        bounds: &mut BTreeMap<Type, Bounds>,
    ) -> bool {
        let Some((prefix, variable)) = self.tuple_pack_position(expected) else {
            return expected.len() != actual.len()
                && !actual.iter().any(|&ty| self.unpack_target(ty).is_some());
        };
        let suffix = expected.len() - prefix - 1;
        let start = prefix.min(actual.len());
        let end = actual.len().saturating_sub(suffix).max(start);
        self.constraints(
            variable,
            self.normalize_tuple(actual[start..end].to_vec()),
            variance,
            contextual,
            bounds,
        );
        for (&expected, &actual) in expected[..prefix].iter().zip(actual) {
            self.constraints(expected, actual, variance, contextual, bounds);
        }
        for (&expected, &actual) in expected[prefix + 1..].iter().rev().zip(actual.iter().rev()) {
            self.constraints(expected, actual, variance, contextual, bounds);
        }
        true
    }
}
