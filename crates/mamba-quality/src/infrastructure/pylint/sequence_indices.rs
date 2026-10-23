//! Sequence indices preserve builtin method ownership and slice inference quirks.
use super::{
    callables::Target, operand_inference::Operand, resolution::Context, subscript_protocols,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprSubscript, Number};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

fn sequence(name: &str) -> bool {
    matches!(
        name,
        "str" | "list" | "tuple" | "bytearray" | "range" | "bytes" | "memoryview"
    )
}
impl Checker<'_> {
    fn builtin_sequence_method(&self, value: &Operand, method: &str) -> bool {
        match value {
            Operand::Literal(name) => {
                sequence(name) && (method == "__getitem__" || *name == "list")
            }
            Operand::Instance(class, origin) => {
                if !self.operand_known_bases(class.clone(), origin.clone(), &mut BTreeSet::new()) {
                    return false;
                }
                self.lineage(class.clone(), origin.clone())
                    .into_iter()
                    .find_map(|(class, context)| {
                        if class.identity.0 == "builtins" && class.name == "memoryview" {
                            return Some(true);
                        }
                        if class.methods.contains_key(method) {
                            Some(class.identity.0 == "builtins" && sequence(&class.name))
                        } else if self
                            .local_class_member(&class, context, method, false)
                            .is_some()
                        {
                            Some(false)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(false)
            }
            _ => false,
        }
    }
    fn index_instance(&self, value: &Operand) -> bool {
        match value {
            Operand::Instance(class, origin) => self
                .class_member(class.clone(), origin.clone(), "__index__", true)
                .is_some(),
            _ => false,
        }
    }
    fn known_slice_target(&self, value: &Operand) -> bool {
        matches!(
            value,
            Operand::Literal("list" | "tuple" | "dict" | "set" | "frozenset" | "str" | "bytes")
        ) || matches!(value, Operand::Instance(class, _) if class.identity.0 == "builtins" && class.name == "range")
    }
    fn check_slice_parts(
        &mut self,
        lower: Option<&Expr>,
        upper: Option<&Expr>,
        step: Option<&Expr>,
        target: Option<&Operand>,
        origin: &Context,
    ) {
        let mut invalid = Vec::new();
        for part in [lower, upper, step].into_iter().flatten() {
            let operand = self.operand(part);
            if self.index_instance(&operand) {
                return;
            }
            if matches!(
                operand,
                Operand::Unknown | Operand::Literal("int" | "bool" | "NoneType")
            ) {
                continue;
            }
            invalid.push(part);
        }
        if target.is_some_and(|value| !self.known_slice_target(value)) {
            return;
        }
        for value in invalid {
            self.emit_operand_origin(
                origin,
                "E1127",
                "invalid-slice-index",
                "Slice index is not an int, None, or instance with __index__",
                value.range(),
            );
        }
        let zero = step.is_some_and(|step| match step {
            Expr::BooleanLiteral(value) => !value.value,
            Expr::NumberLiteral(value) => match &value.value {
                Number::Int(value) => value.to_string() == "0",
                Number::Float(value) => *value == 0.0,
                Number::Complex { real, imag } => *real == 0.0 && *imag == 0.0,
            },
            _ => false,
        });
        if zero {
            self.emit_operand_origin(
                origin,
                "E1144",
                "invalid-slice-step",
                "Slice step cannot be 0",
                step.unwrap().range(),
            );
        }
    }
    fn inferred_slice(&mut self, value: &Expr, target: &Operand) -> bool {
        let Some((value, origin)) = self
            .operand_nodes(value, Context::Scope(self.scopes.current_index()))
            .into_iter()
            .next()
        else {
            return false;
        };
        if let Expr::Slice(slice) = &value {
            self.check_slice_parts(
                slice.lower.as_deref(),
                slice.upper.as_deref(),
                slice.step.as_deref(),
                Some(target),
                &origin,
            );
            return true;
        }
        if let Expr::Call(call) = &value {
            if self.expression_target_in(&call.func,origin.clone()).is_some_and(|resolved|matches!(resolved.target,Target::Class(class) if class.identity.0=="builtins" && class.name=="slice")) {
                let mut args=Vec::new();
                for part in &call.arguments.args {
                    let Some((part,context))=self.operand_nodes(part,origin.clone()).into_iter().next() else{return true;};
                    if !matches!(part,Expr::NumberLiteral(ref number) if matches!(number.value,Number::Int(_))) && !matches!(part,Expr::NoneLiteral(_) | Expr::BooleanLiteral(_)) {return true;}
                    args.push((part,context));
                }
                if (1..=3).contains(&args.len()) {
                    let part=|index|args.get(index).map(|(value,_)|value);
                    self.check_slice_parts(part(0),part(1),part(2),None,&origin);
                }
                return true;
            }
        }
        false
    }
    pub(super) fn check_sequence_index(&mut self, value: &ExprSubscript) {
        if matches!(
            value.value.as_ref(),
            Expr::ListComp(_) | Expr::DictComp(_) | Expr::Generator(_)
        ) {
            return;
        }
        let target = self.operand(&value.value);
        if !self.builtin_sequence_method(&target, subscript_protocols::method(value)) {
            return;
        }
        if self.inferred_slice(&value.slice, &target) {
            return;
        }
        let index = self.operand(&value.slice);
        if matches!(
            index,
            Operand::Unknown | Operand::Literal("int" | "bool" | "slice")
        ) || self.index_instance(&index)
            || matches!(&index, Operand::Instance(class, _) if class.identity.0 == "builtins" && matches!(class.name.as_str(), "int" | "slice"))
        {
            return;
        }
        self.emit(
            "E1126",
            "invalid-sequence-index",
            "Sequence index is not an int, slice, or instance with __index__",
            value.range(),
        );
    }
}
