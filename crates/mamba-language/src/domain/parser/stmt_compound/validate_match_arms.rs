use super::*;

/// Whole-statement validation: per-arm pattern rules plus the cross-arm
/// rule that an unguarded irrefutable case must be the last one.
pub(super) fn validate_match_arms(arms: &[MatchArm]) -> Result<(), String> {
    for (idx, arm) in arms.iter().enumerate() {
        let mut names = Vec::new();
        validate_pattern(&arm.pattern.node, &mut names)?;
        if idx + 1 < arms.len() && arm.guard.is_none() && pattern_is_irrefutable(&arm.pattern.node)
        {
            return Err("wildcard makes remaining patterns unreachable".to_string());
        }
    }
    Ok(())
}
