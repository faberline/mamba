//! Timeout checks use callable identity and keyword validity independently.
use super::{callables::Target, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;
use std::collections::BTreeMap;

impl Checker<'_> {
    pub(super) fn check_timeout_call(&mut self, call: &ExprCall) {
        let Some(resolved) = self.expression_target(&call.func) else {
            return;
        };
        let (module, offset) = match resolved.target {
            Target::Function(signature)
                if matches!(signature.category, "function" | "unboundmethod") =>
            {
                let module = match &resolved.context {
                    Context::Module(module) => module.as_str(),
                    Context::Scope(_) => self.module_name,
                };
                (
                    module.to_string(),
                    signature.definition_range.start().to_u32(),
                )
            }
            Target::Class(class) => class.identity.clone(),
            _ => return,
        };
        let Some(name) = self.source_qualification(&module, offset) else {
            return;
        };
        if !super::defaults::fixed().timeout_method(&name) {
            return;
        }
        let Some(keywords) = self.timeout_keywords(call) else {
            return;
        };
        if !keywords.contains_key("timeout") {
            let function = super::display::expression(&call.func, &self.source.text);
            self.emit(
                "W3101",
                "missing-timeout",
                &format!("Missing timeout argument for method '{function}' can cause your program to hang indefinitely"),
                call.range(),
            );
        }
    }
    fn timeout_keywords(&self, call: &ExprCall) -> Option<BTreeMap<String, bool>> {
        let origin = Context::Scope(self.scopes.current_index());
        let mut keywords = BTreeMap::new();
        for keyword in &call.arguments.keywords {
            if let Some(key) = &keyword.arg {
                keywords.insert(key.to_string(), true);
                continue;
            }
            let values = self.unpacked_value(&keyword.value, origin.clone())?;
            if !values.dictionary || values.view.is_some() {
                return None;
            }
            for (key, _) in values.entries? {
                let Expr::StringLiteral(key) = self.stdlib_constant(&key, values.origin.clone())?
                else {
                    return None;
                };
                let key = key.value.to_str().to_string();
                let valid = !keywords.contains_key(&key);
                keywords.insert(key, valid);
            }
        }
        keywords.values().all(|valid| *valid).then_some(keywords)
    }
}
