//! Unpacking inference retains source values, uncertainty, and synthesized collections.
use super::{callables::Target, classes::MethodKind, resolution::Context, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, UnaryOp};
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::collections::BTreeSet;

pub(super) type Active = BTreeSet<(Context, u32, u32)>;
#[derive(Clone)]
pub(super) struct Unpacked {
    pub origin: Context,
    pub definition: Option<TextRange>,
    pub display: String,
    pub elements: Option<Vec<Expr>>,
    pub entries: Option<Vec<(Expr, Expr)>>,
    pub dictionary: bool,
    pub iterable: bool,
    pub vararg: Option<String>,
    pub view: Option<&'static str>,
    pub kind: &'static str,
}
impl Checker<'_> {
    pub(super) fn unpacked_value(&self, expression: &Expr, origin: Context) -> Option<Unpacked> {
        self.unpacked_inner(expression, origin, &mut Active::new())
    }
    pub(super) fn unpacked_inner(
        &self,
        expression: &Expr,
        origin: Context,
        active: &mut Active,
    ) -> Option<Unpacked> {
        let key = (
            origin.clone(),
            expression.start().to_u32(),
            expression.end().to_u32(),
        );
        if !active.insert(key.clone()) {
            return None;
        }
        let result = self.unpacked_step(expression, origin, active);
        active.remove(&key);
        result
    }
    fn unpacking_binding(&self, name: &str, origin: &Context) -> Option<(Expr, Context)> {
        if let Context::Scope(index) = origin {
            if let Some((owner, value)) = self.scopes.unpacking_expression(name, *index) {
                return value.map(|value| (value, Context::Scope(owner)));
            }
        }
        let module = match origin {
            Context::Module(module) => module.as_str(),
            Context::Scope(_) => self.module_name,
        };
        if self
            .modules
            .source_index(self.source.project, module)
            .is_some()
        {
            return self
                .modules
                .unpacking_expression(self.source.project, module, name)
                .map(|value| (value, origin.clone()));
        }
        self.source_binding(name, origin)
    }
    fn unpacking_attribute(
        &self,
        expression: &mamba_language::application::quality_syntax::python_ast::ExprAttribute,
        origin: Context,
    ) -> Option<(Expr, Context)> {
        let resolved = self.expression_target_in(&expression.value, origin)?;
        let (class, context, instance) = match resolved.target {
            Target::Module(module) => {
                return self.unpacking_binding(expression.attr.as_str(), &Context::Module(module))
            }
            Target::Instance(class, origin) => (class, origin, true),
            Target::Class(class) => (class, resolved.context, false),
            _ => return None,
        };
        for (class, context) in self.lineage(class, context) {
            if instance {
                if let Some(value) = class.attribute_expressions.get(expression.attr.as_str()) {
                    return Some((value.clone(), context));
                }
                if let Some(method) = class.methods.get(expression.attr.as_str()) {
                    if self.inferred_method_kind(
                        &method.signature.decorators,
                        context.clone(),
                        method.kind,
                    ) == MethodKind::Property
                    {
                        return method
                            .signature
                            .result_expression
                            .clone()
                            .map(|value| (value, context));
                    }
                }
            }
            if let Some(value) = class.expressions.get(expression.attr.as_str()) {
                return Some((value.clone(), context));
            }
        }
        None
    }
    fn unpacked_step(
        &self,
        expression: &Expr,
        origin: Context,
        active: &mut Active,
    ) -> Option<Unpacked> {
        if let Expr::Name(name) = expression {
            if let Some((value, context)) = self.unpacking_binding(name.id.as_str(), &origin) {
                if context == origin && value.start() > expression.start() {
                    return None;
                }
                if value.range() != expression.range() || context != origin {
                    return self.unpacked_inner(&value, context, active);
                }
            }
            let raw = match &origin {
                Context::Scope(index) => self
                    .scopes
                    .call_target_in(name.id.as_str(), *index)
                    .map(|(_, value)| value),
                Context::Module(module) => {
                    self.modules
                        .call_target(self.source.project, module, name.id.as_str())
                }
            };
            if let Some(Target::Imported(import)) = raw {
                if let Some(member) = import.member {
                    if let Some((value, context)) =
                        self.unpacking_binding(&member, &Context::Module(import.module))
                    {
                        return self.unpacked_inner(&value, context, active);
                    }
                }
            }
            if let Context::Scope(index) = &origin {
                if let Some(positional) = self.scopes.variadic_parameter(name.id.as_str(), *index) {
                    return Some(Unpacked {
                        origin,
                        definition: None,
                        display: if positional { "()" } else { "{}" }.into(),
                        elements: Some(Vec::new()),
                        entries: (!positional).then(Vec::new),
                        dictionary: !positional,
                        iterable: true,
                        vararg: positional.then(|| name.id.to_string()),
                        view: None,
                        kind: if positional { "tuple" } else { "dict" },
                    });
                }
            }
        }
        if let Expr::Attribute(value) = expression {
            let (value, context) = self.unpacking_attribute(value, origin)?;
            return self.unpacked_inner(&value, context, active);
        }
        let source_module = match &origin {
            Context::Module(module) => module.as_str(),
            Context::Scope(_) => self.module_name,
        };
        let source = self
            .modules
            .source(self.source.project, source_module)
            .unwrap_or(&self.source.text);
        let mut result = Unpacked {
            origin: origin.clone(),
            definition: Some(expression.range()),
            display: super::display::expression(expression, source),
            elements: None,
            entries: None,
            dictionary: false,
            iterable: false,
            vararg: None,
            view: None,
            kind: "constant",
        };
        match expression {
            Expr::Tuple(value) => {
                result.elements = Some(self.unpacking_elements(&value.elts, origin, active)?);
                result.kind = "tuple";
                result.iterable = true;
                result.display = super::unpacking_collections::display(
                    result.kind,
                    result.elements.as_ref()?,
                    source,
                );
            }
            Expr::List(value) => {
                result.elements = Some(self.unpacking_elements(&value.elts, origin, active)?);
                result.kind = "list";
                result.iterable = true;
                result.display = super::unpacking_collections::display(
                    result.kind,
                    result.elements.as_ref()?,
                    source,
                );
            }
            Expr::Set(value) => {
                result.elements = Some(self.unpacking_elements(&value.elts, origin, active)?);
                result.kind = "set";
                result.iterable = true;
                result.display = super::unpacking_collections::display(
                    result.kind,
                    result.elements.as_ref()?,
                    source,
                );
            }
            Expr::Dict(value) => {
                let entries = self.unpacking_dictionary(&value.items, origin, active)?;
                result.elements = Some(entries.iter().map(|(key, _)| key.clone()).collect());
                result.entries = Some(entries);
                result.dictionary = true;
                result.iterable = true;
                result.kind = "dict";
            }
            Expr::StringLiteral(_) | Expr::BytesLiteral(_) => {
                result.iterable = true;
                result.kind = "string";
            }
            Expr::NumberLiteral(_) | Expr::BooleanLiteral(_) | Expr::NoneLiteral(_) => {}
            Expr::UnaryOp(value)
                if matches!(value.op, UnaryOp::USub | UnaryOp::UAdd | UnaryOp::Invert)
                    && matches!(
                        self.slot_scalar(&value.operand, origin.clone()),
                        Value::Number(_) | Value::Boolean(_)
                    ) =>
            {
                result.definition = None;
            }
            Expr::BinOp(value) => {
                self.unpacking_binary(value, &mut result, origin, active)?;
            }
            Expr::If(value) => {
                let Expr::BooleanLiteral(test) = value.test.as_ref() else {
                    return None;
                };
                return self.unpacked_inner(
                    if test.value {
                        &value.body
                    } else {
                        &value.orelse
                    },
                    origin,
                    active,
                );
            }
            Expr::Subscript(value) => {
                return self.unpacking_subscript(value, result, origin, active)
            }
            Expr::Call(call) => {
                if let Expr::Attribute(attribute) = call.func.as_ref() {
                    if let Some(dictionary) = self
                        .unpacked_inner(&attribute.value, origin.clone(), active)
                        .filter(|value| value.dictionary && value.view.is_none())
                    {
                        if matches!(attribute.attr.as_str(), "keys" | "values" | "items")
                            && call.arguments.args.is_empty()
                            && call.arguments.keywords.is_empty()
                        {
                            let view = match attribute.attr.as_str() {
                                "keys" => "keys",
                                "values" => "values",
                                _ => "items",
                            };
                            result.elements = Some(if view == "values" {
                                dictionary
                                    .entries?
                                    .into_iter()
                                    .map(|(_, value)| value)
                                    .collect()
                            } else {
                                dictionary.elements?
                            });
                            result.dictionary = true;
                            result.iterable = true;
                            result.view = Some(view);
                            result.kind = "view";
                            return Some(result);
                        }
                    }
                }
                let resolved = self.expression_target_in(&call.func, origin.clone())?;
                match resolved.target {
                    Target::Class(class)
                        if class.identity.0 == "builtins"
                            && matches!(class.name.as_str(), "list" | "tuple" | "dict" | "set") =>
                    {
                        let kind = match class.name.as_str() {
                            "list" => "list",
                            "dict" => "dict",
                            "set" => "set",
                            _ => "tuple",
                        };
                        self.unpacking_constructor(
                            call,
                            kind,
                            &mut result,
                            origin,
                            active,
                            source,
                        )?;
                    }
                    Target::Function(function) => {
                        let module = match &resolved.context {
                            Context::Module(module) => module.as_str(),
                            Context::Scope(_) => self.module_name,
                        };
                        if module == source_module
                            && call.start() < function.definition_range.start()
                        {
                            return None;
                        }
                        return self.unpacked_inner(
                            function.result_expression.as_ref()?,
                            resolved.context,
                            active,
                        );
                    }
                    Target::Class(class) => {
                        result.iterable = self
                            .dynamic_member(class.clone(), resolved.context.clone())
                            || class.bases.iter().any(|base| {
                                self.declared_base(base, resolved.context.clone()).is_none()
                            });
                        for name in ["__iter__", "__getitem__"] {
                            if let Some(member) = self.class_member(
                                class.clone(),
                                resolved.context.clone(),
                                name,
                                true,
                            ) {
                                result.iterable |= !matches!(member.target, Target::Value(value) if !matches!(value.as_ref(), Value::Unknown));
                            }
                        }
                        result.kind = "instance";
                        result.origin = Context::Module(class.identity.0.clone());
                        result.definition = (class.identity.0 != "builtins")
                            .then(|| TextRange::empty(TextSize::new(class.identity.1)));
                    }
                    _ => return None,
                }
            }
            _ => return None,
        }
        Some(result)
    }
}
