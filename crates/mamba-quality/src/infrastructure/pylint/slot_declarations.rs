//! Slot declaration checks use each inferred object's original location.
use super::{
    classes::Class,
    resolution::Context,
    slot_model::{Slot, Slots},
    values::Value,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, Stmt, StmtClassDef};
use ruff_text_size::Ranged;
use std::{collections::BTreeSet, rc::Rc};

impl Checker<'_> {
    pub(super) fn check_slots(
        &mut self,
        statement: &StmtClassDef,
        class: Rc<Class>,
        context: Context,
    ) {
        let range = self.declaration_range(statement.name.range());
        for expression in &class.slot_check_values {
            for (inferred, origin) in self.slot_nodes(expression, context.clone()) {
                match self.slot_value(&inferred, origin.clone()) {
                    Slots::Invalid => {
                        self.emit("E0238", "invalid-slots", "Invalid __slots__ object", range)
                    }
                    Slots::String(_) | Slots::Bytes(_) => self.emit(
                        "C0205",
                        "single-string-used-for-slots",
                        "Class __slots__ should be a non-string iterable",
                        range,
                    ),
                    Slots::Elements(elements, origin) => {
                        let mut names = Vec::new();
                        for element in &elements {
                            if let Some(name) = self.slot_name(element, origin.clone()) {
                                if class.declared_members.contains(&name)
                                    && !class.annotation_only.contains(&name)
                                {
                                    self.emit_slot_origin(
                                        &origin,
                                        "E0242",
                                        "class-variable-slots-conflict",
                                        &format!(
                                            "Value '{name}' in slots conflicts with class variable"
                                        ),
                                        element.range(),
                                    );
                                }
                                names.push(name);
                            } else {
                                let value = self.expression_target_in(element, origin.clone());
                                if value.is_none()
                                    || matches!(value.map(|value| value.target), Some(super::callables::Target::Value(value)) if matches!(self.expand_value((*value).clone(), &origin, &mut BTreeSet::new()), Value::Unknown))
                                {
                                    continue;
                                }
                                let module = match &origin {
                                    Context::Module(module) => module.as_str(),
                                    Context::Scope(_) => self.module_name,
                                };
                                let source = self
                                    .modules
                                    .source(self.source.project, module)
                                    .unwrap_or(&self.source.text);
                                let text = super::display::expression(element, source);
                                self.emit_slot_origin(&origin, "E0236", "invalid-slots-object", &format!("Invalid object {} in __slots__, must contain only non empty strings", super::base_rules::representation(&text)), element.range());
                            }
                        }
                        let mut inherited = BTreeSet::new();
                        for (ancestor, origin) in self
                            .lineage(class.clone(), context.clone())
                            .into_iter()
                            .skip(1)
                        {
                            if let Some(names) = self.layout_slots(ancestor, origin) {
                                inherited.extend(names);
                            }
                        }
                        let repeated: Vec<_> = names
                            .iter()
                            .filter(|name| inherited.contains(&Slot::Name((*name).clone())))
                            .map(|name| super::base_rules::representation(name))
                            .collect();
                        if !repeated.is_empty() {
                            self.emit_slot_origin(
                                &origin,
                                "W0244",
                                "redefined-slots-in-subclass",
                                &format!("Redefined slots [{}] in subclass", repeated.join(", ")),
                                inferred.range(),
                            );
                        }
                    }
                    Slots::Unknown => {}
                }
            }
        }
    }
    pub(super) fn check_declared_slots(
        &mut self,
        statement: &StmtClassDef,
        class: &Class,
        context: Context,
    ) {
        if !self.valid_slots(class, context.clone()) {
            return;
        }
        let mut names = self.own_slots(class, context.clone()).unwrap_or_default();
        if names.is_empty() || names.iter().any(|name| name.named("__dict__")) {
            return;
        }
        for base in &class.bases {
            if let Some(super::resolution::Resolved {
                target: super::callables::Target::Class(ancestor),
                context: origin,
            }) = self.declared_base(base, context.clone())
            {
                if !self.valid_slots(&ancestor, origin.clone()) {
                    return;
                }
                let inherited = self.own_slots(&ancestor, origin).unwrap_or_default();
                if inherited.iter().any(|name| name.named("__dict__")) {
                    return;
                }
                names.extend(inherited);
            }
        }
        for statement in &statement.body {
            let Stmt::AnnAssign(assignment) = statement else {
                continue;
            };
            let Expr::Name(name) = assignment.target.as_ref() else {
                continue;
            };
            if assignment.value.is_some()
                || (name.id.as_str() == "__slots__" && !class.slot_values.is_empty())
                || names.iter().any(|value| value.named(name.id.as_str()))
            {
                continue;
            }
            let annotation = match assignment.annotation.as_ref() {
                Expr::Subscript(value) => value.value.as_ref(),
                value => value,
            };
            if matches!(annotation, Expr::Name(name) if name.id.as_str() == "ClassVar")
                || matches!(annotation, Expr::Attribute(value) if value.attr.as_str() == "ClassVar")
            {
                continue;
            }
            self.emit(
                "E0245",
                "declare-non-slot",
                &format!("No such name '{}' in __slots__", name.id),
                name.range(),
            );
        }
    }
}
