//! Override checks preserve the original argument, return, and signature diagnostics.
mod fields;
mod format;
mod multiple;
mod post_init;
mod shape;
use super::super::classes::{Method, MethodKind};
use super::*;
use quality_syntax::python_ast::StmtFunctionDef;
use signatures::{Kind, Signature};
use types::Type;

impl Checker<'_> {
    pub(super) fn check_method_override(&mut self, function: &StmtFunctionDef) {
        let name = function.name.as_str();
        if name.starts_with("__") && !name.ends_with("__") {
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
        let class = self.context.class(id);
        let Some(method) = class.methods.get(name) else {
            return;
        };
        self.check_inherited_field_method(function, id, method);
        self.check_dataclass_post_init(function, &class, method);
        let mut found = false;
        let mut argument_hint = false;
        for base in self.context.class_lineage(id).into_iter().skip(1) {
            let parent = self.context.class(base);
            let Some(original) = parent.methods.get(name) else {
                continue;
            };
            found = true;
            if original.final_method {
                let range = if self.scopes.is_overload(function) {
                    ruff_text_size::TextRange::at(
                        function.decorator_list.first().unwrap().expression.start(),
                        ruff_text_size::TextSize::new(1),
                    )
                } else {
                    function
                        .decorator_list
                        .first()
                        .map_or(function.range(), |decorator| {
                            ruff_text_size::TextRange::new(
                                decorator.expression.start(),
                                function.end(),
                            )
                        })
                };
                self.emit("misc", format!("Cannot override final attribute \"{name}\" (previously declared in base class \"{}\")", parent.name), &range);
            }
            if matches!(
                name,
                "__init__" | "__new__" | "__init_subclass__" | "__post_init__"
            ) {
                continue;
            }
            let parent_name = if parent.namespace == class.namespace {
                parent.name.clone()
            } else {
                let module = parent
                    .namespace
                    .split_once(':')
                    .map_or(parent.namespace.as_str(), |(_, module)| module);
                format!(
                    "{}.{}",
                    if module == "<builtins>" {
                        "builtins"
                    } else {
                        module
                    },
                    parent.name
                )
            };
            let original = self
                .context
                .specialize_method(original, class.instance, base);
            if method.kind == MethodKind::Property
                && original.kind == MethodKind::Property
                && method.setter.is_none()
                && !class.cached_properties.contains(name)
                && (original.setter.is_some() || parent.cached_properties.contains(name))
            {
                let range = function.decorator_list.first().map_or_else(
                    || self.function_range(function),
                    |decorator| {
                        ruff_text_size::TextRange::new(decorator.expression.start(), function.end())
                    },
                );
                self.emit(
                    "misc",
                    "Read-only property cannot override read-write property".into(),
                    &range,
                );
            }
            self.check_override_types(
                function,
                method,
                &original,
                &parent_name,
                &mut argument_hint,
            );
        }
        if !found && matches!(name, "__eq__" | "__ne__") {
            let object = self.context.class(self.context.builtin_class(Type::Object));
            if let Some(original) = object.methods.get(name) {
                found = true;
                self.check_override_types(
                    function,
                    method,
                    original,
                    "builtins.object",
                    &mut argument_hint,
                );
            }
        }
        if !found
            && function
                .decorator_list
                .iter()
                .any(|decorator| match &decorator.expression {
                    Expr::Name(name) => name.id.as_str() == "override",
                    Expr::Attribute(attribute) => attribute.attr.as_str() == "override",
                    _ => false,
                })
        {
            self.emit("misc", format!("Method \"{name}\" is marked as an override, but no base method was found with this name"), &self.function_range(function));
        }
    }

    fn check_override_types(
        &mut self,
        function: &StmtFunctionDef,
        method: &Method,
        original: &Method,
        parent: &str,
        argument_hint: &mut bool,
    ) {
        let name = function.name.as_str();
        if method.kind == MethodKind::Property || original.kind == MethodKind::Property {
            if method.kind == original.kind
                && self.context.accepts(
                    original.signature.returns.unwrap_or(Type::Any),
                    method.signature.returns.unwrap_or(Type::Any),
                )
            {
                return;
            }
            self.override_signature(function, method, original, parent);
            return;
        }
        let post_init = parent == "dataclass" && name == "__post_init__";
        let child = method
            .signature
            .bound(usize::from(!post_init && method.kind != MethodKind::Static));
        let base = original.signature.bound(usize::from(
            !post_init && original.kind != MethodKind::Static,
        ));
        let changed_binding = matches!(original.kind, MethodKind::Static | MethodKind::Class)
            && !matches!(method.kind, MethodKind::Static | MethodKind::Class);
        if !changed_binding && shape::compatible(&self.context, &base, &child) {
            return;
        }
        if !base.overloads.is_empty() || !child.overloads.is_empty() {
            if !base.overloads.is_empty() && !child.overloads.is_empty() {
                let order: Vec<_> = child
                    .overloads
                    .iter()
                    .filter_map(|child| {
                        base.overloads
                            .iter()
                            .position(|parent| self.context.accepts_callable(parent, child, true))
                    })
                    .collect();
                if order.len() == base.overloads.len()
                    && order.windows(2).any(|pair| pair[0] > pair[1])
                {
                    self.emit_with_hint("override", format!("Signature of \"{name}\" incompatible with supertype \"{parent}\""), &ruff_text_size::TextRange::at(function.decorator_list.first().unwrap().expression.start(), ruff_text_size::TextSize::new(1)), Some(format!("Overload variants must be defined in the same order as they are in \"{parent}\"")));
                    return;
                }
            }
            self.override_signature(function, method, original, parent);
            return;
        }
        let mut detailed = false;
        if child.parameters.len() == base.parameters.len()
            && shape::minimum_positional(&child) == shape::minimum_positional(&base)
        {
            for (index, parameter) in child.parameters.iter().enumerate() {
                let original = if matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword
                ) && matches!(
                    base.parameters[index].kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword
                ) {
                    Some(&base.parameters[index])
                } else if parameter.kind == Kind::KeywordOnly {
                    base.parameters.iter().find(|original| {
                        original.kind == Kind::KeywordOnly && original.name == parameter.name
                    })
                } else {
                    None
                };
                let Some(original) = original else { continue };
                let expected = original.ty.unwrap_or(Type::Any);
                if !self
                    .context
                    .accepts(parameter.ty.unwrap_or(Type::Any), expected)
                {
                    let offset = usize::from(!post_init && method.kind != MethodKind::Static);
                    let argument = function.parameters.iter().nth(index + offset).unwrap();
                    self.emit_with_hint("override", format!("Argument {} of \"{name}\" is incompatible with supertype \"{parent}\"; supertype defines the argument type as \"{}\"", index + 1, self.context.name(expected)), &argument,
                        (!*argument_hint).then(|| "This violates the Liskov substitution principle\nSee https://mypy.readthedocs.io/en/stable/common_issues.html#incompatible-overrides".into()));
                    *argument_hint = true;
                    detailed = true;
                }
            }
            let expected = base.returns.unwrap_or(Type::Any);
            let actual = child.returns.unwrap_or(Type::Any);
            if !self.context.accepts(expected, actual) {
                self.emit("override", format!("Return type \"{}\" of \"{name}\" incompatible with return type \"{}\" in supertype \"{parent}\"", self.context.name(actual), self.context.name(expected)), &self.function_range(function));
                detailed = true;
            }
        }
        if !detailed {
            self.override_signature(function, method, original, parent);
        }
    }

    fn override_signature(
        &mut self,
        function: &StmtFunctionDef,
        method: &Method,
        original: &Method,
        parent: &str,
    ) {
        let range = if !method.signature.overloads.is_empty() && self.scopes.is_overload(function) {
            ruff_text_size::TextRange::at(
                function.decorator_list.first().unwrap().expression.start(),
                ruff_text_size::TextSize::new(1),
            )
        } else {
            self.function_range(function)
        };
        self.emit_with_hint(
            "override",
            format!(
                "Signature of \"{}\" incompatible with supertype \"{parent}\"",
                function.name
            ),
            &range,
            Some(format!(
                "     Superclass:\n{}\n     Subclass:\n{}",
                format::method_with_receiver(&self.context, original, parent == "dataclass"),
                format::method_with_receiver(&self.context, method, parent == "dataclass")
            )),
        );
    }

    pub(super) fn function_range(&self, function: &StmtFunctionDef) -> ruff_text_size::TextRange {
        let prefix = &self.source.text[..function.name.start().to_usize()];
        let line_start = prefix.rfind('\n').map_or(0, |offset| offset + 1);
        let line = &prefix[line_start..];
        let offset = line_start + line.len() - line.trim_start().len();
        ruff_text_size::TextRange::new(ruff_text_size::TextSize::new(offset as u32), function.end())
    }

    pub(super) fn current_method_kind(&self, function: &StmtFunctionDef) -> MethodKind {
        if super::super::classes::dataclass::order_symbol(function.name.as_str()).is_some() {
            return super::super::classes::method_kind(function);
        }
        self.class_types
            .last()
            .copied()
            .flatten()
            .and_then(|ty| self.context.class_id(ty))
            .and_then(|id| {
                self.context
                    .class(id)
                    .methods
                    .get(function.name.as_str())
                    .map(|method| method.kind)
            })
            .unwrap_or(MethodKind::Instance)
    }
}
