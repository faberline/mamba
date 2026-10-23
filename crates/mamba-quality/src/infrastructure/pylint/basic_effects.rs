//! Basic side-effect checks retain the original inference and lambda boundaries.
use super::{callables::Target, defaults, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall};
use ruff_text_size::{Ranged, TextRange};

impl Checker<'_> {
    pub(super) fn check_list_literal(&mut self, call: &ExprCall) {
        if call.arguments.args.is_empty() && call.arguments.keywords.is_empty() && matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "list") && self.expression_target(&call.func).is_some_and(|value| matches!(value.target, Target::Class(class) if class.identity.0 == "builtins" && class.name == "list")) {
            self.emit("R1734", "use-list-literal", "Consider using [] instead of list()", call.range());
        }
    }
    pub(super) fn check_dict_literal(&mut self, call: &ExprCall) {
        if !matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "dict")
            || !call.arguments.args.is_empty()
        {
            return;
        }
        if !self.expression_target(&call.func).is_some_and(|value| matches!(value.target, Target::Class(class) if class.identity.0 == "builtins" && class.name == "dict")) { return; }
        let mut elements = Vec::new();
        for keyword in call
            .arguments
            .keywords
            .iter()
            .filter(|value| value.arg.is_some())
        {
            if !elements.is_empty() && elements.join(", ").len() >= 64 {
                break;
            }
            elements.push(format!(
                "\"{}\": {}",
                keyword.arg.as_ref().unwrap(),
                super::display::expression(&keyword.value, &self.source.text)
            ));
        }
        for keyword in call
            .arguments
            .keywords
            .iter()
            .filter(|value| value.arg.is_none())
        {
            if !elements.is_empty() && elements.join(", ").len() >= 64 {
                break;
            }
            elements.push(format!(
                "**{}",
                super::display::expression(&keyword.value, &self.source.text)
            ));
        }
        let elements = elements.join(", ");
        let suggestion = format!(
            "{{{elements}{}}}",
            if elements.len() > 64 { ", ... " } else { "" }
        );
        self.emit(
            "R1735",
            "use-dict-literal",
            &format!("Consider using '{suggestion}' instead of a call to 'dict'."),
            call.range(),
        );
    }
    pub(super) fn check_exception_statement(&mut self, call: &ExprCall, range: TextRange) {
        let name = match call.func.as_ref() {
            Expr::Name(name) => name.id.as_str(),
            Expr::Attribute(attribute) => attribute.attr.as_str(),
            _ => "",
        };
        if !name.chars().next().is_some_and(char::is_uppercase) {
            return;
        }
        let Some(resolved) = self.expression_target(&Expr::Call(call.clone())) else {
            return;
        };
        let Target::Instance(class, origin) = resolved.target else {
            return;
        };
        let frozen_exception = self
            .modules
            .source_index(self.source.project, &class.identity.0)
            .is_none()
            && super::stdlib_facts::member(
                &format!("{}.{}", class.identity.0, class.name),
                self.modules.target(),
            )
            .is_some_and(|member| member.exception);
        if frozen_exception
            || self
                .lineage(class, origin)
                .iter()
                .any(|(class, _)| class.identity.0 == "builtins" && class.name == "BaseException")
        {
            self.emit(
                "W0133",
                "pointless-exception-statement",
                "Exception statement has no effect",
                range,
            );
        }
    }

    pub(super) fn check_rebound_argument(&mut self, value: &Expr) {
        match value {
            Expr::Name(name) => {
                let argument = self.scopes.declarations().get(name.id.as_str()).is_some_and(|definition| matches!(definition.kind, crate::infrastructure::project::symbols::DefinitionKind::Parameter { .. }));
                if argument && !defaults::fixed().ignored(name.id.as_str(), true) {
                    self.emit(
                        "R1704",
                        "redefined-argument-from-local",
                        &format!("Redefining argument with the local name '{}'", name.id),
                        name.range(),
                    );
                }
            }
            Expr::Tuple(tuple) => {
                for value in &tuple.elts {
                    self.check_rebound_argument(value);
                }
            }
            Expr::List(list) => {
                for value in &list.elts {
                    self.check_rebound_argument(value);
                }
            }
            Expr::Starred(value) => self.check_rebound_argument(&value.value),
            _ => {}
        }
    }

    pub(super) fn check_quit_exit(&mut self, call: &ExprCall) {
        if matches!(call.func.as_ref(), Expr::Name(name) if matches!(name.id.as_str(), "exit" | "quit"))
        {
            let imported = self.scopes.global("exit").is_some_and(|definition| {
                matches!(
                    definition.kind,
                    crate::infrastructure::project::symbols::DefinitionKind::Import(_)
                )
            }) || self
                .scopes
                .declarations()
                .get("exit")
                .is_some_and(|definition| {
                    matches!(
                        definition.kind,
                        crate::infrastructure::project::symbols::DefinitionKind::Import(_)
                    )
                });
            if !imported {
                self.emit(
                    "R1722",
                    "consider-using-sys-exit",
                    "Consider using 'sys.exit' instead",
                    call.range(),
                );
            }
        }
    }
}
