//! Unary operations report original unsupported types without binary extensions.
use super::{operand_inference::Operand, Checker};
use mamba_language::application::quality_syntax::python_ast::{ExprUnaryOp, UnaryOp};
use ruff_text_size::Ranged;
impl Checker<'_> {
    pub(super) fn check_unary_operand(&mut self, value: &ExprUnaryOp) {
        let (operator, method) = match value.op {
            UnaryOp::UAdd => ("+", "__pos__"),
            UnaryOp::USub => ("-", "__neg__"),
            UnaryOp::Invert => ("~", "__invert__"),
            UnaryOp::Not => return,
        };
        let alternatives = self.operand_alternatives(&value.operand);
        if alternatives
            .iter()
            .any(|value| matches!(value, Operand::Unknown))
        {
            return;
        }
        let mut seen_instances = std::collections::BTreeSet::new();
        for operand in alternatives {
            if let Operand::Instance(class, _) = &operand {
                if !seen_instances.insert(class.identity.clone()) {
                    continue;
                }
            }
            let supported = match &operand {
                Operand::Unknown | Operand::Generator => true,
                Operand::Literal(name) => {
                    matches!(*name, "int" | "bool")
                        || (operator != "~" && matches!(*name, "float" | "complex"))
                }
                Operand::Instance(class, origin) => self
                    .class_member(class.clone(), origin.clone(), method, false)
                    .is_some(),
                Operand::Class(class, origin) => self
                    .lineage(class.clone(), origin.clone())
                    .into_iter()
                    .find_map(|(class, context)| {
                        class
                            .metaclass
                            .as_ref()
                            .and_then(|value| self.expression_target_in(value, context))
                    })
                    .is_some_and(|value| match value.target {
                        super::callables::Target::Class(class) => self
                            .class_member(class, value.context, method, false)
                            .is_some(),
                        _ => false,
                    }),
                _ => false,
            };
            if !supported {
                if let Some(name) = operand.name() {
                    self.emit(
                        "E1130",
                        "invalid-unary-operand-type",
                        &format!("bad operand type for unary {operator}: {name}"),
                        value.range(),
                    );
                }
            }
        }
    }
}
