//! Inline tokens preserve partial commands and the first parser failure.
#[derive(Clone)]
pub(super) enum Item {
    Command {
        action: String,
        targets: Vec<String>,
    },
    Failure {
        invalid: bool,
        token: String,
    },
}

fn message_action(value: &str) -> bool {
    matches!(
        value,
        "disable" | "enable" | "disable-next" | "disable-msg" | "enable-msg"
    )
}
fn emit(action: &str, targets: Vec<String>) -> Item {
    if targets.is_empty() && message_action(action) {
        Item::Failure {
            invalid: true,
            token: action.into(),
        }
    } else {
        Item::Command {
            action: action.into(),
            targets,
        }
    }
}

pub(super) fn parse(value: &str) -> Vec<Item> {
    static TOKENS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let tokens = TOKENS.get_or_init(|| regex::Regex::new(r"(?P<keyword>\b(disable-next|disable-msg|enable-msg|disable-all|skip-file|disable|enable)\b)|(?P<message>[0-9A-Za-z_-]{2,})|(?P<assign>=)|(?P<number>[CREIWF]\d*)").expect("inline pragma tokens"));
    let mut output = Vec::new();
    let mut action: Option<&str> = None;
    let mut targets = Vec::new();
    let mut assignment_required = false;
    let mut previous = "";
    for token in tokens.captures_iter(value) {
        let text = token.get(0).expect("pragma token").as_str();
        if token.name("assign").is_some() {
            if !assignment_required {
                output.push(Item::Failure {
                    invalid: action.is_none() && previous.is_empty(),
                    token: action.unwrap_or(previous).into(),
                });
                return output;
            }
            assignment_required = false;
        } else if assignment_required {
            output.push(Item::Failure {
                invalid: true,
                token: action.unwrap_or_default().into(),
            });
            return output;
        } else if token.name("keyword").is_some() {
            if let Some(action) = action {
                let item = emit(action, std::mem::take(&mut targets));
                let failed = matches!(item, Item::Failure { .. });
                output.push(item);
                if failed {
                    return output;
                }
            }
            action = Some(text);
            assignment_required = message_action(text);
        } else {
            targets.push(text.into());
            assignment_required = false;
        }
        previous = text;
    }
    if let Some(action) = action {
        output.push(emit(action, targets));
    } else {
        output.push(Item::Failure {
            invalid: false,
            token: previous.into(),
        });
    }
    output
}

pub(super) fn comment(value: &str) -> Option<&str> {
    static MARKER: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let marker = MARKER.get_or_init(|| {
        regex::Regex::new(r"\bpylint:\s*([^;#\n]+)").expect("inline pragma marker")
    });
    marker.captures(value)?.get(1).map(|value| value.as_str())
}
