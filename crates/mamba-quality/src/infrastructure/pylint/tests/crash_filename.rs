//! The approved crash filename variant leaves all other fatal fields exact.
use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

pub(super) fn normalize(diagnostics: &mut Value) {
    static MESSAGE: OnceLock<Regex> = OnceLock::new();
    let pattern = MESSAGE.get_or_init(|| Regex::new(
        r"^(.*?): Fatal error while checking '(.*?)'\. Please open an issue in our bug tracker so we address this\. There is a pre-filled template that you can use in '(.*/)?pylint-crash-([0-9]{4}-[0-9]{2}-[0-9]{2}-[0-9]{2}-[0-9]{2}-[0-9]{2})\.txt'\.$",
    ).unwrap());
    for diagnostic in diagnostics.as_array_mut().unwrap() {
        if diagnostic["rule"] != "F0002" {
            continue;
        }
        let message = diagnostic["message"].as_str().unwrap();
        let Some(parts) = pattern.captures(message) else {
            continue;
        };
        let path = diagnostic["path"].as_str().unwrap();
        let same_path = |value: &str| value == path || value.ends_with(&format!("/{path}"));
        if !same_path(&parts[1]) || !same_path(&parts[2]) {
            continue;
        }
        diagnostic["message"] = Value::String(format!(
            "{path}: Fatal error while checking '{path}'. Please open an issue in our bug tracker so we address this. There is a pre-filled template that you can use in '<cache>/pylint-crash-<timestamp>.txt'."
        ));
    }
}

#[test]
fn crash_variation_retains_message_and_path_contract() {
    let message = "main.py: Fatal error while checking 'main.py'. Please open an issue in our bug tracker so we address this. There is a pre-filled template that you can use in '/cache/pylint-crash-2026-10-11-00-00-00.txt'.";
    let source = serde_json::json!([{"rule":"F0002", "path":"main.py", "message":message}]);
    let mut valid = source.clone();
    normalize(&mut valid);
    assert_ne!(valid, source);
    for changed in [
        message.replace("main.py: ", "other.py: "),
        message.replace("checking 'main.py'", "checking 'other.py'"),
        message.replace("Please open", "Please create"),
        message.replace("pylint-crash-2026", "other-crash-2026"),
    ] {
        let mut actual = source.clone();
        actual[0]["message"] = Value::String(changed);
        let before = actual.clone();
        normalize(&mut actual);
        assert_eq!(actual, before);
    }
}
