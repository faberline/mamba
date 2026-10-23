//! Frozen original module facts supplement annotations with runtime identities.
use super::{callables::Target, values::Value};
use mamba_language::application::quality_syntax::{self, PythonTarget};
use serde::Deserialize;
use std::{collections::BTreeMap, rc::Rc, sync::OnceLock};

#[derive(Deserialize)]
pub(super) struct Member {
    pub kind: String,
    pub target: Option<String>,
    pub qname: Option<String>,
    signature: Option<String>,
    asynchronous: Option<bool>,
    #[serde(default)]
    decorators: Vec<String>,
    #[serde(default)]
    pub r#abstract: bool,
    #[serde(default)]
    pub exception: bool,
    #[serde(default)]
    attributes: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct Module {
    pub members: BTreeMap<String, Member>,
    pub dynamic: bool,
    hint_names: Vec<String>,
}

pub(super) fn module(name: &str, target: PythonTarget) -> Option<&'static Module> {
    static DATA: OnceLock<Vec<BTreeMap<String, Module>>> = OnceLock::new();
    let data = DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/pylint-stdlib-facts.json"))
            .expect("locked original standard-library facts")
    });
    data[match target {
        PythonTarget::Py312 => 0,
        PythonTarget::Py313 => 1,
        PythonTarget::Py314 => 2,
    }]
    .get(name)
}

pub(super) fn member(name: &str, target: PythonTarget) -> Option<&'static Member> {
    let (owner, name) = name.rsplit_once('.')?;
    module(owner, target)?.members.get(name)
}

pub(super) fn missing_hint(module: &str, name: &str, target: PythonTarget) -> String {
    self::module(module, target)
        .and_then(|module| {
            module
                .hint_names
                .iter()
                .find(|candidate| super::builtin_members::one_edit(name, candidate))
        })
        .map(|candidate| format!("; maybe '{candidate}'?"))
        .unwrap_or_default()
}

pub(super) fn call_target(module: &str, name: &str, target: PythonTarget) -> Option<Target> {
    let fact = self::module(module, target)?.members.get(name)?;
    let qname = fact.qname.as_deref()?;
    let (owner, actual) = qname.rsplit_once('.')?;
    let source = match fact.target.as_deref()? {
        "function" => format!(
            "{}{}def {name}({}):\n    return _unknown\n",
            fact.decorators
                .iter()
                .map(|value| format!("@{value}\n"))
                .collect::<String>(),
            if fact.asynchronous == Some(true) {
                "async "
            } else {
                ""
            },
            fact.signature.as_deref().unwrap_or("*args, **kwargs"),
        ),
        "class" => {
            let body = fact.signature.as_ref().map_or_else(
                || "    pass\n".to_string(),
                |signature| format!("    def __init__({signature}):\n        return _unknown\n"),
            );
            format!("class {name}:\n{body}")
        }
        _ => return None,
    };
    let document = quality_syntax::parse(&source, target, true).ok()?;
    let mut targets = super::callables::collect(&document.parsed.syntax().body, owner, false);
    match targets.remove(name)? {
        Target::Function(function) => {
            let mut function = (*function).clone();
            function.name = actual.into();
            function.returns = super::callables::Returns::Unknown;
            function.result_expression = None;
            function.result_expressions.clear();
            Some(Target::Function(Rc::new(function)))
        }
        Target::Class(class) => {
            let mut class = (*class).clone();
            class.name = actual.into();
            class.identity.0 = owner.into();
            for name in &fact.attributes {
                class
                    .members
                    .entry(name.clone())
                    .or_insert_with(|| Target::Value(Rc::new(Value::Unknown)));
                class
                    .first_members
                    .entry(name.clone())
                    .or_insert_with(|| Target::Value(Rc::new(Value::Unknown)));
                class.declared_members.insert(name.clone());
            }
            Some(Target::Class(Rc::new(class)))
        }
        _ => None,
    }
}
