//! Subscript diagnostics retain original protocol and source-range behavior.
use super::{
    callables::Target, classes::Class, display, operand_inference::Operand, resolution::Context,
    values::Value, Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprSubscript};
use ruff_text_size::Ranged;
use std::{collections::BTreeSet, rc::Rc};

pub(super) fn method(value: &ExprSubscript) -> &'static str {
    if value.ctx.is_store() {
        "__setitem__"
    } else if value.ctx.is_del() {
        "__delitem__"
    } else {
        "__getitem__"
    }
}
impl Checker<'_> {
    pub(super) fn operand_protocol_member(
        &self,
        class: Rc<Class>,
        origin: Context,
        name: &str,
    ) -> bool {
        if class.identity.0 == "builtins"
            && class.name == "memoryview"
            && matches!(name, "__getitem__" | "__setitem__" | "__delitem__")
        {
            return true;
        }
        let Some(resolved) = self.class_member(class, origin, name, false) else {
            return false;
        };
        match resolved.target {
            Target::Value(value) => {
                !matches!(
                    value.as_ref(),
                    Value::Scalar(_)
                        | Value::Number(_)
                        | Value::Boolean(_)
                        | Value::String(_)
                        | Value::Bytes(_)
                ) && !matches!(value.as_ref(), Value::Sequence(values, _) if values.iter().all(|value| matches!(value, Value::Scalar(_) | Value::Number(_) | Value::Boolean(_) | Value::String(_) | Value::Bytes(_))))
            }
            _ => true,
        }
    }
    fn operand_supports_subscript(&self, operand: &Operand, method: &str) -> bool {
        match operand {
            Operand::Unknown | Operand::Generator => true,
            Operand::Literal(name) => match method {
                "__getitem__" => matches!(*name, "list" | "tuple" | "str" | "bytes" | "dict"),
                _ => matches!(*name, "list" | "dict"),
            },
            Operand::Instance(class, origin) => {
                !self.operand_known_bases(class.clone(), origin.clone(), &mut BTreeSet::new())
                    || self.dynamic_member(class.clone(), origin.clone())
                    || self.operand_protocol_member(class.clone(), origin.clone(), method)
            }
            Operand::Class(class, origin) => {
                if !self.operand_known_bases(class.clone(), origin.clone(), &mut BTreeSet::new()) {
                    return true;
                }
                if method == "__getitem__" {
                    if self.operand_protocol_member(
                        class.clone(),
                        origin.clone(),
                        "__class_getitem__",
                    ) {
                        return true;
                    }
                    if class.identity.0 == "builtins"
                        && matches!(
                            class.name.as_str(),
                            "list" | "tuple" | "dict" | "set" | "frozenset" | "type"
                        )
                    {
                        return true;
                    }
                    if self.annotation_context
                        && self
                            .source
                            .text
                            .lines()
                            .any(|line| line.trim() == "from __future__ import annotations")
                    {
                        return true;
                    }
                }
                self.lineage(class.clone(), origin.clone())
                    .into_iter()
                    .find_map(|(class, context)| {
                        class
                            .metaclass
                            .as_ref()
                            .and_then(|value| self.expression_target_in(value, context))
                    })
                    .is_some_and(|resolved| match resolved.target {
                        Target::Class(class) => {
                            self.operand_protocol_member(class, resolved.context, method)
                        }
                        _ => false,
                    })
            }
            Operand::Function(_) => false,
        }
    }
    fn subscript_abstract_context(&self) -> bool {
        self.class_stack.iter().any(|(class, origin)| {
            let name = class.name.to_lowercase();
            let abstract_base=class.bases.iter().any(|base|self.declared_base(base,origin.clone()).is_some_and(|resolved|matches!(resolved.target,Target::Class(class) if matches!((class.identity.0.as_str(),class.name.as_str()),("abc"|"_py_abc","ABC")|("typing"|"typing_extensions","Protocol")))));
            let abstract_meta=class.metaclass.as_ref().and_then(|value|self.expression_target_in(value,origin.clone())).is_some_and(|resolved|matches!(resolved.target,Target::Class(class) if matches!(class.identity.0.as_str(),"abc"|"_py_abc") && class.name=="ABCMeta"));
            abstract_base || abstract_meta || name.ends_with("mixin")
                || name.starts_with("abstract")
                || name.starts_with("base")
                || name.ends_with("base")
                || !self
                    .abstract_methods(class.clone(), origin.clone(), true)
                    .is_empty()
        })
    }
    pub(super) fn check_subscript(&mut self, value: &ExprSubscript) {
        self.check_sequence_index(value);
        self.check_potential_index(value);
        if matches!(value.value.as_ref(), Expr::ListComp(_) | Expr::DictComp(_)) {
            return;
        }
        if matches!(value.value.as_ref(), Expr::Dict(_)) && !self.operand_hashable(&value.slice) {
            self.report_unhashable(&value.slice, "key", "dict", value.value.range());
        }
        let method = method(value);
        let (rule, symbol) = if value.ctx.is_store() {
            ("E1137", "unsupported-assignment-operation")
        } else if value.ctx.is_del() {
            ("E1138", "unsupported-delete-operation")
        } else {
            ("E1136", "unsubscriptable-object")
        };
        if !matches!(value.value.as_ref(), Expr::SetComp(_)) {
            if self.type_checking || self.subscript_abstract_context() {
                return;
            }
            let mut operand = self.operand(&value.value);
            if let Some(resolved) = self.expression_target(&value.value) {
                if let Target::Function(function) = resolved.target {
                    if let Some(decorator) = function.decorators.first() {
                        let Some(decorator) =
                            self.expression_target_in(decorator, resolved.context)
                        else {
                            return;
                        };
                        let Target::Class(class) = decorator.target else {
                            return;
                        };
                        operand = Operand::Instance(class, decorator.context);
                    }
                }
            }
            if let Operand::Instance(class, origin) | Operand::Class(class, origin) = &operand {
                if let Some(decorator) = class
                    .decorators
                    .first()
                    .filter(|_| class.identity.0 != "builtins")
                {
                    let Some(resolved) = self.expression_target_in(decorator, origin.clone())
                    else {
                        return;
                    };
                    let Target::Class(class) = resolved.target else {
                        return;
                    };
                    operand = Operand::Instance(class, resolved.context);
                }
            }
            if self.operand_supports_subscript(&operand, method) {
                return;
            }
        }
        let display = display::expression(&value.value, &self.source.text);
        let message = if value.ctx.is_load() {
            format!("Value '{display}' is unsubscriptable")
        } else {
            format!(
                "{} does not support item {}",
                super::string_format::python_string_repr(&display),
                if value.ctx.is_store() {
                    "assignment"
                } else {
                    "deletion"
                }
            )
        };
        self.emit(rule, symbol, &message, value.value.range());
    }
}
