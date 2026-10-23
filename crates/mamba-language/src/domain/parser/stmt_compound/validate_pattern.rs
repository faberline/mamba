use super::*;

/// Per-pattern walk: records capture names (duplicates are SyntaxErrors),
/// checks mapping-key duplicates, class-pattern keyword repeats, and OR
/// alternation rules (same bound names per branch; only the final
/// alternative may be irrefutable).
pub(super) fn validate_pattern(p: &Pattern, names: &mut Vec<String>) -> Result<(), String> {
    match p {
        Pattern::Wildcard | Pattern::Literal(_) => Ok(()),
        Pattern::Binding(n) => {
            if names.iter().any(|x| x == n) {
                return Err(format!("multiple assignments to name '{n}' in pattern"));
            }
            names.push(n.clone());
            Ok(())
        }
        Pattern::Star(name) => {
            if let Some(n) = name {
                if n != "_" {
                    if names.iter().any(|x| x == n) {
                        return Err(format!("multiple assignments to name '{n}' in pattern"));
                    }
                    names.push(n.clone());
                }
            }
            Ok(())
        }
        Pattern::As { pattern, name } => {
            if name == "_" {
                return Err("cannot use '_' as a target".to_string());
            }
            validate_pattern(&pattern.node, names)?;
            if names.iter().any(|x| x == name) {
                return Err(format!("multiple assignments to name '{name}' in pattern"));
            }
            names.push(name.clone());
            Ok(())
        }
        Pattern::Sequence(items) => {
            for it in items {
                validate_pattern(&it.node, names)?;
            }
            Ok(())
        }
        Pattern::Mapping { pairs, rest } => {
            let mut keys: Vec<MatchKey> = Vec::new();
            for (k, sub) in pairs {
                if let Some(key) = match_key_of(&k.node) {
                    if keys.iter().any(|seen| match_keys_equal(seen, &key)) {
                        return Err("mapping pattern checks duplicate key".to_string());
                    }
                    keys.push(key);
                }
                validate_pattern(&sub.node, names)?;
            }
            if let Some(r) = rest {
                if r != "_" {
                    if names.iter().any(|x| x == r) {
                        return Err(format!("multiple assignments to name '{r}' in pattern"));
                    }
                    names.push(r.clone());
                }
            }
            Ok(())
        }
        Pattern::ClassPattern { patterns, .. } => {
            let mut kw_seen: Vec<&String> = Vec::new();
            for (kw, sub) in patterns {
                if let Some(name) = kw {
                    if kw_seen.contains(&name) {
                        return Err(format!("attribute name repeated in class pattern: {name}"));
                    }
                    kw_seen.push(name);
                }
                validate_pattern(&sub.node, names)?;
            }
            Ok(())
        }
        Pattern::Constructor { fields, .. } => {
            for n in fields {
                if n != "_" {
                    if names.iter().any(|x| x == n) {
                        return Err(format!("multiple assignments to name '{n}' in pattern"));
                    }
                    names.push(n.clone());
                }
            }
            Ok(())
        }
        Pattern::Or(alts) => {
            // Only the final alternative may be irrefutable.
            for a in alts.iter().take(alts.len().saturating_sub(1)) {
                if pattern_is_irrefutable(&a.node) {
                    return Err("wildcard makes remaining patterns unreachable".to_string());
                }
            }
            // Every alternative must bind the same set of names; the whole
            // OR contributes that set once to the enclosing pattern.
            let mut first_set: Option<Vec<String>> = None;
            for a in alts {
                let mut branch = Vec::new();
                validate_pattern(&a.node, &mut branch)?;
                let mut sorted = branch.clone();
                sorted.sort();
                match &first_set {
                    None => first_set = Some(sorted),
                    Some(expected) => {
                        if *expected != sorted {
                            return Err("alternative patterns bind different names".to_string());
                        }
                    }
                }
            }
            if let Some(set) = first_set {
                for n in set {
                    if names.contains(&n) {
                        return Err(format!("multiple assignments to name '{n}' in pattern"));
                    }
                    names.push(n);
                }
            }
            Ok(())
        }
    }
}
