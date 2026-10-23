use super::*;

pub(super) fn private_binding_name(name: &str, class_name: Option<&str>) -> Name {
    if !name.starts_with("__") || name.ends_with("__") {
        return name.to_string();
    }
    let Some(class_name) = class_name else {
        return name.to_string();
    };
    let class_name = class_name.trim_start_matches('_');
    if class_name.is_empty() {
        return name.to_string();
    }
    format!("_{class_name}{name}")
}
