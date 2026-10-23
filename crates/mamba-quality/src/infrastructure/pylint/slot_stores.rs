//! Attribute stores check slots before validating assigned class objects.
use super::{callables::Target, classes::MethodKind, resolution::Context, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprAttribute};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn check_assignment_slots(&mut self, target: &Expr) {
        match target {
            Expr::Attribute(value) => self.check_slot_store(value),
            Expr::Tuple(value) => {
                for value in &value.elts {
                    self.check_assignment_slots(value);
                }
            }
            Expr::List(value) => {
                for value in &value.elts {
                    self.check_assignment_slots(value);
                }
            }
            Expr::Starred(value) => self.check_assignment_slots(&value.value),
            _ => {}
        }
    }
    pub(super) fn check_slot_store(&mut self, attribute: &ExprAttribute) {
        if !self.checked_slot_stores.insert(attribute.start().to_u32()) {
            return;
        }
        self.check_slot_attribute(attribute);
        if attribute.attr.as_str() != "__class__" {
            return;
        }
        let Some(assignment) = self.assigned_attributes.get(&attribute.start().to_u32()) else {
            return;
        };
        let context = Context::Scope(self.scopes.current_index());
        let value = assignment.value.clone();
        let starred = assignment.starred;
        let Some((value, context)) = self.slot_nodes(&value, context).into_iter().next() else {
            return;
        };
        let Some(resolved) = self.expression_target_in(&value, context.clone()) else {
            return;
        };
        let category = if starred {
            "List"
        } else {
            match resolved.target {
                Target::Class(_) => return,
                Target::Instance(class, _) if class.identity.0 == "builtins" => {
                    match class.name.as_str() {
                        "list" => "List",
                        "tuple" => "Tuple",
                        "dict" => "Dict",
                        "set" => "Set",
                        "str" | "bytes" | "int" | "float" | "bool" | "complex" => "Const",
                        _ => "Instance",
                    }
                }
                Target::Instance(_, _) => "Instance",
                Target::Function(function) if function.category == "lambda" => "Lambda",
                Target::Function(_) => "FunctionDef",
                Target::Module(_) => "Module",
                Target::Value(_) => match self.slot_scalar(&value, context) {
                    Value::Unknown => return,
                    Value::Sequence(_, "list") => "List",
                    Value::Sequence(_, "tuple") => "Tuple",
                    Value::Sequence(_, "set") => "Set",
                    Value::Mapping(_) => "Dict",
                    Value::String(_)
                    | Value::Bytes(_)
                    | Value::Number(_)
                    | Value::Boolean(_)
                    | Value::Scalar(_) => "Const",
                    _ => return,
                },
                _ => return,
            }
        };
        self.emit("E0243", "invalid-class-object", &format!("Invalid assignment to '__class__'. Should be a class definition but got a '{category}'"), attribute.range());
    }
    fn check_slot_attribute(&mut self, attribute: &ExprAttribute) {
        let Some(resolved) = self.expression_target(&attribute.value) else {
            return;
        };
        let Target::Instance(class, origin) = resolved.target else {
            return;
        };
        if class.slot_values.is_empty() {
            return;
        }
        let lineage = self.lineage(class.clone(), origin.clone());
        if lineage.iter().any(|(class, _)| {
            class.members.contains_key("__setattr__")
                && !(class.identity.0 == "builtins" && class.name == "object")
        }) {
            return;
        }
        let name = attribute.attr.as_str();
        if let Some(resolved) = self.class_member(class.clone(), origin.clone(), name, false) {
            let resolved = match resolved.target {
                Target::Construct(call) => {
                    self.expression_target_in(&Expr::Call((*call).clone()), resolved.context)
                }
                Target::Reference(names) => self.reference_in(names, resolved.context),
                target => Some(super::resolution::Resolved {
                    target,
                    context: resolved.context,
                }),
            };
            if let Some(super::resolution::Resolved {
                target: Target::Instance(descriptor, context),
                ..
            }) = resolved
            {
                if self
                    .class_member(descriptor.clone(), context.clone(), "__get__", true)
                    .is_some()
                    && self
                        .class_member(descriptor, context, "__set__", true)
                        .is_some()
                {
                    return;
                }
            }
        }
        let Some(slots) = self.layout_slots(class.clone(), origin.clone()) else {
            return;
        };
        if slots
            .iter()
            .any(|slot| slot.named(name) || slot.named("__dict__"))
        {
            return;
        }
        if lineage.iter().any(|(class, context)| {
            class.methods.get(name).is_some_and(|method| {
                self.inferred_method_kind(
                    &method.signature.decorators,
                    context.clone(),
                    method.kind,
                ) == MethodKind::Property
            }) || class.annotation_only.contains(name)
                || (name != "__class__" && class.declared_members.contains(name))
        }) {
            return;
        }
        if name == "__class__" {
            let Some(assignment) = self
                .assigned_attributes
                .get(&attribute.start().to_u32())
                .filter(|assignment| assignment.direct)
            else {
                return;
            };
            let value = assignment.value.clone();
            let Some((value, value_context)) = self
                .slot_nodes(&value, Context::Scope(self.scopes.current_index()))
                .into_iter()
                .next()
            else {
                return;
            };
            if let Some(super::resolution::Resolved {
                target: Target::Class(replacement),
                context,
            }) = self.expression_target_in(&value, value_context)
            {
                if self
                    .layout_slots(replacement, context)
                    .is_some_and(|layout| layout == slots)
                {
                    return;
                }
            }
        }
        self.emit(
            "E0237",
            "assigning-non-slot",
            &format!("Assigning to attribute '{name}' not defined in class slots"),
            attribute.range(),
        );
    }
}
