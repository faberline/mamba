//! Class checks retain method declarations and receiver-specific diagnostics.
use super::{
    callables::Target,
    classes::{method_kind, MethodKind},
    defaults,
    resolution::Context,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    ExprAttribute, StmtClassDef, StmtFunctionDef,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeSet;

impl Checker<'_> {
    pub(super) fn enter_class(&mut self, class: &StmtClassDef) {
        let mut model =
            super::classes::Class::from_statement(class, self.module_name, self.is_package);
        self.add_dataclass_design_fields(class, &mut model);
        self.modules.apply_class_stores(&mut model);
        self.build_attributes.apply(&mut model);
        let model = std::rc::Rc::new(model);
        self.check_class_design(
            class,
            model.clone(),
            Context::Scope(self.scopes.current_index()),
        );
        self.check_instance_attribute_names(
            model.clone(),
            Context::Scope(self.scopes.current_index()),
        );
        self.check_inherited_abstract_methods(
            class,
            model.clone(),
            Context::Scope(self.scopes.current_index()),
        );
        self.class_stack
            .push((model, Context::Scope(self.scopes.current_index())));
    }

    pub(super) fn declaration_range(&self, name: TextRange) -> TextRange {
        if let Some(range) = self.declaration_locations.range(name) {
            return range;
        }
        let before = &self.source.text[..name.start().to_usize()];
        let line = before.rfind('\n').map_or(0, |offset| offset + 1);
        let indent = before[line..].len() - before[line..].trim_start().len();
        TextRange::new(
            ruff_text_size::TextSize::try_from(line + indent).unwrap(),
            name.end(),
        )
    }

    pub(super) fn check_declaration_name(&mut self, name: &str, category: &str, range: TextRange) {
        if self.check_bad_name(name, range) {
            return;
        }
        if !defaults::fixed().valid_name(name, category) {
            let (label, style) = if category == "class" {
                ("Class", "PascalCase")
            } else if category == "function" {
                ("Function", "snake_case")
            } else if category == "attr" {
                ("Attribute", "snake_case")
            } else if category == "variable" {
                ("Variable", "snake_case")
            } else {
                ("Method", "snake_case")
            };
            self.emit(
                "C0103",
                "invalid-name",
                &format!("{label} name \"{name}\" doesn't conform to {style} naming style"),
                range,
            );
        }
    }

    pub(super) fn leave_class(&mut self, statement: &StmtClassDef) {
        self.check_private_methods(statement);
        self.check_private_fields(statement);
        let Some((class, origin)) = self.class_stack.pop() else {
            return;
        };
        self.check_member_access(statement, class.clone(), origin.clone());
        self.check_attribute_definitions(class.clone(), origin.clone());
        let exempt = self.exempt_public_minimum(statement, class.clone(), origin.clone());
        let lineage = self.lineage(class.clone(), origin);
        let mut methods = BTreeSet::new();
        for (class, _) in lineage {
            methods.extend(
                class
                    .methods
                    .iter()
                    .filter(|(name, method)| {
                        class.design_methods.contains(*name)
                            && !name.starts_with('_')
                            && !(class.identity.0 == "builtins"
                                && method.kind == MethodKind::Property)
                    })
                    .map(|(name, _)| name.clone()),
            );
        }
        let special = class
            .design_methods
            .iter()
            .filter(|name| {
                name.strip_prefix("__")
                    .and_then(|name| name.strip_suffix("__"))
                    .is_some_and(|name| {
                        !name.is_empty() && name.bytes().all(|value| value.is_ascii_lowercase())
                    })
                    && name.as_str() != "__init__"
            })
            .count();
        let count = methods.len() + special;
        let (min, max) = defaults::fixed().public_limits();
        let own = class
            .design_methods
            .iter()
            .filter(|name| !name.starts_with('_'))
            .count();
        let range = self.declaration_range(statement.name.range());
        if own > max {
            self.emit(
                "R0904",
                "too-many-public-methods",
                &format!("Too many public methods ({own}/{max})"),
                range,
            );
        }
        if !exempt && count < min {
            self.emit(
                "R0903",
                "too-few-public-methods",
                &format!("Too few public methods ({count}/{min})"),
                range,
            );
        }
    }

    pub(super) fn declaration_method_kind(&self, function: &StmtFunctionDef) -> Option<MethodKind> {
        if !self.scopes.in_class() {
            return None;
        }
        let kind = self.inferred_method_kind(
            function
                .decorator_list
                .iter()
                .map(|value| &value.expression),
            Context::Scope(self.scopes.current_index()),
            method_kind(function),
        );
        let kind = self
            .class_stack
            .last()
            .and_then(|(class, _)| class.methods.get(function.name.as_str()))
            .filter(|method| method.kind != MethodKind::Instance)
            .map_or(kind, |method| method.kind);
        Some(kind)
    }

    pub(super) fn check_method(&mut self, function: &StmtFunctionDef) -> Option<MethodKind> {
        let kind = self.declaration_method_kind(function)?;
        self.check_special_return(function);
        let first = function
            .parameters
            .posonlyargs
            .first()
            .or_else(|| function.parameters.args.first())
            .map(|parameter| parameter.parameter.name.as_str());
        self.check_special_signature(function, kind);
        self.check_property_parameters(function, kind);
        let range = self.declaration_range(function.name.range());
        self.check_declaration_name(
            function.name.as_str(),
            if kind == MethodKind::Property {
                "attr"
            } else {
                "method"
            },
            range,
        );
        let metaclass = self.class_stack.last().is_some_and(|(class, origin)| {
            self.lineage(class.clone(), origin.clone())
                .iter()
                .any(|(class, _)| class.identity.0 == "builtins" && class.name == "type")
        });
        match kind {
            MethodKind::Static => {
                if first.is_some_and(|name| {
                    name == "self"
                        || defaults::fixed().class_receiver(name)
                        || defaults::fixed().metaclass_receiver(name)
                }) {
                    self.emit(
                        "W0211",
                        "bad-staticmethod-argument",
                        &format!("Static method with '{}' as first argument", first.unwrap()),
                        range,
                    );
                }
            }
            _ if first.is_none()
                && function.parameters.vararg.is_none()
                && function.parameters.kwarg.is_none() =>
            {
                self.emit(
                    "E0211",
                    "no-method-argument",
                    &format!("Method '{}' has no argument", function.name),
                    range,
                );
            }
            MethodKind::Class if metaclass => {
                if !first.is_some_and(|name| defaults::fixed().metaclass_receiver(name)) {
                    self.emit(
                        "C0204",
                        "bad-mcs-classmethod-argument",
                        &format!(
                            "Metaclass class method {} should have {} as first argument",
                            function.name,
                            defaults::fixed().metaclass_receiver_names()
                        ),
                        range,
                    );
                }
            }
            _ if metaclass => {
                if !first.is_some_and(|name| defaults::fixed().class_receiver(name)) {
                    self.emit(
                        "C0203",
                        "bad-mcs-method-argument",
                        &format!(
                            "Metaclass method {} should have 'cls' as first argument",
                            function.name
                        ),
                        range,
                    );
                }
            }
            MethodKind::Class
                if !first.is_some_and(|name| defaults::fixed().class_receiver(name)) =>
            {
                self.emit(
                    "C0202",
                    "bad-classmethod-argument",
                    &format!(
                        "Class method {} should have 'cls' as first argument",
                        function.name
                    ),
                    range,
                );
            }
            MethodKind::Instance | MethodKind::Property if first != Some("self") => {
                self.emit(
                    "E0213",
                    "no-self-argument",
                    &format!(
                        "Method '{}' should have \"self\" as first argument",
                        function.name
                    ),
                    range,
                );
            }
            _ => {}
        }
        Some(kind)
    }

    pub(super) fn bind_receiver(
        &mut self,
        function: &StmtFunctionDef,
        method: Option<MethodKind>,
    ) -> Option<String> {
        let kind = method?;
        if kind == MethodKind::Static {
            return None;
        }
        let (class, origin) = self.class_stack.last()?.clone();
        let name = function.parameters.iter().next()?.name().to_string();
        let target = if kind == MethodKind::Class {
            Target::Contextual(Box::new(Target::Class(class)), origin)
        } else {
            Target::Instance(class, origin)
        };
        self.scopes.receiver(&name, target);
        Some(name)
    }

    pub(super) fn check_object_member(&mut self, attribute: &ExprAttribute) {
        if self.native_member_guarded(attribute) {
            return;
        }
        if attribute.attr.as_str() == "__class__"
            && self
                .expression_target(
                    &mamba_language::application::quality_syntax::python_ast::Expr::Attribute(
                        attribute.clone(),
                    ),
                )
                .is_some()
        {
            return;
        }
        let Some(resolved) = self.expression_target(&attribute.value) else {
            return;
        };
        let (class, origin, instance) = match resolved.target {
            Target::Function(function) => {
                self.check_function_member(attribute, &function, &resolved.context);
                return;
            }
            Target::Class(class) => (class, resolved.context, false),
            Target::Instance(class, origin) => (class, origin, true),
            Target::Value(value) => {
                let name = match value.as_ref() {
                    super::values::Value::Sequence(_, name) => Some(*name),
                    super::values::Value::Mapping(_) => Some("dict"),
                    _ => value.builtin_name(),
                };
                let Some(name) = name else {
                    return;
                };
                let Some(class) = self.modules.builtin_class(name) else {
                    return;
                };
                (class, Context::Module("builtins".into()), true)
            }
            Target::Super(proxy) => {
                if self.super_member(&proxy, attribute.attr.as_str()).is_none() {
                    self.emit(
                        "E1101",
                        "no-member",
                        &format!(
                            "Super of '{}' has no '{}' member",
                            proxy.name, attribute.attr
                        ),
                        attribute.range(),
                    );
                }
                return;
            }
            _ => return,
        };
        let self_reference = instance
            && class
                .attribute_expressions
                .get(attribute.attr.as_str())
                .is_some_and(|value| value.range() == attribute.range())
            && class
                .attribute_definitions
                .iter()
                .filter(|value| value.name == attribute.attr.as_str())
                .count()
                == 1;
        if (!self_reference
            && self
                .class_member(
                    class.clone(),
                    origin.clone(),
                    attribute.attr.as_str(),
                    instance,
                )
                .is_some())
            || self.source_class_scope_member(&class, attribute.attr.as_str(), instance)
            || (instance && self.dynamic_member(class.clone(), origin))
            || (class.identity.0 == "builtins" && matches!(class.name.as_str(), "super" | "type"))
            || (class.identity.0 == "builtins"
                && super::builtin_members::exists(
                    &class.name,
                    attribute.attr.as_str(),
                    self.modules.target(),
                ))
        {
            return;
        }
        let kind = if instance { "Instance of" } else { "Class" };
        let hint = if class.identity.0 == "builtins" {
            super::builtin_members::hint(
                &class.name,
                attribute.attr.as_str(),
                self.modules.target(),
            )
        } else {
            String::new()
        };
        self.emit(
            "E1101",
            "no-member",
            &format!(
                "{kind} '{}' has no '{}' member{hint}",
                class.name, attribute.attr
            ),
            attribute.range(),
        );
    }
}
