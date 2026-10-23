//! Hashability follows inferred hash members rather than runtime execution.
use super::{callables::Target, display, operand_inference::Operand, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprDict, ExprSet};
use ruff_text_size::{Ranged, TextRange};

impl Checker<'_> {
    pub(super) fn operand_hashable(&self, value: &Expr) -> bool {
        let values = self.operand_alternatives(value);
        values.is_empty() || values.into_iter().any(|value|match value {
            Operand::Literal(name)=>!matches!(name,"list"|"dict"|"set"),
            Operand::Instance(class,origin)=>{
                for (class,context) in self.lineage(class,origin) {
                    if class.identity.0=="builtins" && matches!(class.name.as_str(),"list"|"dict"|"set") {return false;}
                    if let Some(resolved)=self.local_class_member(&class,context,"__hash__",true) {
                        return !matches!(resolved.target,Target::Value(value) if matches!(value.as_ref(),Value::Scalar("NoneType")));
                    }
                }
                true
            }
            _=>true,
        })
    }
    pub(super) fn report_unhashable(
        &mut self,
        value: &Expr,
        role: &str,
        container: &str,
        range: TextRange,
    ) {
        self.emit(
            "E1143",
            "unhashable-member",
            &format!(
                "'{}' is unhashable and can't be used as a {role} in a {container}",
                display::expression(value, &self.source.text)
            ),
            range,
        );
    }
    pub(super) fn check_hashable_dictionary(&mut self, value: &ExprDict) {
        for item in &value.items {
            if let Some(key) = &item.key {
                if !self.operand_hashable(key) {
                    self.report_unhashable(key, "key", "dict", key.range());
                }
            }
        }
    }
    pub(super) fn check_hashable_set(&mut self, value: &ExprSet) {
        for item in &value.elts {
            if !self.operand_hashable(item) {
                self.report_unhashable(item, "member", "set", item.range());
            }
        }
    }
}
