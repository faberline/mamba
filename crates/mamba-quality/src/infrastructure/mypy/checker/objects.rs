//! Attribute errors and assignment types keep their distinct original spans.
use super::*;
use quality_syntax::python_ast::ExprAttribute;
mod descriptors;

impl Checker<'_> {
    pub(super) fn check_object_attribute(&mut self, attribute: &ExprAttribute) {
        let _ = self.check_object_attribute_binding(attribute);
    }

    pub(super) fn check_object_attribute_binding(
        &mut self,
        attribute: &ExprAttribute,
    ) -> Option<(types::Type, std::sync::Arc<signatures::Signature>)> {
        let environment = self.environment();
        if let Some((_, error)) = self.scopes.super_method_binding(attribute, &environment) {
            if let Some(message) = error {
                self.emit("misc", message, attribute);
            }
            None
        } else if let Some(ty) = self.infer(&attribute.value) {
            self.check_object_attribute_type(attribute, ty, None)
                .map(|signature| (ty, signature))
        } else {
            None
        }
    }

    fn check_object_attribute_type(
        &mut self,
        attribute: &ExprAttribute,
        ty: types::Type,
        union: Option<types::Type>,
    ) -> Option<std::sync::Arc<signatures::Signature>> {
        if let Some(node) = self.context.node(ty) {
            if let types::Node::Union(items) = node.as_ref() {
                for &item in items {
                    let _ = self.check_object_attribute_type(
                        attribute,
                        item,
                        Some(union.unwrap_or(ty)),
                    );
                }
                return None;
            }
            if union.is_none()
                && attribute.ctx.is_load()
                && matches!(node.as_ref(),
                    types::Node::Container(types::Container::Dict, arguments)
                        if arguments.len() == 2)
                && !(attribute.attr.starts_with("__") && attribute.attr.ends_with("__"))
            {
                // This first query may load the locked target declarations.
                // The subsequent presence proof never loads or repairs metadata.
                let absent = self
                    .context
                    .runtime_builtin_class_instance("dict", self.python_target)
                    .and_then(|declared| {
                        self.context.builtin_dictionary_member_presence(
                            declared,
                            attribute.attr.as_str(),
                            self.python_target,
                        )
                    })
                    == Some(false);
                if absent {
                    self.emit_missing_object_attribute(
                        format!(
                            "\"{}\" has no attribute \"{}\"",
                            self.context.name(ty),
                            attribute.attr
                        ),
                        attribute,
                        None,
                    );
                    return None;
                }
            }
        }
        if union.is_some() || ty == types::Type::None {
            if let Some(node) = self.context.node(ty) {
                match node.as_ref() {
                    types::Node::Literal(value) | types::Node::LiteralValue(value) => {
                        let _ = self.check_object_attribute_type(attribute, value.base(), union);
                        return None;
                    }
                    types::Node::Container(
                        kind @ (types::Container::List | types::Container::Dict),
                        _,
                    ) => {
                        let name = attribute.attr.as_str();
                        if *kind == types::Container::Dict
                            && name.starts_with("__")
                            && name.ends_with("__")
                        {
                            return None;
                        }
                        let builtin = if *kind == types::Container::List {
                            "list"
                        } else {
                            "dict"
                        };
                        let declared = self
                            .context
                            .runtime_builtin_class_instance(builtin, self.python_target);
                        let known = self
                            .context
                            .missing_builtin_attribute(
                                types::Type::Object,
                                name,
                                self.python_target,
                            )
                            .is_none()
                            || self
                                .context
                                .class_method_for(ty, name, self.python_target)
                                .is_some()
                            || declared.is_some_and(|instance| {
                                self.context.class_field(instance, name).is_some()
                                    || self
                                        .context
                                        .class_method_for(instance, name, self.python_target)
                                        .is_some()
                            });
                        if declared.is_some() && !known {
                            self.emit_missing_object_attribute(
                                format!(
                                    "\"{}\" has no attribute \"{name}\"",
                                    self.context.name(ty)
                                ),
                                attribute,
                                union,
                            );
                        }
                        return None;
                    }
                    types::Node::Container(types::Container::Sequence, arguments)
                        if arguments.len() == 1 =>
                    {
                        let name = attribute.attr.as_str();
                        if name.starts_with("__") && name.ends_with("__") {
                            return None;
                        }
                        let declared = self
                            .context
                            .builtin_value_class_instance("typing.Sequence", self.python_target)?;
                        let id = self.context.class_id(declared)?;
                        let Ok(lineage) = self.context.class_mro(id) else {
                            return None;
                        };
                        if self.context.class(id).generic.len() != 1
                            || lineage.iter().any(|&id| {
                                let class = self.context.class(id);
                                class.plugin || !class.any_base_origins.is_empty()
                            })
                            || self
                                .context
                                .class_member_owner(declared, "__getattr__")
                                .is_some()
                        {
                            return None;
                        }
                        let known = self
                            .context
                            .missing_builtin_attribute(
                                types::Type::Object,
                                name,
                                self.python_target,
                            )
                            .is_none()
                            || self.context.class_member_owner(declared, name).is_some();
                        if !known {
                            self.emit_missing_object_attribute(
                                format!(
                                    "\"{}\" has no attribute \"{name}\"",
                                    self.context.name(ty)
                                ),
                                attribute,
                                union,
                            );
                        }
                        return None;
                    }
                    _ => {}
                }
            }
            if ty == types::Type::None {
                let name = attribute.attr.as_str();
                // None has no public members beyond object. Leave special methods
                // on their existing path until their declarations are modeled.
                if !(name.starts_with("__") && name.ends_with("__"))
                    && self
                        .context
                        .missing_builtin_attribute(types::Type::Object, name, self.python_target)
                        .is_some()
                {
                    self.emit_missing_object_attribute(
                        format!("\"None\" has no attribute \"{name}\""),
                        attribute,
                        union,
                    );
                }
                return None;
            }
        }
        if matches!(
            self.context.node(ty).as_deref(),
            Some(types::Node::Callable(_))
        ) {
            if self
                .context
                .builtin_constructor_member(ty, attribute.attr.as_str(), self.python_target)
                .is_some()
            {
                return None;
            }
            if self
                .context
                .callable_attribute(ty, attribute.attr.as_str())
                .is_none()
            {
                let name = self.context.name(ty);
                let display = if name == "overloaded function" {
                    name
                } else {
                    format!("\"{name}\"")
                };
                let (rule, message) = if attribute.attr.as_str() == "__call__" {
                    ("operator", format!("{display} not callable"))
                } else {
                    (
                        "attr-defined",
                        format!("{display} has no attribute \"{}\"", attribute.attr),
                    )
                };
                if rule == "attr-defined" {
                    self.emit_missing_object_attribute(message, attribute, union);
                } else {
                    self.emit(rule, message, attribute);
                }
            }
            return None;
        }
        let Some(id) = self.context.class_id(ty) else {
            if let Some(message) = self.context.missing_builtin_attribute(
                ty,
                attribute.attr.as_str(),
                self.python_target,
            ) {
                self.emit_missing_object_attribute(message, attribute, union);
            }
            return None;
        };
        if attribute.attr.as_str() == "__init__" && !self.context.is_class_value(ty) {
            self.emit("misc", "Accessing \"__init__\" on an instance is unsound, since instance.__init__ could be from an incompatible subclass".into(), attribute);
            return None;
        }
        if attribute.ctx.is_load()
            && self.context.is_class_value(ty)
            && self.context.class_method_kind(ty, attribute.attr.as_str())
                == Some(super::super::classes::MethodKind::Property)
        {
            return None;
        }
        if attribute.ctx.is_store()
            && self
                .context
                .class_field(ty, attribute.attr.as_str())
                .is_none()
            && self
                .context
                .class_method(ty, attribute.attr.as_str())
                .is_some()
        {
            self.emit(
                "method-assign",
                "Cannot assign to a method".into(),
                attribute,
            );
        }
        let field = if attribute.ctx.is_store() && union.is_none() {
            self.context
                .class_field_for_store(ty, attribute.attr.as_str(), self.python_target)
        } else {
            self.context.class_field(ty, attribute.attr.as_str())
        };
        let mut plain_method = field.is_none();
        if let Some(field) = field {
            if attribute.ctx.is_store()
                && self.context.is_class_value(ty)
                && self
                    .context
                    .class_member_owner(ty, attribute.attr.as_str())
                    .is_some_and(|owner| {
                        self.context
                            .class(owner)
                            .decorated_fields
                            .contains(attribute.attr.as_str())
                    })
            {
                self.emit(
                    "method-assign",
                    "Cannot assign to a method".into(),
                    attribute,
                );
            }
            if self.context.is_class_value(ty)
                && self
                    .context
                    .class(id)
                    .dataclass
                    .as_ref()
                    .is_some_and(|definition| {
                        definition.generated_slots
                            && definition
                                .attributes
                                .iter()
                                .any(|field| field.name == attribute.attr.as_str())
                    })
            {
                self.emit(
                    "misc",
                    format!(
                        "\"{}\" in __slots__ conflicts with class variable access",
                        attribute.attr
                    ),
                    attribute,
                );
            }
            let via_instance = self.context.member_is_instance(ty, attribute.attr.as_str());
            if field.class_var && attribute.ctx.is_store() && via_instance {
                self.emit(
                    "misc",
                    format!(
                        "Cannot assign to class variable \"{}\" via instance",
                        attribute.attr
                    ),
                    attribute,
                );
            }
            let outside_slots = self
                .context
                .classes
                .borrow()
                .embedded_class_slots
                .get(&id)
                .is_some_and(|slots| !slots.contains(attribute.attr.as_str()));
            if attribute.ctx.is_store()
                && !self.context.is_class_value(ty)
                && outside_slots
                && self.context.class_method_kind(ty, attribute.attr.as_str())
                    != Some(super::super::classes::MethodKind::Property)
            {
                self.emit(
                    "misc",
                    format!(
                        "Trying to assign name \"{}\" that is not in \"__slots__\" of type \"{}\"",
                        attribute.attr,
                        self.context.revealed_name(ty)
                    ),
                    attribute,
                );
            }
            if attribute.ctx.is_store()
                && !self.context.is_class_value(ty)
                && self
                    .context
                    .class(id)
                    .dataclass
                    .as_ref()
                    .is_some_and(|definition| {
                        definition.generated_slots
                            && !definition
                                .attributes
                                .iter()
                                .any(|field| field.name == attribute.attr.as_str())
                    })
            {
                self.emit(
                    "misc",
                    format!(
                        "Trying to assign name \"{}\" that is not in \"__slots__\" of type \"{}\"",
                        attribute.attr,
                        self.context.revealed_name(ty)
                    ),
                    attribute,
                );
            }
            if field.readonly
                && attribute.ctx.is_store()
                && !self.final_initializers.contains(&attribute.start())
            {
                let owner = self
                    .context
                    .class_member_owner(ty, attribute.attr.as_str())
                    .unwrap_or(id);
                let message = if field.final_attribute {
                    format!("Cannot assign to final attribute \"{}\"", attribute.attr)
                } else {
                    format!(
                        "Property \"{}\" defined in \"{}\" is read-only",
                        attribute.attr,
                        self.context.class(owner).name
                    )
                };
                self.emit("misc", message, attribute);
            }
            if attribute.ctx.is_load() && self.context.is_class_value(ty) && !field.class_access {
                self.emit(
                    "misc",
                    format!(
                        "Cannot access instance-only attribute \"{}\" on class object",
                        attribute.attr
                    ),
                    attribute,
                );
            }
        } else if self
            .context
            .class_method(ty, attribute.attr.as_str())
            .is_none()
        {
            plain_method = false;
            let class = self.context.class(id);
            let display_ty = if self.context.is_class_value(ty)
                && !class.generic.is_empty()
                && self
                    .context
                    .node(ty)
                    .is_some_and(|node| matches!(node.as_ref(), types::Node::Class(_)))
            {
                self.context.applied_class(id, class.generic.clone(), true)
            } else {
                ty
            };
            if let Some(symbol) =
                super::super::classes::dataclass::order_symbol(attribute.attr.as_str())
            {
                self.emit(
                    "operator",
                    format!(
                        "Unsupported left operand type for {symbol} (\"{}\")",
                        self.context.name(display_ty)
                    ),
                    attribute,
                );
                return None;
            }
            if attribute.attr.as_str() == "__contains__" {
                self.emit(
                    "operator",
                    format!(
                        "Unsupported right operand type for in (\"{}\")",
                        self.context.name(display_ty)
                    ),
                    attribute,
                );
                return None;
            }
            let message = if attribute.ctx.is_store()
                && union.is_none()
                && self.context.is_module_type_instance(ty, self.python_target)
            {
                format!("Module has no attribute \"{}\"", attribute.attr)
            } else {
                format!(
                    "\"{}\" has no attribute \"{}\"{}",
                    self.context.name(display_ty),
                    attribute.attr,
                    match attribute.attr.as_str() {
                        "__iter__" => " (not iterable)",
                        "__aiter__" => " (not async iterable)",
                        _ => "",
                    }
                )
            };
            self.emit_missing_object_attribute(message, attribute, union);
        }
        if attribute.ctx.is_load() {
            let name = attribute.attr.as_str();
            // Preserve the signature and error from this existing binding.
            let binding = self
                .context
                .class_method_call_binding_for(ty, name, self.python_target);
            let mut checked = None;
            if let Some((signature, error)) = binding {
                if let Some(message) = error {
                    self.emit("misc", message, attribute);
                } else if plain_method && union.is_none() {
                    checked = Some(signature);
                }
            }
            self.check_descriptor_access(attribute, ty);
            return checked;
        }
        None
    }

    fn emit_missing_object_attribute(
        &mut self,
        message: String,
        attribute: &ExprAttribute,
        union: Option<types::Type>,
    ) {
        if let Some(union) = union {
            // Keep the branch's display name, suffix, suggestions, and span.
            // Only the diagnostic context changes from a single type to a union item.
            let (item, member) = message
                .rsplit_once(" has no attribute ")
                .expect("missing-attribute diagnostic shape");
            self.emit(
                "union-attr",
                format!(
                    "Item {item} of \"{}\" has no attribute {member}",
                    self.context.name(union)
                ),
                attribute,
            );
        } else {
            self.emit("attr-defined", message, attribute);
        }
    }
}
