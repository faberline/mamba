//! Operand alternatives retain AST ownership and original inference multiplicity.
use super::{
    callables::Target, classes::MethodKind, operand_inference::Active, resolution::Context, Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprAttribute};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn emit_operand_origin(
        &mut self,
        origin: &Context,
        rule: &str,
        name: &str,
        message: &str,
        range: ruff_text_size::TextRange,
    ) {
        self.emit_slot_origin(origin, rule, name, message, range);
    }
    pub(super) fn operand_nodes(&self, expression: &Expr, origin: Context) -> Vec<(Expr, Context)> {
        let mut seen = Active::new();
        self.operand_nodes_inner(expression, origin, &mut Active::new())
            .into_iter()
            .filter(|(value, origin)| {
                seen.insert((origin.clone(), value.start().to_u32(), value.end().to_u32()))
            })
            .collect()
    }
    fn operand_nodes_inner(
        &self,
        expression: &Expr,
        origin: Context,
        active: &mut Active,
    ) -> Vec<(Expr, Context)> {
        let key = (
            origin.clone(),
            expression.start().to_u32(),
            expression.end().to_u32(),
        );
        if !active.insert(key.clone()) {
            return Vec::new();
        }
        let result = self.operand_nodes_step(expression, origin, active);
        active.remove(&key);
        result
    }
    fn operand_nodes_step(
        &self,
        expression: &Expr,
        origin: Context,
        active: &mut Active,
    ) -> Vec<(Expr, Context)> {
        match expression {
            Expr::Name(name) => {
                if let Context::Scope(index) = &origin {
                    if let Some((owner, values)) =
                        self.scopes
                            .values_before(name.id.as_str(), *index, name.start())
                    {
                        if values.is_empty() {
                            return vec![(expression.clone(), origin)];
                        }
                        return values
                            .iter()
                            .flat_map(|value| {
                                self.operand_nodes_inner(value, Context::Scope(owner), active)
                            })
                            .collect();
                    }
                    if self.scopes.parameter_before_assignment(
                        name.id.as_str(),
                        *index,
                        name.start(),
                    ) {
                        return vec![(expression.clone(), origin)];
                    }
                    if let Some((owner, values)) =
                        self.scopes.operand_expressions(name.id.as_str(), *index)
                    {
                        if !values.is_empty() {
                            return values
                                .iter()
                                .flat_map(|value| {
                                    self.operand_nodes_inner(value, Context::Scope(owner), active)
                                })
                                .collect();
                        }
                    }
                }
                if let Some((value, context)) = self.source_binding(name.id.as_str(), &origin) {
                    if value.range() != expression.range() || context != origin {
                        return self.operand_nodes_inner(&value, context, active);
                    }
                }
                let raw = self.expression_target_in(expression, origin.clone());
                if let Some(resolved) = raw {
                    if let Target::Value(value) = resolved.target {
                        if let super::values::Value::Reference(names) = value.as_ref() {
                            if names.len() == 1 {
                                if let Some((value, context)) =
                                    self.source_binding(&names[0], &resolved.context)
                                {
                                    return self.operand_nodes_inner(&value, context, active);
                                }
                            }
                        }
                    }
                }
                let imported = match &origin {
                    Context::Scope(index) => self
                        .scopes
                        .call_target_in(name.id.as_str(), *index)
                        .map(|(_, target)| target),
                    Context::Module(module) => {
                        self.modules
                            .call_target(self.source.project, module, name.id.as_str())
                    }
                };
                if let Some(Target::Imported(import)) = imported {
                    if let Some(member) = import.member {
                        if let Some((value, context)) =
                            self.source_binding(&member, &Context::Module(import.module))
                        {
                            return self.operand_nodes_inner(&value, context, active);
                        }
                    }
                }
            }
            Expr::If(value) => {
                if matches!(value.body.as_ref(),Expr::Name(name) if !self.scopes.declared(name.id.as_str()) && !self.modules.builtin(name.id.as_str()))
                {
                    return vec![(expression.clone(), origin)];
                }
                let values = match value.test.as_ref() {
                    Expr::BooleanLiteral(test) => {
                        if test.value {
                            vec![value.body.as_ref()]
                        } else {
                            vec![value.orelse.as_ref()]
                        }
                    }
                    _ => vec![value.body.as_ref(), value.orelse.as_ref()],
                };
                return values.into_iter().filter(|value| !matches!(value, Expr::Name(name) if !self.scopes.declared(name.id.as_str()) && !self.modules.builtin(name.id.as_str()))).flat_map(|value| self.operand_nodes_inner(value,origin.clone(),active)).collect();
            }
            Expr::Call(call) => {
                if let Expr::Attribute(attribute) = call.func.as_ref() {
                    if attribute.attr.as_str() == "copy" {
                        let values =
                            self.operand_nodes_inner(&attribute.value, origin.clone(), active);
                        if !values.is_empty()
                            && values.iter().all(|(value, context)| {
                                self.unpacked_value(value, context.clone())
                                    .is_some_and(|value| {
                                        matches!(value.kind, "dict" | "list" | "set" | "frozenset")
                                            && value.elements.is_some()
                                    })
                            })
                        {
                            return values;
                        }
                    }
                }
                if let Some(resolved) = self.expression_target_in(&call.func, origin.clone()) {
                    if let Target::Function(function) = resolved.target {
                        if !function.is_async
                            && !function.generates
                            && !function.result_expressions.is_empty()
                        {
                            return function
                                .result_expressions
                                .iter()
                                .flat_map(|value| {
                                    self.operand_nodes_inner(
                                        value,
                                        resolved.context.clone(),
                                        active,
                                    )
                                })
                                .collect();
                        }
                    }
                }
            }
            Expr::Attribute(value) => {
                if let Some(nodes) = self.operand_attribute_nodes(value, origin.clone(), active) {
                    return nodes;
                }
            }
            Expr::Subscript(value) => {
                for (collection, context) in
                    self.operand_nodes_inner(&value.value, origin.clone(), active)
                {
                    let index = self.unpacking_integer(&value.slice, origin.clone());
                    let elements = match &collection {
                        Expr::Tuple(value) => Some(&value.elts),
                        Expr::List(value) => Some(&value.elts),
                        _ => None,
                    };
                    if let (Some(index), Some(elements)) = (index, elements) {
                        let index = if index < 0 {
                            i64::try_from(elements.len())
                                .ok()
                                .and_then(|length| length.checked_add(index))
                        } else {
                            Some(index)
                        };
                        if let Some(element) = index
                            .and_then(|index| usize::try_from(index).ok())
                            .and_then(|index| elements.get(index))
                        {
                            return self.operand_nodes_inner(element, context, active);
                        }
                    }
                    if let Expr::Dict(dictionary) = &collection {
                        if let Some(item) = dictionary.items.iter().find(|item| {
                            item.key.as_ref().is_some_and(|key| {
                                super::comparison_constants::same(key, &value.slice)
                            })
                        }) {
                            return self.operand_nodes_inner(&item.value, context, active);
                        }
                    }
                    if matches!(collection, Expr::StringLiteral(_)) && index.is_some() {
                        return vec![(collection, context)];
                    }
                }
            }
            _ => {}
        }
        vec![(expression.clone(), origin)]
    }
    fn operand_attribute_nodes(
        &self,
        value: &ExprAttribute,
        origin: Context,
        active: &mut Active,
    ) -> Option<Vec<(Expr, Context)>> {
        let resolved = self.expression_target_in(&value.value, origin)?;
        let (class, origin, instance) = match resolved.target {
            Target::Module(module) => {
                let (expression, context) =
                    self.source_binding(value.attr.as_str(), &Context::Module(module))?;
                return Some(self.operand_nodes_inner(&expression, context, active));
            }
            Target::Class(class) => (class, resolved.context, false),
            Target::Instance(class, context) => (class, context, true),
            _ => return None,
        };
        for (class, context) in self.lineage(class, origin) {
            if instance {
                if let Some(expression) = class.attribute_expressions.get(value.attr.as_str()) {
                    return Some(self.operand_nodes_inner(expression, context, active));
                }
                if let Some(method) = class.methods.get(value.attr.as_str()) {
                    if self.inferred_method_kind(
                        &method.signature.decorators,
                        context.clone(),
                        method.kind,
                    ) == MethodKind::Property
                    {
                        return Some(
                            method
                                .signature
                                .result_expressions
                                .iter()
                                .flat_map(|value| {
                                    self.operand_nodes_inner(value, context.clone(), active)
                                })
                                .collect(),
                        );
                    }
                }
            }
            if let Some(expression) = class.expressions.get(value.attr.as_str()) {
                return Some(self.operand_nodes_inner(expression, context, active));
            }
        }
        None
    }
}
