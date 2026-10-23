//! Referenced class objects retain their value identity in declared class fields.
use super::*;

impl Context {
    pub(in crate::infrastructure::mypy::classes) fn declared_class_value(
        &self,
        expression: &Expr,
        variables: &types::Variables,
    ) -> Option<Type> {
        if !matches!(expression, Expr::Name(_) | Expr::Attribute(_)) {
            return None;
        }
        let ty = self.annotation_with_variables(expression, variables)?;
        let id = self.class_id(ty).or_else(|| {
            self.builtin_scalar(ty)
                .map(|scalar| self.builtin_class(scalar))
        })?;
        Some(self.class(id).object)
    }
}
