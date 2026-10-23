//! Logging declarations use embedded type data and native callable identities.
use super::super::callables::{self, Kind, Returns, Target};
use mamba_language::application::quality_syntax;
use std::{collections::BTreeMap, rc::Rc};

pub(in crate::infrastructure::pylint) fn targets(
    target: quality_syntax::PythonTarget,
) -> BTreeMap<String, Target> {
    let source =
        crate::infrastructure::embedded::module("logging").expect("embedded logging declarations");
    let document = quality_syntax::parse(source, target, true).expect("embedded logging syntax");
    let mut targets = callables::collect(&document.parsed.syntax().body, "logging", false);
    for (name, value) in &mut targets {
        if let Target::Function(signature) = value {
            let mut signature = (**signature).clone();
            if name == "getLogger" {
                signature.returns = Returns::Value;
                signature.result_class = Some(("logging".into(), "Logger".into()));
            }
            if super::convenience(name) || name == "log" {
                signature
                    .parameters
                    .retain(|parameter| parameter.kind != Kind::KeywordOnly);
                signature.keywords = Some("kwargs".into());
            }
            *value = Target::Function(Rc::new(signature));
        }
        if let Target::Class(class) = value {
            let mut class = (**class).clone();
            for (method_name, method) in &mut class.methods {
                let mut signature = (*method.signature).clone();
                if super::convenience(method_name) || method_name == "log" {
                    signature
                        .parameters
                        .retain(|parameter| parameter.kind != Kind::KeywordOnly);
                    signature.keywords = Some("kwargs".into());
                }
                method.signature = Rc::new(signature);
            }
            *value = Target::Class(Rc::new(class));
        }
    }
    targets
}
