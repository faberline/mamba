use super::*;

pub(super) fn match_keys_equal(a: &MatchKey, b: &MatchKey) -> bool {
    match (a, b) {
        // -0.0 == 0.0 under f64 PartialEq, matching CPython 0/-0/False/0.0.
        (MatchKey::Num(x), MatchKey::Num(y)) => x == y,
        (MatchKey::Str(x), MatchKey::Str(y)) => x == y,
        (MatchKey::NoneKey, MatchKey::NoneKey) => true,
        _ => false,
    }
}
