//! The embedded version-info value has five readonly typed fields.
use super::super::classes::Field;
use super::*;
use mamba_language::application::quality_syntax::python_ast::CmpOp;
mod indexes;

impl Context {
    pub(in crate::infrastructure::mypy) fn system_version_info(&self) -> Type {
        let id = self.declare_class("sys._version_info", "_version_info", 0);
        if self.class(id).fields.is_empty() {
            let mut class = (*self.class(id)).clone();
            let parent = self.builtin_class(Type::Object);
            class.bases.push(parent);
            class.base_types.push(self.class(parent).instance);
            class.final_class = true;
            for (name, ty) in ["major", "minor", "micro", "releaselevel", "serial"]
                .into_iter()
                .zip(self.version_info_items())
            {
                class.fields.insert(
                    name.into(),
                    Field {
                        ty: Some(ty),
                        needs_annotation: false,
                        class_access: false,
                        readonly: true,
                        final_attribute: false,
                        class_var: false,
                        abstract_field: false,
                    },
                );
            }
            self.replace_class(id, class);
        }
        self.class(id).instance
    }

    fn version_info_items(&self) -> Vec<Type> {
        let levels = ["alpha", "beta", "candidate", "final"]
            .into_iter()
            .map(|value| self.intern(Node::Literal(Literal::Str(value.into()))));
        vec![
            Type::Int,
            Type::Int,
            Type::Int,
            self.union(levels.collect()),
            Type::Int,
        ]
    }

    pub(in crate::infrastructure::mypy) fn system_version_items(
        &self,
        ty: Type,
    ) -> Option<Vec<Type>> {
        (!self.is_class_value(ty)
            && self
                .class_id(ty)
                .is_some_and(|id| self.class(id).namespace == "sys._version_info"))
        .then(|| self.version_info_items())
    }

    pub(in crate::infrastructure::mypy) fn version_tuple_items(
        &self,
        ty: Type,
    ) -> Option<Vec<Type>> {
        self.system_version_items(ty)
            .or_else(|| match self.node(ty).as_deref() {
                Some(Node::Tuple(items)) => Some(items.clone()),
                _ => None,
            })
    }

    pub(in crate::infrastructure::mypy) fn version_comparison(
        &self,
        left: Type,
        right: Type,
        operator: CmpOp,
    ) -> Option<comparisons::Comparison> {
        let (_, _, symbol) = comparisons::method(operator)?;
        let left_tuple = self.version_tuple_items(left).is_some();
        let right_tuple = self.version_tuple_items(right).is_some();
        let numeric = |ty| matches!(ty, Type::Int | Type::Float | Type::Bool);
        let supported = left.is_any()
            || right.is_any()
            || (left_tuple && right_tuple)
            || (numeric(left) && numeric(right));
        Some(comparisons::Comparison {
            returns: Type::Bool,
            failure: (!supported).then_some(comparisons::Failure::Right),
            symbol,
            left,
            right,
        })
    }

    pub(in crate::infrastructure::mypy) fn version_overlap(&self, left: Type, right: Type) -> bool {
        match (
            self.version_tuple_items(left),
            self.version_tuple_items(right),
        ) {
            (Some(left), Some(right)) => {
                left.len() == right.len()
                    && left
                        .into_iter()
                        .zip(right)
                        .all(|(left, right)| self.type_overlap(left, right))
            }
            (Some(_), None) | (None, Some(_)) => {
                left.is_any() || right.is_any() || left == Type::Object || right == Type::Object
            }
            (None, None) => self.type_overlap(left, right),
        }
    }
}
