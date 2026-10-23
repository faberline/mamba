use super::*;

pub(super) fn collect_pattern_targets(pattern: &Pattern, assigned: &mut Vec<String>) {
    match pattern {
        Pattern::Binding(name) => {
            if !assigned.iter().any(|candidate| candidate == name) {
                assigned.push(name.clone());
            }
        }
        Pattern::Constructor { fields, .. } => {
            for field in fields {
                if !assigned.iter().any(|candidate| candidate == field) {
                    assigned.push(field.clone());
                }
            }
        }
        Pattern::Or(patterns) | Pattern::Sequence(patterns) => {
            for pattern in patterns {
                collect_pattern_targets(&pattern.node, assigned);
            }
        }
        Pattern::Mapping { pairs, rest } => {
            for (_, pattern) in pairs {
                collect_pattern_targets(&pattern.node, assigned);
            }
            if let Some(rest) = rest {
                if !assigned.iter().any(|candidate| candidate == rest) {
                    assigned.push(rest.clone());
                }
            }
        }
        Pattern::ClassPattern { patterns, .. } => {
            for (_, pattern) in patterns {
                collect_pattern_targets(&pattern.node, assigned);
            }
        }
        Pattern::Star(Some(name)) => {
            if !assigned.iter().any(|candidate| candidate == name) {
                assigned.push(name.clone());
            }
        }
        Pattern::As { pattern, name } => {
            collect_pattern_targets(&pattern.node, assigned);
            if !assigned.iter().any(|candidate| candidate == name) {
                assigned.push(name.clone());
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) | Pattern::Star(None) => {}
    }
}
