use super::*;

/// True when the pattern matches any subject (a bare capture/wildcard or an
/// OR whose last alternative is irrefutable, or an AS over one).
pub(super) fn pattern_is_irrefutable(p: &Pattern) -> bool {
    match p {
        Pattern::Wildcard | Pattern::Binding(_) => true,
        Pattern::Or(alts) => alts
            .last()
            .map(|a| pattern_is_irrefutable(&a.node))
            .unwrap_or(false),
        Pattern::As { pattern, .. } => pattern_is_irrefutable(&pattern.node),
        _ => false,
    }
}
