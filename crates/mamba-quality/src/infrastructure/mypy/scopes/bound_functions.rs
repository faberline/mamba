//! Nested functions keep enclosing type variables in the enclosing binder.
use super::*;

impl Scopes {
    pub(in crate::infrastructure::mypy) fn capture_function_variables(
        &self,
        signature: Arc<Signature>,
    ) -> Arc<Signature> {
        if self.current().kind != Kind::Function {
            return signature;
        }
        Arc::new(Signature {
            generic: signature
                .generic
                .iter()
                .copied()
                .filter(|&ty| !self.type_variable_bound(ty))
                .collect::<Vec<_>>()
                .into(),
            ..signature.as_ref().clone()
        })
    }
}
