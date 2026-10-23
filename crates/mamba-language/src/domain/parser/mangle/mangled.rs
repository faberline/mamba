/// The mangled form of `name` under class context `class`, or `None` when the
/// name is not eligible (no class context, missing leading `__`, trailing
/// `__`, or an all-underscore class name).
pub(super) fn mangled(name: &str, class: Option<&str>) -> Option<String> {
    let class = class?;
    if !name.starts_with("__") || name.ends_with("__") {
        return None;
    }
    let stripped = class.trim_start_matches('_');
    if stripped.is_empty() {
        return None;
    }
    Some(format!("_{stripped}{name}"))
}
