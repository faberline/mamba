//! Explicit class declarations preserve inherited variable kinds and class values.
use super::*;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_inherited_class_annotation(
        &mut self,
        assignment: &StmtAnnAssign,
    ) {
        if !self.in_class || !self.functions.is_empty() {
            return;
        }
        let Expr::Name(name) = assignment.target.as_ref() else {
            return;
        };
        let Some(id) = self
            .class_types
            .last()
            .copied()
            .flatten()
            .and_then(|ty| self.context.class_id(ty))
        else {
            return;
        };
        let class = self.context.class(id);
        let Some(field) = class.fields.get(name.id.as_str()) else {
            return;
        };
        for base in self.context.class_lineage(id).into_iter().skip(1) {
            let parent = self.context.class(base);
            let Some(original) = parent.fields.get(name.id.as_str()) else {
                continue;
            };
            if original.class_var != field.class_var {
                let (previous, current) = if original.class_var {
                    ("class", "instance")
                } else {
                    ("instance", "class")
                };
                self.emit("misc", format!("Cannot override {previous} variable (previously declared on base class \"{}\") with {current} variable", parent.name), assignment.target.as_ref());
                break;
            }
        }
    }

    pub(in crate::infrastructure::mypy::checker) fn check_inherited_class_assignment(
        &mut self,
        assignment: &StmtAssign,
    ) {
        if !self.in_class || !self.functions.is_empty() {
            return;
        }
        let Some(id) = self
            .class_types
            .last()
            .copied()
            .flatten()
            .and_then(|ty| self.context.class_id(ty))
        else {
            return;
        };
        let Some(actual) = self.infer(&assignment.value).and_then(|ty| {
            if self.context.is_class_value(ty) {
                return Some(ty);
            }
            let signature = self.context.callable_signature(ty)?;
            if !signature.constructor {
                return None;
            }
            let returned = signature.returns?;
            let id = self.context.class_id(returned).or_else(|| {
                self.context
                    .builtin_scalar(returned)
                    .map(|scalar| self.context.builtin_class(scalar))
            })?;
            Some(self.context.class(id).object)
        }) else {
            return;
        };
        for target in &assignment.targets {
            let Expr::Name(name) = target else {
                continue;
            };
            for base in self.context.class_lineage(id).into_iter().skip(1) {
                let parent = self.context.class(base);
                let Some(expected) = parent
                    .fields
                    .get(name.id.as_str())
                    .and_then(|field| field.ty)
                    .filter(|&ty| self.context.is_class_value(ty))
                else {
                    continue;
                };
                if !self.context.accepts(expected, actual) {
                    let original = self.context.class(self.context.class_id(expected).unwrap());
                    let original_name = original.name.rsplit('.').next().unwrap();
                    self.emit("assignment", format!("Incompatible types in assignment (expression has type \"{}\", base class \"{}\" defined the type as \"type[{original_name}]\")",self.context.name(actual),parent.name), assignment.value.as_ref());
                    break;
                }
            }
        }
    }
}
