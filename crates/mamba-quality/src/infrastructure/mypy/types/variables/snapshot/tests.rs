//! Small snapshot, map-view, and Reader bridge checks need no workspace loading.
use super::*;
use crate::infrastructure::mypy::{declarations::Reader, signatures::Signature, types::Context};
use mamba_language::application::quality_syntax::{self, python_ast::Stmt, PythonTarget};
use std::hash::{DefaultHasher, Hash, Hasher};

fn hash(value: &impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn variables_cow_clone_then_write_preserves_full_snapshot() {
    let context = Context::default();
    let first = context.variable("T".into(), Type::Object, vec![], None);
    let second = context.variable("T".into(), Type::Object, vec![], None);
    assert_ne!(first, second);
    let mut current = Variables::from([
        ("T".into(), first),
        ("keep".into(), Type::Str),
        ("remove".into(), Type::Int),
    ]);
    let old = current.clone();
    let mut signature = Signature::empty("captured".into(), first);
    signature.variables = current.clone();
    assert!(Arc::ptr_eq(&old.0, &current.0));
    assert!(Arc::ptr_eq(&signature.variables.0, &current.0));

    current.insert("added".into(), Type::Bool);
    assert!(!Arc::ptr_eq(&old.0, &current.0));
    assert_eq!(current.remove("remove"), Some(Type::Int));
    *current.entry("T".into()).or_insert(Type::Any) = second;
    current.entry("new_entry".into()).or_insert(Type::Float);

    assert_eq!(old, signature.variables);
    assert_eq!(
        old.as_map(),
        &VariableMap::from([
            ("T".into(), first),
            ("keep".into(), Type::Str),
            ("remove".into(), Type::Int),
        ])
    );
    assert_eq!(
        current.as_map(),
        &VariableMap::from([
            ("T".into(), second),
            ("added".into(), Type::Bool),
            ("keep".into(), Type::Str),
            ("new_entry".into(), Type::Float),
        ])
    );
}

#[test]
fn variables_cow_map_api_preserves_content_order_and_hash() {
    let context = Context::default();
    let first = context.variable("T".into(), Type::Object, vec![], None);
    let second = context.variable("T".into(), Type::Object, vec![], None);
    let entries = [
        ("z".into(), Type::Str),
        ("T".into(), first),
        ("a".into(), Type::Int),
        ("z".into(), Type::Bool),
    ];
    let raw = VariableMap::from(entries.clone());
    let from_array = Variables::from(entries.clone());
    let collected: Variables = entries.into_iter().collect();
    let from_map = Variables::from(raw.clone());
    assert!(!Arc::ptr_eq(&from_array.0, &from_map.0));
    assert_eq!(from_array, collected);
    assert_eq!(from_array, from_map);
    assert_eq!(from_array["z"], Type::Bool);
    assert_eq!(hash(&raw), hash(&from_array));
    assert_eq!(hash(&from_array), hash(&from_map));
    assert_eq!(format!("{raw:?}"), format!("{from_array:?}"));
    let mut borrowed = Vec::new();
    for (name, &ty) in &from_array {
        borrowed.push((name.clone(), ty));
    }
    assert_eq!(
        borrowed,
        raw.iter()
            .map(|(name, &ty)| (name.clone(), ty))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        from_array.keys().map(String::as_str).collect::<Vec<_>>(),
        ["T", "a", "z"]
    );
    let mut different_binder = from_map.clone();
    different_binder.insert("T".into(), second);
    assert_ne!(different_binder, from_array);
    assert_eq!(from_map["T"], first);
    assert!(Variables::new().is_empty());
    assert_eq!(Variables::default().as_map(), &VariableMap::new());
}

#[test]
fn variables_cow_read_views_accept_raw_and_snapshot_maps() {
    let context = Context::default();
    let first = context.variable("T".into(), Type::Object, vec![], None);
    let second = context.variable("T".into(), Type::Object, vec![], None);
    let raw = VariableMap::from([
        ("T".into(), first),
        ("P".into(), context.parameter_spec("P".into(), None)),
        ("Ts".into(), context.tuple_variable("Ts".into(), None)),
    ]);
    let snapshot = Variables::from(raw.clone());
    let shared = snapshot.clone();
    let document = quality_syntax::parse("T\n", PythonTarget::Py314, false).unwrap();
    assert!(document.is_valid());
    let [Stmt::Expr(statement)] = document.parsed.syntax().body.as_slice() else {
        panic!("one annotation expression required")
    };
    let query = |view: &VariableMap| context.annotation_with_variables(&statement.value, view);
    assert_eq!(query(&raw), Some(first));
    assert_eq!(query(&snapshot), query(&raw));
    assert!(Arc::ptr_eq(&snapshot.0, &shared.0));
    let mut shadow = snapshot.clone();
    shadow.insert("T".into(), second);
    assert_eq!(query(&shadow), Some(second));
    assert_eq!(query(&snapshot), Some(first));
    assert_eq!(shadow["P"], snapshot["P"]);
    assert_eq!(shadow["Ts"], snapshot["Ts"]);
}

#[test]
fn variables_cow_reader_bridge_preserves_all_entries() {
    let context = Context::default();
    let raw = VariableMap::from([("alpha".into(), Type::Int), ("omega".into(), Type::Str)]);
    let reader = Reader {
        context: &context,
        aliases: BTreeMap::new(),
        original_aliases: BTreeMap::new(),
        variables: raw.clone(),
        aliases_ready: true,
    };
    context.classes.borrow_mut().builtin_value_variables[0] = reader.variables.into();
    let saved = context.classes.borrow().builtin_value_variables[0].clone();
    let mut reader = Reader {
        context: &context,
        aliases: BTreeMap::new(),
        original_aliases: BTreeMap::new(),
        variables: context.classes.borrow().builtin_value_variables[0]
            .as_map()
            .clone(),
        aliases_ready: true,
    };
    assert_eq!(reader.variables, raw);
    reader.variables.remove("alpha");
    reader.variables.insert("omega".into(), Type::Bool);
    reader.variables.insert("new".into(), Type::Float);
    assert_eq!(saved.as_map(), &raw);
    assert_eq!(context.classes.borrow().builtin_value_variables[0], saved);
    assert_eq!(
        reader.variables,
        VariableMap::from([("new".into(), Type::Float), ("omega".into(), Type::Bool),])
    );
}
