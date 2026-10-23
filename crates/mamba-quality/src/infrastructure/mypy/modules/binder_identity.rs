//! Finite refresh tests for declaration identity. Never call Modules::build.
use super::*;
use mamba_language::application::quality_syntax::{self, PythonTarget};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Write;

const REFRESHES: usize = 3;
const TARGETS: [(PythonTarget, &str); 3] = [
    (PythonTarget::Py312, "3.12"),
    (PythonTarget::Py313, "3.13"),
    (PythonTarget::Py314, "3.14"),
];

fn fixture() -> Value {
    let input = include_str!("binder_identity.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(input.as_bytes())),
        "194f54c86101469efa2a6086d0514f509106c490299516cc132c7da5da21918b"
    );
    let value: Value = serde_json::from_str(input).unwrap();
    assert_eq!(
        value["unchanged_refreshes"].as_u64(),
        Some(REFRESHES as u64)
    );
    value
}

fn source<'a>(fixture: &'a Value, name: &str) -> &'a str {
    let input = &fixture["inputs"][name];
    let source = input["source"].as_str().unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        input["source_sha256"].as_str().unwrap()
    );
    source
}

// This is only a lookup registry. There is no pending queue or fixpoint loop.
fn registry(context: Rc<Context>, target: PythonTarget, names: &[&str]) -> Rc<Modules> {
    let info: Vec<_> = names
        .iter()
        .map(|name| (0, (*name).to_string(), false))
        .collect();
    for (_, name, _) in &info {
        context.set_module_target(format!("0:{name}"), target);
    }
    Rc::new(Modules {
        context,
        names: names
            .iter()
            .enumerate()
            .map(|(id, name)| ((0, (*name).to_string()), id))
            .collect(),
        info,
        exports: RefCell::new(vec![Exports::new(); names.len()]),
        export_states: RefCell::new(vec![Some(Default::default()); names.len()]),
        export_gaps: RefCell::new(vec![BTreeSet::new(); names.len()]),
        export_uses: RefCell::new(vec![BTreeSet::new(); names.len()]),
        namespaces: names.iter().map(|name| (0, (*name).to_string())).collect(),
        shadow_namespaces: names.iter().map(|name| (0, (*name).to_string())).collect(),
        embedded_sources: vec![false; names.len()],
        early_issues: RefCell::new(vec![None; names.len()]),
        reexports: vec![BTreeMap::new(); names.len()],
        dependency_roots: vec![Vec::new()],
        untyped: BTreeSet::new(),
        imported: BTreeSet::new(),
    })
}

// Alias markers retain declaration identity; compare their expanded annotation target.
fn alias_target(context: &Context, alias: Option<Type>) -> Option<Type> {
    alias
        .and_then(|ty| context.alias_id(ty))
        .and_then(|id| context.expand_alias(id, Vec::new()))
}

// A shape panic remains a test failure. Keep partial raw evidence for that target.
fn capture_target(
    version: &str,
    rows: &mut Vec<Value>,
    failures: &mut usize,
    body: impl FnOnce(&mut Vec<Value>, &mut usize),
) {
    rows.push(json!({"python": version, "phase": "target_begin"}));
    if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        body(rows, failures);
    })) {
        *failures += 1;
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied());
        rows.push(json!({"python": version, "phase": "unexpected_panic",
            "payload": message, "payload_is_string": message.is_some(),
            "payload_type_id": format!("{:?}", payload.as_ref().type_id()),
            "increments_failure": true}));
    }
}

fn variable_types(context: &Context, signature: &Signature) -> Vec<Type> {
    let mut variables = Vec::new();
    for &ty in signature.generic.iter() {
        context.collect_variables(ty, &mut variables);
    }
    for parameter in signature.parameters.iter() {
        if let Some(ty) = parameter.ty {
            context.collect_variables(ty, &mut variables);
        }
    }
    if let Some(ty) = signature.returns {
        context.collect_variables(ty, &mut variables);
    }
    for variant in signature.overloads.iter() {
        for ty in variable_types(context, variant) {
            if !variables.contains(&ty) {
                variables.push(ty);
            }
        }
    }
    variables.sort();
    variables
}

fn one_variable(context: &Context, export: &Export) -> Type {
    let variables = variable_types(context, export.callable.as_ref().unwrap());
    assert_eq!(
        variables.len(),
        1,
        "Fixed declaration must have one generic binder"
    );
    variables[0]
}

fn variable_id(context: &Context, ty: Type) -> u32 {
    let node = context.node(ty).unwrap();
    let super::super::types::Node::Variable(variable) = node.as_ref() else {
        panic!("Expected a variable node: {node:?}");
    };
    variable.id
}

fn variable_bound(context: &Context, ty: Type) -> Type {
    let node = context.node(ty).unwrap();
    let super::super::types::Node::Variable(variable) = node.as_ref() else {
        panic!("Expected a variable node: {node:?}");
    };
    variable.bound
}

fn type_value(context: &Context, ty: Option<Type>) -> Value {
    json!({
        "type_id": format!("{ty:?}"),
        "raw_node": ty.and_then(|ty| context.node(ty)).map(|node| format!("{node:#?}")),
    })
}

fn signature_value(context: &Context, signature: &Signature) -> Value {
    json!({
        "raw_signature": format!("{signature:#?}"),
        "generic_nodes": variable_types(context, signature).into_iter()
            .map(|ty| type_value(context, Some(ty))).collect::<Vec<_>>(),
        "returns": type_value(context, signature.returns),
    })
}

fn export_value(context: &Context, export: &Export) -> Value {
    json!({
        "ty": type_value(context, export.ty),
        "callable": export.callable.as_ref().map(|signature| signature_value(context, signature)),
        "module": export.module, "explicit": export.explicit,
        "module_hidden": export.module_hidden,
        "alias": type_value(context, export.alias),
    })
}

fn exports_value(context: &Context, exports: &Exports) -> Value {
    Value::Object(
        exports
            .iter()
            .map(|(name, export)| (name.clone(), export_value(context, export)))
            .collect(),
    )
}

fn signature_changes(before: &Signature, after: &Signature) -> Vec<&'static str> {
    let mut fields = Vec::new();
    macro_rules! field {
        ($name:ident) => {
            if before.$name != after.$name {
                fields.push(stringify!($name));
            }
        };
    }
    field!(name);
    field!(parameters);
    field!(returns);
    field!(narrowing);
    field!(return_explicit);
    field!(implicit);
    field!(ignored);
    field!(variables);
    field!(generic);
    field!(owner);
    field!(constructor);
    field!(ellipsis);
    field!(overloads);
    field!(bound_receiver);
    field!(definition_module);
    field!(native_symbol);
    fields
}

fn export_delta(context: &Context, before: &Exports, after: &Exports) -> Value {
    let names: BTreeSet<_> = before.keys().chain(after.keys()).collect();
    let mut changes = Vec::new();
    for name in names {
        let old = before.get(name);
        let new = after.get(name);
        if old == new {
            continue;
        }
        let mut fields = Vec::new();
        let mut signature_fields = Vec::new();
        if let (Some(old), Some(new)) = (old, new) {
            if old.ty != new.ty {
                fields.push("ty");
            }
            if old.callable != new.callable {
                fields.push("callable");
            }
            if old.module != new.module {
                fields.push("module");
            }
            if old.explicit != new.explicit {
                fields.push("explicit");
            }
            if old.module_hidden != new.module_hidden {
                fields.push("module_hidden");
            }
            if old.alias != new.alias {
                fields.push("alias");
            }
            if let (Some(old), Some(new)) = (&old.callable, &new.callable) {
                signature_fields = signature_changes(old, new);
            }
        } else {
            fields.push("presence");
        }
        changes.push(json!({
            "name": name, "changed_fields": fields, "changed_signature_fields": signature_fields,
            "before": old.map(|export| export_value(context, export)),
            "after": new.map(|export| export_value(context, export)),
        }));
    }
    json!({
        "exports_equal": before == after, "changes": changes,
        "raw_before": exports_value(context, before), "raw_after": exports_value(context, after),
    })
}

fn evidence(name: &str, rows: Vec<Value>, failures: usize) {
    if let Some(root) = std::env::var_os("MAMBA_QUALITY_MYPY_BINDER_IDENTITY_EVIDENCE") {
        std::fs::create_dir_all(&root).unwrap();
        let path = std::path::PathBuf::from(root).join(format!("{name}.json"));
        let mut stream = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        serde_json::to_writer_pretty(
            &mut stream,
            &json!({
                "schema": "mamba.quality.mypy-binder-identity.v1", "rows": rows, "failed": failures,
            "fixture_sha256": "194f54c86101469efa2a6086d0514f509106c490299516cc132c7da5da21918b",
                "finite_test_refreshes": REFRESHES, "calls_modules_build": false,
                "whole_mypy_accepted": false, "whole_plan_accepted": false,
            }),
        )
        .unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();
    }
}

#[test]
fn native_generic_imports_keep_declaration_identity() {
    let inputs = fixture();
    let aliases: Vec<_> = inputs["native_alias_pairs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| (pair[0].as_str().unwrap(), pair[1].as_str().unwrap()))
        .collect();
    let mut rows = Vec::new();
    let mut failures = 0;
    for (target, version) in TARGETS {
        capture_target(version, &mut rows, &mut failures, |rows, failures| {
            let context = Rc::new(Context::default());
            let modules = registry(context.clone(), target, &["native_a", "native_b"]);
            let a = quality_syntax::parse(source(&inputs, "native_a"), target, false).unwrap();
            let b = quality_syntax::parse(source(&inputs, "native_b"), target, false).unwrap();
            assert!(a.is_valid() && b.is_valid());
            let mut left = Scopes::new(
                &a.parsed.syntax().body,
                context.clone(),
                "0:native_a".into(),
            );
            let mut right = Scopes::new(
                &b.parsed.syntax().body,
                context.clone(),
                "0:native_b".into(),
            );
            left.attach_modules(Modules::lookup(&modules, 0), &a.parsed.syntax().body);
            right.attach_modules(Modules::lookup(&modules, 1), &b.parsed.syntax().body);
            let mut previous_left = left.exports();
            let mut previous_right = right.exports();
            rows.push(json!({"python": version, "phase": "native_initial_exports",
            "raw_left": exports_value(&context, &previous_left),
            "raw_right": exports_value(&context, &previous_right)}));
            let mut distinct = BTreeSet::new();
            for &(a, b) in &aliases {
                let x = one_variable(&context, &previous_left[a]);
                let y = one_variable(&context, &previous_right[b]);
                let shared_declaration =
                    x == y && previous_left[a].callable == previous_right[b].callable;
                *failures += usize::from(!shared_declaration);
                distinct.insert(variable_id(&context, x));
                rows.push(
                    json!({"python": version, "phase": "alias_identity", "left": a, "right": b,
                "same_declaration_identity": shared_declaration,
                "raw_left": export_value(&context, &previous_left[a]),
                "raw_right": export_value(&context, &previous_right[b])}),
                );
            }
            *failures += usize::from(distinct.len() != aliases.len());
            rows.push(
                json!({"python": version, "phase": "distinct_native_declarations",
            "binder_ids": distinct, "expected_distinct": aliases.len(),
            "raw_left": exports_value(&context, &previous_left),
            "raw_right": exports_value(&context, &previous_right)}),
            );
            for refresh in 1..=REFRESHES {
                left.attach_modules(Modules::lookup(&modules, 0), &a.parsed.syntax().body);
                right.attach_modules(Modules::lookup(&modules, 1), &b.parsed.syntax().body);
                let current_left = left.exports();
                let current_right = right.exports();
                *failures += usize::from(previous_left != current_left);
                *failures += usize::from(previous_right != current_right);
                rows.push(
                json!({"python": version, "phase": "unchanged_import_refresh", "refresh": refresh,
                "left": export_delta(&context, &previous_left, &current_left),
                "right": export_delta(&context, &previous_right, &current_right)}),
            );
                previous_left = current_left;
                previous_right = current_right;
            }
        });
    }
    evidence("native_generic_imports", rows, failures);
    assert_eq!(
        failures, 0,
        "Native generic declaration identity changed across fixed refreshes"
    );
}

#[test]
fn generic_declarations_keep_distinct_binders_and_refresh_real_bounds() {
    let inputs = fixture();
    let mut rows = Vec::new();
    let mut failures = 0;
    for (target, version) in TARGETS {
        capture_target(version, &mut rows, &mut failures, |rows, failures| {
            let context = Rc::new(Context::default());
            let modules = registry(context.clone(), target, &["consumer", "provider"]);
            let provider_int =
                quality_syntax::parse(source(&inputs, "provider_int"), target, false).unwrap();
            let provider_str =
                quality_syntax::parse(source(&inputs, "provider_str"), target, false).unwrap();
            let consumer =
                quality_syntax::parse(source(&inputs, "declarations"), target, false).unwrap();
            assert!(provider_int.is_valid() && provider_str.is_valid() && consumer.is_valid());
            let mut provider = Scopes::new(
                &provider_int.parsed.syntax().body,
                context.clone(),
                "0:provider".into(),
            );
            provider.attach_modules(
                Modules::lookup(&modules, 1),
                &provider_int.parsed.syntax().body,
            );
            let before_provider = provider.exports();
            let before_target = alias_target(
                &context,
                before_provider.get("Bound").and_then(|export| export.alias),
            );
            rows.push(
                json!({"python": version, "phase": "provider_int_alias_target",
            "raw_provider": exports_value(&context, &before_provider),
            "semantic_target": type_value(&context, before_target),
            "expected_target": type_value(&context, Some(Type::Int))}),
            );
            *failures += usize::from(before_target != Some(Type::Int));
            modules.exports.borrow_mut()[1] = before_provider.clone();
            let mut scope = Scopes::new(
                &consumer.parsed.syntax().body,
                context.clone(),
                "0:consumer".into(),
            );
            scope.attach_modules(Modules::lookup(&modules, 0), &consumer.parsed.syntax().body);
            let initial = scope.exports();
            rows.push(json!({"python": version, "phase": "local_initial_exports",
            "raw_consumer": exports_value(&context, &initial)}));
            let left = one_variable(&context, &initial["left"]);
            let right = one_variable(&context, &initial["right"]);
            *failures += usize::from(variable_id(&context, left) == variable_id(&context, right));
            *failures += usize::from(variable_bound(&context, left) != Type::Int);
            *failures += usize::from(
                initial["direct"].callable.as_ref().unwrap().returns != Some(Type::Int),
            );
            rows.push(json!({"python": version, "phase": "distinct_local_declarations",
            "raw_provider": exports_value(&context, &before_provider), "raw_consumer": exports_value(&context, &initial),
            "left_binder_id": variable_id(&context, left), "right_binder_id": variable_id(&context, right)}));
            let mut previous = initial.clone();
            for refresh in 1..=REFRESHES {
                scope.attach_modules(Modules::lookup(&modules, 0), &consumer.parsed.syntax().body);
                let current = scope.exports();
                *failures += usize::from(previous != current);
                rows.push(
                    json!({"python": version, "phase": "unchanged_bound_int", "refresh": refresh,
                "delta": export_delta(&context, &previous, &current)}),
                );
                previous = current;
            }
            let mut provider = Scopes::new(
                &provider_str.parsed.syntax().body,
                context.clone(),
                "0:provider".into(),
            );
            provider.attach_modules(
                Modules::lookup(&modules, 1),
                &provider_str.parsed.syntax().body,
            );
            let after_provider = provider.exports();
            let after_target = alias_target(
                &context,
                after_provider.get("Bound").and_then(|export| export.alias),
            );
            rows.push(
                json!({"python": version, "phase": "provider_str_alias_target",
            "raw_provider": exports_value(&context, &after_provider),
            "semantic_target": type_value(&context, after_target),
            "expected_target": type_value(&context, Some(Type::Str))}),
            );
            *failures += usize::from(after_target != Some(Type::Str));
            modules.exports.borrow_mut()[1] = after_provider.clone();
            scope.attach_modules(Modules::lookup(&modules, 0), &consumer.parsed.syntax().body);
            let changed = scope.exports();
            rows.push(json!({"python": version, "phase": "local_changed_exports",
            "raw_consumer": exports_value(&context, &changed)}));
            let new_left = one_variable(&context, &changed["left"]);
            let new_right = one_variable(&context, &changed["right"]);
            *failures +=
                usize::from(variable_id(&context, left) != variable_id(&context, new_left));
            *failures +=
                usize::from(variable_id(&context, right) != variable_id(&context, new_right));
            *failures +=
                usize::from(left == new_left || variable_bound(&context, new_left) != Type::Str);
            *failures += usize::from(
                changed["direct"].callable.as_ref().unwrap().returns != Some(Type::Str),
            );
            *failures += usize::from(previous == changed);
            rows.push(json!({"python": version, "phase": "actual_bound_and_return_change",
            "provider_delta": export_delta(&context, &before_provider, &after_provider),
            "consumer_delta": export_delta(&context, &previous, &changed),
            "left_binder_before": variable_id(&context, left), "left_binder_after": variable_id(&context, new_left),
            "right_binder_before": variable_id(&context, right), "right_binder_after": variable_id(&context, new_right)}));
            previous = changed;
            for refresh in 1..=REFRESHES {
                scope.attach_modules(Modules::lookup(&modules, 0), &consumer.parsed.syntax().body);
                let current = scope.exports();
                *failures += usize::from(previous != current);
                rows.push(
                    json!({"python": version, "phase": "unchanged_bound_str", "refresh": refresh,
                "delta": export_delta(&context, &previous, &current)}),
                );
                previous = current;
            }
        });
    }
    evidence("local_generic_bounds", rows, failures);
    assert_eq!(
        failures, 0,
        "Local declaration identity or real bound/return refresh was lost"
    );
}
