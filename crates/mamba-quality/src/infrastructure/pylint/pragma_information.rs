//! Information events retain message status changes and their original order.
use super::directives::Directive;

pub(super) fn notification(
    template: &Directive,
    rule: &'static str,
    symbol: &'static str,
    message: String,
) -> Directive {
    let mut entry = template.clone();
    entry.text.clear();
    entry.problem = Some((rule, symbol, message));
    entry
}

pub(super) fn deprecated(template: &Directive, before: &str, after: &str) -> Directive {
    notification(
        template,
        "I0022",
        "deprecated-pragma",
        format!("Pragma \"{before}\" is deprecated, use \"{after}\" instead"),
    )
}

pub(super) fn ignored(template: &Directive) -> Vec<Directive> {
    let notice = notification(
        template,
        "I0013",
        "file-ignored",
        "Ignoring entire file".into(),
    );
    let mut skipped = template.clone();
    skipped.skip = true;
    vec![notice, skipped]
}

pub(super) fn local(template: &Directive, action: &str, code: &str) -> Option<Directive> {
    if action.starts_with("enable") || code == "I0011" {
        return None;
    }
    let mut entry = notification(
        template,
        "I0011",
        "locally-disabled",
        format!(
            "Locally disabling {} ({code})",
            super::message_registry::symbol(code)
        ),
    );
    entry.report_line = template.line + u32::from(action == "disable-next");
    Some(entry)
}
