//! Overload keyword errors retain the original argument type confidence rule.
use super::*;
use signatures::Kind;
use std::collections::BTreeSet;

impl Checker<'_> {
    pub(super) fn check_overload_keywords(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
    ) -> bool {
        if signature.overloads.iter().any(|variant| {
            variant
                .parameters
                .iter()
                .any(|parameter| parameter.kind == Kind::VariadicKeyword)
        }) {
            return false;
        }
        let names: BTreeSet<_> = signature
            .overloads
            .iter()
            .flat_map(|variant| variant.parameters.iter())
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    Kind::PositionalOrKeyword | Kind::KeywordOnly
                )
            })
            .map(|parameter| parameter.name.as_str())
            .collect();
        if names.is_empty() {
            return false;
        }
        let mut unexpected = 0;
        let mut confident = true;
        let mut unsuggested = Vec::new();
        let target = signature
            .display_target()
            .map(|target| format!(" for overloaded function {target}"))
            .unwrap_or_default();
        for keyword in &call.arguments.keywords {
            let Some(name) = &keyword.arg else {
                continue;
            };
            if names.contains(name.as_str()) {
                continue;
            }
            unexpected += 1;
            let actual = self
                .context
                .literal_expression(&keyword.value)
                .or_else(|| self.infer(&keyword.value))
                .unwrap_or(types::Type::Any);
            let matching: BTreeSet<_> = signature
                .overloads
                .iter()
                .flat_map(|variant| variant.parameters.iter())
                .filter(|parameter| {
                    matches!(
                        parameter.kind,
                        Kind::PositionalOrKeyword | Kind::KeywordOnly
                    ) && self
                        .context
                        .accepts(parameter.ty.unwrap_or(types::Type::Any), actual)
                })
                .map(|parameter| parameter.name.as_str())
                .collect();
            confident &= !matching.is_empty();
            let suggestions = signatures::suggestions::best_matches(name.as_str(), &matching);
            let suggestions = if suggestions.is_empty() {
                let remaining = names.difference(&matching).copied().collect();
                signatures::suggestions::best_matches(name.as_str(), &remaining)
            } else {
                suggestions
            };
            if suggestions.is_empty() {
                unsuggested.push(name.to_string());
            } else {
                self.emit(
                    "call-overload",
                    format!(
                        "Unexpected keyword argument \"{name}\"{target}; did you mean {}?",
                        quoted_names(&suggestions, "or")
                    ),
                    call,
                );
            }
        }
        if !unsuggested.is_empty() {
            let (plural, quoted) = if unsuggested.len() == 1 {
                ("argument", quoted_names(&unsuggested, ""))
            } else {
                (
                    "arguments",
                    unsuggested
                        .iter()
                        .map(|name| format!("\"{name}\""))
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            };
            self.emit(
                "call-overload",
                format!("Unexpected keyword {plural} {quoted}{target}"),
                call,
            );
        }
        unexpected > 0
            && confident
            && unexpected == call.arguments.args.len() + call.arguments.keywords.len()
    }
}

fn quoted_names(names: &[String], conjunction: &str) -> String {
    let quoted = names
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>();
    match quoted.as_slice() {
        [one] => one.clone(),
        [one, two] => format!("{one} {conjunction} {two}"),
        [head @ .., last] => format!("{}, {conjunction} {last}", head.join(", ")),
        [] => String::new(),
    }
}
