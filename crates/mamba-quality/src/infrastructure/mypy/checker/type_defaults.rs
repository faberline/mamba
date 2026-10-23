//! Type defaults keep declaration, version, and annotation errors independent.
use super::*;
use quality_syntax::python_ast::{StmtAssign, StmtClassDef, TypeParam, TypeParams};

impl Checker<'_> {
    pub(super) fn emit_column_zero(&mut self, rule: &str, message: String, node: &impl Ranged) {
        let previous = self.diagnostics.len();
        let point = ruff_text_size::TextRange::at(node.start(), ruff_text_size::TextSize::new(1));
        self.emit(rule, message, &point);
        if self.diagnostics.len() != previous {
            let location = &mut self.diagnostics.last_mut().unwrap().location;
            location.column = 0;
            location.end_column = Some(0);
            location.offset = None;
            location.end_offset = None;
        }
    }

    pub(super) fn check_type_default_version(
        &mut self,
        parameters: Option<&TypeParams>,
        declaration: &impl Ranged,
    ) {
        if self.python_target == quality_syntax::PythonTarget::Py312
            && parameters.is_some_and(|parameters| {
                parameters
                    .type_params
                    .iter()
                    .any(|parameter| match parameter {
                        TypeParam::TypeVar(parameter) => parameter.default.is_some(),
                        TypeParam::ParamSpec(parameter) => parameter.default.is_some(),
                        TypeParam::TypeVarTuple(parameter) => parameter.default.is_some(),
                    })
            })
        {
            self.emit("syntax", "Type parameter defaults: requires Python 3.13 or newer (current target: Python 3.12)".into(), &ruff_text_size::TextRange::at(declaration.start(), ruff_text_size::TextSize::new(1)));
        }
    }

    pub(super) fn check_class_type_defaults(&mut self, class: &StmtClassDef) {
        self.check_type_default_version(class.type_params.as_deref(), class);
        self.check_parameter_spec_defaults(class.type_params.as_deref());
        self.check_tuple_variable_defaults(class.type_params.as_deref());
        let Some(id) = self
            .scopes
            .class_object(class.name.as_str())
            .and_then(|ty| self.context.class_id(ty))
        else {
            return;
        };
        let mut previous_default = None;
        for &ty in &self.context.class(id).generic {
            let node = self.context.node(ty).unwrap();
            let types::Node::Variable(variable) = node.as_ref() else {
                continue;
            };
            if variable.default.is_some() {
                previous_default = Some(variable.name.clone());
                if class.type_params.is_some() {
                    self.check_variable_default(ty, class);
                }
            } else if let Some(previous) = &previous_default {
                self.emit("misc", format!("\"{}\" cannot appear after \"{previous}\" in type parameter list because it has no default type", variable.name), class);
            }
        }
    }

    pub(super) fn check_legacy_type_default(&mut self, assignment: &StmtAssign) {
        self.check_parameter_spec_legacy(assignment);
        self.check_tuple_variable_legacy(assignment);
        let [Expr::Name(name)] = assignment.targets.as_slice() else {
            return;
        };
        let Expr::Call(call) = assignment.value.as_ref() else {
            return;
        };
        let typevar = match call.func.as_ref() {
            Expr::Name(name) => name.id.as_str() == "TypeVar",
            Expr::Attribute(value) => value.attr.as_str() == "TypeVar",
            _ => false,
        };
        if typevar {
            if let Some(&ty) = self.scopes.variables().get(name.id.as_str()) {
                self.check_variable_default(ty, assignment);
            }
        }
    }

    pub(super) fn check_variable_default(&mut self, ty: types::Type, declaration: &impl Ranged) {
        let Some(node) = self.context.node(ty) else {
            return;
        };
        let types::Node::Variable(variable) = node.as_ref() else {
            return;
        };
        let Some(default) = variable.default else {
            return;
        };
        if !self.context.accepts(variable.bound, default) {
            self.emit_column_zero(
                "misc",
                "TypeVar default must be a subtype of the bound type".into(),
                declaration,
            );
        } else if !variable.values.is_empty()
            && !variable.values.contains(&default)
            && !default.is_any()
        {
            self.emit_column_zero(
                "misc",
                "TypeVar default must be one of the constraint types".into(),
                declaration,
            );
        }
    }
}
