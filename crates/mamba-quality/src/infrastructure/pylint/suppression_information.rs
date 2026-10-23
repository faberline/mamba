//! Suppression reports retain raw directive order and ignored checker events.
use super::{
    directives::{self, Directive},
    message_registry, Checker,
};
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Ignored {
    values: Vec<(String, u32, BTreeSet<u32>)>,
}
impl Ignored {
    pub(super) fn record(&mut self, directives: &[Directive], rule: &str, name: &str, line: u32) {
        let Some(from) = origin(directives, rule, name, line) else {
            return;
        };
        if let Some((_, _, lines)) = self
            .values
            .iter_mut()
            .find(|(code, source, _)| code == rule && *source == from)
        {
            lines.insert(line);
        } else {
            self.values
                .push((rule.into(), from, BTreeSet::from([line])));
        }
    }
}

fn origin(directives: &[Directive], rule: &str, name: &str, line: u32) -> Option<u32> {
    let mut result = None;
    for entry in directives {
        let first = if rule.starts_with('I')
            || matches!(
                rule,
                "E0011" | "W0012" | "R0022" | "C0301" | "C0303" | "C0304" | "R1710"
            ) {
            entry.line
        } else {
            entry.start_line
        };
        if line < first || line > entry.end_line {
            continue;
        }
        let Some((action, arguments)) = entry.text.split_once('=') else {
            continue;
        };
        if !arguments
            .split(',')
            .any(|target| target == rule || target == name)
        {
            continue;
        }
        if action == "disable-next" {
            if entry.line + 1 == line {
                return Some(line);
            }
        } else if entry.inline && !rule.starts_with('I') {
            if action == "disable" && entry.line == line {
                return Some(entry.line);
            }
        } else {
            match action {
                "disable" => result = Some(entry.line),
                "enable" => result = None,
                _ => {}
            }
        }
    }
    result
}

impl Checker<'_> {
    pub(super) fn record_suppression(&mut self, rule: &str, name: &str, line: u32) {
        self.ignored_messages
            .record(&self.directives, rule, name, line);
    }
    pub(super) fn check_suppression_information(&mut self) {
        let mut states = Vec::<(String, u32, bool)>::new();
        let mut rules = Vec::<String>::new();
        for entry in &self.directives {
            let Some((action, targets)) = entry.text.split_once('=') else {
                continue;
            };
            let line = entry.line + u32::from(action == "disable-next");
            for rule in targets.split(',').filter(|value| !value.is_empty()) {
                if !rules.iter().any(|value| value == rule) {
                    rules.push(rule.into());
                }
                let enabled = action == "enable";
                if let Some((_, _, state)) = states
                    .iter_mut()
                    .find(|(code, source, _)| code == rule && *source == line)
                {
                    *state = enabled;
                } else {
                    states.push((rule.into(), line, enabled));
                }
            }
        }
        for rule in rules {
            for (code, line, enabled) in states.iter().filter(|(code, _, _)| code == &rule) {
                if !enabled
                    && !message_registry::useless_exempt(code)
                    && !self
                        .ignored_messages
                        .values
                        .iter()
                        .any(|(value, source, _)| value == code && source == line)
                {
                    self.suppression_notice(
                        "I0021",
                        "useless-suppression",
                        &format!(
                            "Useless suppression of '{}'",
                            message_registry::symbol(code)
                        ),
                        *line,
                    );
                }
            }
        }
        for (rule, from, lines) in self.ignored_messages.values.clone() {
            for line in lines {
                self.suppression_notice(
                    "I0020",
                    "suppressed-message",
                    &format!(
                        "Suppressed '{}' (from line {from})",
                        message_registry::symbol(&rule)
                    ),
                    line,
                );
            }
        }
    }
    fn suppression_notice(&mut self, rule: &str, name: &str, message: &str, line: u32) {
        if !directives::active(&self.directives, rule, line) {
            self.record_suppression(rule, name, line);
            return;
        }
        let offset = super::source_layout::lines(&self.source.text)
            .get(line.saturating_sub(1) as usize)
            .map_or(0, |(offset, _)| *offset);
        let mut value = super::super::diagnostics::at(
            self.source,
            crate::domain::model::QualityTool::Pylint,
            rule,
            message.into(),
            super::super::diagnostics::point(offset),
        );
        value.location.line = line;
        value.location.column = 0;
        self.diagnostics.push(value);
    }
}
