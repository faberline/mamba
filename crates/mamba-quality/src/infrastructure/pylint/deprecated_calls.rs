//! Deprecated calls retain raw spellings and original positional argument gates.
use super::{callables::Target, deprecated_data, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    Decorator, Expr, ExprAttribute, ExprCall,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

impl Checker<'_> {
    pub(super) fn check_deprecated_decorators(&mut self, decorators: &[Decorator]) {
        let Some(first) = decorators.first() else {
            return;
        };
        let expression = match &first.expression {
            Expr::Call(call) => call.func.as_ref(),
            value => value,
        };
        let Some(name) = self.deprecated_callable(expression) else {
            return;
        };
        if deprecated_data::fixed(self.modules.target())
            .decorators
            .contains(&name)
        {
            let end = decorators.last().unwrap().end();
            self.emit(
                "W4905",
                "deprecated-decorator",
                &format!("Using deprecated decorator {name}()"),
                ruff_text_size::TextRange::new(first.start(), end),
            );
        }
    }
    pub(super) fn deprecated_callable(&self, expression: &Expr) -> Option<String> {
        if let Some(name) = self.stdlib_name(expression) {
            if let Some(member) = super::stdlib_facts::member(&name, self.modules.target()) {
                return member.qname.clone();
            }
        }
        let resolved = self.expression_target(expression)?;
        match resolved.target {
            Target::Function(signature) => {
                let module = match &resolved.context {
                    Context::Module(module) => module.as_str(),
                    Context::Scope(_) => self.module_name,
                };
                self.source_qualification(module, signature.definition_range.start().to_u32())
                    .or_else(|| self.stdlib_name(expression))
                    .or_else(|| Some(format!("{module}.{}", signature.name)))
            }
            Target::Class(class) => self
                .source_qualification(&class.identity.0, class.identity.1)
                .or_else(|| Some(format!("{}.{}", class.identity.0, class.name))),
            _ => None,
        }
    }

    pub(super) fn check_deprecated_call(&mut self, call: &ExprCall) {
        let data = deprecated_data::fixed(self.modules.target());
        if let Expr::Attribute(attribute) = call.func.as_ref() {
            if let Expr::Name(module) = attribute.value.as_ref() {
                self.emit_deprecated_class(
                    module.id.as_str(),
                    attribute.attr.as_str(),
                    call.range(),
                );
            }
        }
        let raw = match call.func.as_ref() {
            Expr::Name(name) => name.id.as_str(),
            Expr::Attribute(value) => value.attr.as_str(),
            _ => return,
        };
        let Some(name) = self.deprecated_callable(&call.func) else {
            return;
        };
        let names: BTreeSet<_> = [name.as_str(), raw].into_iter().collect();
        if names.iter().any(|name| data.methods.contains(*name)) {
            if raw == "warn" && self.logging_method(&call.func).is_some() {
                return;
            }
            self.emit(
                "W4902",
                "deprecated-method",
                &format!("Using deprecated method {raw}()"),
                call.range(),
            );
            return;
        }
        for name in names {
            for (position, argument) in data.arguments.get(name).into_iter().flatten() {
                if call.arguments.keywords.iter().any(|keyword| {
                    keyword
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == argument)
                }) || position.is_some_and(|position| position < call.arguments.args.len())
                {
                    self.emit(
                        "W4903",
                        "deprecated-argument",
                        &format!("Using deprecated argument {argument} of method {raw}()"),
                        call.range(),
                    );
                }
            }
        }
    }

    pub(super) fn emit_deprecated_class(
        &mut self,
        module: &str,
        name: &str,
        range: ruff_text_size::TextRange,
    ) {
        if deprecated_data::fixed(self.modules.target())
            .classes
            .get(module)
            .is_some_and(|names| names.contains(name))
        {
            self.emit(
                "W4904",
                "deprecated-class",
                &format!("Using deprecated class {name} of module {module}"),
                range,
            );
        }
    }

    pub(super) fn check_deprecated_attribute(&mut self, attribute: &ExprAttribute) {
        if !attribute.ctx.is_load() {
            return;
        }
        let Some(resolved) = self.expression_target(&attribute.value) else {
            return;
        };
        let owner = match resolved.target {
            Target::Module(module) => module,
            Target::Class(class) | Target::Instance(class, _) => {
                format!("{}.{}", class.identity.0, class.name)
            }
            _ => return,
        };
        let name = format!("{owner}.{}", attribute.attr);
        if deprecated_data::fixed(self.modules.target())
            .attributes
            .contains(&name)
        {
            self.emit(
                "W4906",
                "deprecated-attribute",
                &format!("Using deprecated attribute '{name}'"),
                attribute.range(),
            );
        }
    }
}
