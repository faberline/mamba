//! Standard replacement calls check the receiver and generated field signature.
use super::super::*;
use quality_syntax::python_ast::ExprCall;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_dataclass_replace_call(
        &mut self,
        call: &ExprCall,
    ) -> bool {
        let Some((signature, invalid)) = self
            .scopes
            .dataclass_replace_signature(call, &self.environment())
        else {
            return false;
        };
        if invalid {
            self.emit(
                "type-var",
                format!(
                    "Value of type variable \"_DataclassT\" of \"replace\" cannot be \"{}\"",
                    self.context.name(signature.parameters[0].ty.unwrap()),
                ),
                call,
            );
        }
        let first = self.diagnostics.len();
        self.bind_call(call, &signature);
        if let Some(module) = self
            .scopes
            .dataclass_replace_origin(call, &self.environment())
        {
            for diagnostic in &mut self.diagnostics[first..] {
                if diagnostic.rule == "call-arg"
                    && diagnostic.message.starts_with("Missing named argument")
                {
                    let note = format!("\"replace\" defined in \"{module}\"");
                    diagnostic.hint = Some(
                        diagnostic
                            .hint
                            .take()
                            .map_or_else(|| note.clone(), |hint| format!("{note}\n{hint}")),
                    );
                }
            }
        }
        true
    }
}
