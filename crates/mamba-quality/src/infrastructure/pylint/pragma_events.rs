//! Pragma problems precede all raw, token, and AST checker diagnostics.
use super::{
    directives::Directive, message_registry, pragma_information, pragma_parser::Item, Checker,
};

pub(super) fn entries(template: Directive, items: Vec<Item>) -> Vec<Directive> {
    let mut entries = Vec::new();
    for item in items {
        match item {
            Item::Failure { invalid, token } => {
                let mut entry = template.clone();
                entry.problem = Some(if invalid {
                    (
                        "I0010",
                        "bad-inline-option",
                        format!("Unable to consider inline option '{token}'"),
                    )
                } else {
                    (
                        "E0011",
                        "unrecognized-inline-option",
                        format!("Unrecognized file option '{token}'"),
                    )
                });
                entries.push(entry);
            }
            Item::Command { action, targets } => {
                if matches!(action.as_str(), "disable-msg" | "enable-msg") {
                    entries.push(pragma_information::deprecated(
                        &template,
                        &action,
                        action.trim_end_matches("-msg"),
                    ));
                }
                if matches!(action.as_str(), "skip-file" | "disable-all") {
                    if action == "disable-all" {
                        entries.push(pragma_information::deprecated(
                            &template,
                            &action,
                            "skip-file",
                        ));
                    }
                    entries.extend(pragma_information::ignored(&template));
                    break;
                }
                for target in targets {
                    if action == "disable" && target == "all" {
                        entries.push(pragma_information::deprecated(
                            &template,
                            "disable=all",
                            "skip-file",
                        ));
                        entries.extend(pragma_information::ignored(&template));
                        return entries;
                    }
                    match message_registry::resolve(&target, action.starts_with("enable")) {
                        Ok(codes) => {
                            let normalized_action = match action.as_str() {
                                "disable-msg" => "disable",
                                "enable-msg" => "enable",
                                value => value,
                            };
                            for code in codes {
                                let mut entry = template.clone();
                                entry.text = format!("{normalized_action}={code}");
                                entries.push(entry);
                                if let Some(notice) =
                                    pragma_information::local(&template, &action, &code)
                                {
                                    entries.push(notice);
                                }
                            }
                        }
                        Err(error) => {
                            let mut entry = template.clone();
                            entry.problem = Some(match error {
                                message_registry::Error::Unknown => ("W0012", "unknown-option-value", format!("Unknown option value for '{action}', expected a valid pylint message and got '{target}'")),
                                message_registry::Error::Retired(reason) => ("R0022", "useless-option-value", format!("Useless option value for '{action}', {reason}")),
                            });
                            entries.push(entry);
                        }
                    }
                }
            }
        }
    }
    entries
}

impl Checker<'_> {
    pub(super) fn check_pragma_events(&mut self) -> bool {
        let mut processed = Vec::new();
        for entry in self.directives.clone() {
            if entry.skip {
                return true;
            }
            if let Some((rule, symbol, message)) = &entry.problem {
                if super::directives::active(&processed, rule, entry.report_line) {
                    let mut diagnostic = super::super::diagnostics::at(
                        self.source,
                        crate::domain::model::QualityTool::Pylint,
                        rule,
                        message.clone(),
                        super::super::diagnostics::point(entry.offset),
                    );
                    diagnostic.location.line = entry.report_line;
                    diagnostic.location.column = 0;
                    self.diagnostics.push(diagnostic);
                } else {
                    self.ignored_messages
                        .record(&processed, rule, symbol, entry.report_line);
                }
            }
            processed.push(entry);
        }
        false
    }
}
