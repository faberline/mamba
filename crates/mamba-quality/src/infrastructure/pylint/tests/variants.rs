//! Only whole diagnostic lists observed from the pinned original are alternatives.
use serde_json::Value;
use std::sync::OnceLock;

fn fixtures() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("variants.json")).unwrap())
}

pub(super) fn observed(case: &Value, target: usize, actual: &Value, exit: i64) -> bool {
    fixtures()["cases"].as_array().unwrap().iter().any(|entry| {
        entry["name"] == case["name"]
            && entry["python"] == ["3.12", "3.13", "3.14"][target]
            && entry["source_hashes"] == case["source_sha256"]
            && entry["runs"].as_array().unwrap().iter().any(|run| {
                run["exit_code"] == exit
                    && exit == case["reference_exits"][target].as_i64().unwrap()
                    && run["diagnostics"] == *actual
            })
    })
}

#[test]
fn alternatives_reject_unobserved_message_location_exit_and_source() {
    let entry = &fixtures()["cases"][0];
    let target = ["3.12", "3.13", "3.14"]
        .iter()
        .position(|version| entry["python"] == *version)
        .unwrap();
    let run = &entry["runs"][0];
    let exit = run["exit_code"].as_i64().unwrap();
    let case = serde_json::json!({
        "name":entry["name"], "source_sha256":entry["source_hashes"],
        "reference_exits":[exit, exit, exit],
    });
    assert!(observed(&case, target, &run["diagnostics"], exit));
    for pointer in [
        "/0/message",
        "/0/rule",
        "/0/path",
        "/0/severity",
        "/0/hint",
        "/0/location/line",
        "/0/location/column",
        "/0/location/end_line",
        "/0/location/end_column",
    ] {
        let mut changed = run["diagnostics"].clone();
        *changed.pointer_mut(pointer).unwrap() = Value::String("unobserved".into());
        assert!(!observed(&case, target, &changed, exit), "{pointer}");
    }
    assert!(!observed(&case, target, &run["diagnostics"], exit ^ 1));
    let mut changed = case.clone();
    changed["source_sha256"]["main.py"] = Value::String("unobserved".into());
    assert!(!observed(&changed, target, &run["diagnostics"], exit));
}
