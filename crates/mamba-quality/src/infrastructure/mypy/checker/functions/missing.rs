//! Missing call arguments retain positional recovery and keyword-only domains.
use super::*;

impl Checker<'_> {
    pub(super) fn check_missing_call_arguments(
        &mut self,
        call: &ExprCall,
        signature: &Signature,
        provided: &[bool],
        invalid: bool,
        extra_positionals: bool,
    ) {
        let parameters = &signature.parameters;
        let positional_count = parameters
            .iter()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword,
                )
            })
            .count();
        let consumed: std::collections::BTreeSet<_> = parameters
            .iter()
            .enumerate()
            .filter(|(_, parameter)| parameter.kind == Kind::KeywordOnly)
            .take(if extra_positionals {
                call.arguments.args.len().saturating_sub(positional_count)
            } else {
                0
            })
            .map(|(index, _)| index)
            .collect();
        let missing: Vec<_> = parameters
            .iter()
            .enumerate()
            .filter(|(index, parameter)| {
                !invalid && parameter.required && !provided[*index] && !consumed.contains(index)
            })
            .map(|(_, parameter)| parameter)
            .collect();
        let positional: Vec<_> = missing
            .iter()
            .filter(|parameter| parameter.kind != Kind::KeywordOnly)
            .collect();
        if !positional.is_empty() {
            let required = parameters
                .iter()
                .filter(|parameter| {
                    parameter.required
                        && matches!(
                            parameter.kind,
                            Kind::PositionalOnly | Kind::PositionalOrKeyword
                        )
                })
                .count();
            let names = parameters
                .iter()
                .filter(|parameter| {
                    matches!(
                        parameter.kind,
                        Kind::PositionalOnly | Kind::PositionalOrKeyword
                    )
                })
                .take(required)
                .skip(call.arguments.args.len())
                .filter(|parameter| {
                    !call.arguments.keywords.iter().any(|keyword| {
                        keyword
                            .arg
                            .as_ref()
                            .is_some_and(|name| name.as_str() == parameter.name)
                    })
                })
                .map(|parameter| format!("\"{}\"", parameter.name))
                .collect::<Vec<_>>();
            let plural = if names.len() == 1 {
                "argument"
            } else {
                "arguments"
            };
            let message = if signature.name.is_some() {
                if !names.is_empty()
                    && positional
                        .iter()
                        .all(|parameter| parameter.kind != Kind::PositionalOnly)
                {
                    format!(
                        "Missing positional {plural} {names} in call to {}",
                        signature.display_target().unwrap(),
                        names = names.join(", "),
                    )
                } else {
                    format!("Too few arguments{}", signature.for_function())
                }
            } else {
                "Too few arguments".into()
            };
            self.emit_with_hint(
                "call-arg",
                message,
                call,
                self.callable_field_hint(call, signature),
            );
        }
        for parameter in missing
            .iter()
            .filter(|parameter| parameter.kind == Kind::KeywordOnly)
        {
            self.emit_with_hint(
                "call-arg",
                format!(
                    "Missing named argument \"{}\"{}",
                    parameter.name,
                    signature.for_function()
                ),
                call,
                signature.definition_note(),
            );
        }
    }
}
