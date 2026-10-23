//! Runtime defaults correct selected embedded stubs without changing project classes.
use super::{callables::Target, values::Value};
use std::{collections::BTreeMap, rc::Rc};

pub(super) fn adjust_targets(module: &str, targets: &mut BTreeMap<String, Target>) {
    if module == "itertools" {
        if let Some(Target::Function(function)) = targets.get("tee") {
            let mut function = (**function).clone();
            function.returns = super::callables::Returns::Value;
            targets.insert("tee".into(), Target::Function(Rc::new(function)));
        }
        return;
    }
    if module == "dataclasses" {
        if let Some(Target::Function(function)) = targets.get("make_dataclass") {
            let mut function = (**function).clone();
            function.returns = super::callables::Returns::Value;
            targets.insert("make_dataclass".into(), Target::Function(Rc::new(function)));
        }
        return;
    }
    if module != "contextlib" {
        return;
    }
    let Some(Target::Class(original)) = targets.get("nullcontext") else {
        return;
    };
    let mut class = (**original).clone();
    let Some(method) = class.methods.get_mut("__init__") else {
        return;
    };
    let mut signature = (*method.signature).clone();
    let Some(parameter) = signature
        .parameters
        .iter_mut()
        .find(|value| value.name == "enter_result")
    else {
        return;
    };
    parameter.required = false;
    parameter.default = Some(Value::Scalar("NoneType"));
    let signature = Rc::new(signature);
    method.signature = signature.clone();
    class
        .members
        .insert("__init__".into(), Target::Function(signature.clone()));
    class
        .first_members
        .insert("__init__".into(), Target::Function(signature));
    targets.insert("nullcontext".into(), Target::Class(Rc::new(class)));
}
