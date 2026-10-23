//! Only the project path prefix in known file-encoding messages is normalized.
use serde_json::Value;

pub(super) fn normalize(diagnostics: &mut Value) {
    for diagnostic in diagnostics.as_array_mut().expect("diagnostic list") {
        if diagnostic["rule"] != "F0010" {
            continue;
        }
        let message = diagnostic["message"].as_str().expect("message");
        let prefix = "error while code parsing: Wrong or no encoding specified for ";
        let relative = diagnostic["path"].as_str().expect("path");
        let Some(path) = message
            .strip_prefix(prefix)
            .and_then(|value| value.strip_suffix('.'))
        else {
            continue;
        };
        if path == relative || path.ends_with(&format!("/{relative}")) {
            diagnostic["message"] = Value::String(format!("{prefix}<project>/{relative}."));
        }
    }
}
