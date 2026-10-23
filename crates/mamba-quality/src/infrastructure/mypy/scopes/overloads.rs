//! Overload declarations remain ordered and separate from their implementation.
use super::*;
use mamba_language::application::quality_syntax::python_ast::StmtFunctionDef;
mod resolve;

impl Scopes {
    pub(crate) fn is_overload(&self, function: &StmtFunctionDef) -> bool {
        super::super::signatures::decorators::overload(function, &self.current().overload_aliases)
    }

    pub(super) fn overload_signature(
        &self,
        function: &StmtFunctionDef,
        mut signature: Signature,
    ) -> Arc<Signature> {
        let previous = self.raw_function_signature(function.name.as_str());
        let variants = previous
            .as_ref()
            .map(|signature| signature.overloads.to_vec())
            .unwrap_or_default();
        if self.is_overload(function) {
            let variant = Arc::new(signature.clone());
            signature.overloads = variants
                .into_iter()
                .chain(std::iter::once(variant))
                .collect::<Vec<_>>()
                .into();
        } else {
            signature.overloads = variants.into();
        }
        Arc::new(signature)
    }

    pub(super) fn refresh_overloads(&mut self, body: &[Stmt]) {
        let mut groups: BTreeMap<&str, Vec<&StmtFunctionDef>> = BTreeMap::new();
        for statement in body {
            if let Stmt::FunctionDef(function) = statement {
                if self.is_overload(function) {
                    groups
                        .entry(function.name.as_str())
                        .or_default()
                        .push(function);
                }
            }
        }
        for (name, functions) in groups {
            let Some(previous) = self.raw_function_signature(name) else {
                continue;
            };
            let mut signature = (*previous).clone();
            signature.overloads = previous
                .overloads
                .iter()
                .zip(functions)
                .map(|(signature, function)| {
                    self.capture_function_variables(Arc::new(
                        self.refresh_function_annotations(function, signature),
                    ))
                })
                .collect::<Vec<_>>()
                .into();
            let signature = Arc::new(signature);
            self.replace_raw_function_signature(name, signature);
        }
    }
}
