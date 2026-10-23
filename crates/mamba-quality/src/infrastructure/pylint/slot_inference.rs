//! Slot inference preserves source nodes through aliases and simple expressions.
use super::{callables::Target, resolution::Context, values::Value, Checker};
use mamba_language::application::quality_syntax::{
    self,
    python_ast::{Expr, Number, Operator, UnaryOp},
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

impl Checker<'_> {
    pub(super) fn source_binding(&self, name: &str, context: &Context) -> Option<(Expr, Context)> {
        if let Context::Scope(index) = context {
            if let Some((owner, expression)) = self.scopes.expression_in(name, *index) {
                return expression.map(|value| (value, Context::Scope(owner)));
            }
        }
        let module = match context {
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
                .expression(self.source.project, module, name)
                .map(|value| (value, context.clone()));
        }
        if let Some(binding) = self
            .modules
            .embedded_binding(self.source.project, module, name)
        {
            return binding.map(|value| (value, context.clone()));
        }
        let source = self.modules.source(self.source.project, module)?;
        let document = quality_syntax::parse(source, self.modules.target(), false).ok()?;
        super::source_bindings::collect(&document.parsed.syntax().body)
            .remove(name)
            .map(|value| (value, context.clone()))
    }
    pub(super) fn slot_nodes(&self, expression: &Expr, context: Context) -> Vec<(Expr, Context)> {
        self.slot_nodes_inner(expression, context, &mut BTreeSet::new())
    }
    fn slot_nodes_inner(
        &self,
        expression: &Expr,
        context: Context,
        active: &mut BTreeSet<(Context, u32, u32)>,
    ) -> Vec<(Expr, Context)> {
        let key = (
            context.clone(),
            expression.start().to_u32(),
            expression.end().to_u32(),
        );
        if !active.insert(key.clone()) {
            return Vec::new();
        }
        let result = self.slot_nodes_step(expression, context, active);
        active.remove(&key);
        result
    }
    fn slot_nodes_step(
        &self,
        expression: &Expr,
        context: Context,
        active: &mut BTreeSet<(Context, u32, u32)>,
    ) -> Vec<(Expr, Context)> {
        match expression {
            Expr::Name(name) => {
                let raw = match &context {
                    Context::Scope(index) => self
                        .scopes
                        .call_target_in(name.id.as_str(), *index)
                        .map(|(_, target)| target),
                    Context::Module(module) => {
                        self.modules
                            .call_target(self.source.project, module, name.id.as_str())
                    }
                };
                if let Some(Target::Imported(import)) = raw {
                    if let Some(member) = import.member {
                        if let Some((value, origin)) =
                            self.source_binding(&member, &Context::Module(import.module.clone()))
                        {
                            return self.slot_nodes_inner(&value, origin, active);
                        }
                    }
                }
                if let Some((value, origin)) = self.source_binding(name.id.as_str(), &context) {
                    if value.range() != expression.range() || origin != context {
                        return self.slot_nodes_inner(&value, origin, active);
                    }
                }
            }
            Expr::Call(call) => {
                if let Some(resolved) = self.expression_target_in(&call.func, context.clone()) {
                    if let Target::Function(function) = resolved.target {
                        let own_module = match &context {
                            Context::Module(module) => module.as_str(),
                            Context::Scope(_) => self.module_name,
                        };
                        let function_module = match &resolved.context {
                            Context::Module(module) => module.as_str(),
                            Context::Scope(_) => self.module_name,
                        };
                        if own_module == function_module
                            && call.start() < function.definition_range.start()
                        {
                            return vec![(expression.clone(), context)];
                        }
                        if let Some(value) = &function.result_expression {
                            return self.slot_nodes_inner(value, resolved.context, active);
                        }
                    }
                }
            }
            Expr::If(value) => {
                let branches = match value.test.as_ref() {
                    Expr::BooleanLiteral(test) => {
                        if test.value {
                            vec![value.body.as_ref()]
                        } else {
                            vec![value.orelse.as_ref()]
                        }
                    }
                    _ => vec![value.body.as_ref(), value.orelse.as_ref()],
                };
                return branches
                    .into_iter()
                    .flat_map(|value| self.slot_nodes_inner(value, context.clone(), active))
                    .collect();
            }
            Expr::Subscript(value) => {
                if let Expr::NumberLiteral(index) = value.slice.as_ref() {
                    if let Number::Int(index) = &index.value {
                        if let Ok(index) = index.to_string().parse::<usize>() {
                            let mut result = Vec::new();
                            for (sequence, origin) in
                                self.slot_nodes_inner(&value.value, context.clone(), active)
                            {
                                let values = match &sequence {
                                    Expr::Tuple(value) => &value.elts,
                                    Expr::List(value) => &value.elts,
                                    _ => continue,
                                };
                                if let Some(value) = values.get(index) {
                                    result.extend(self.slot_nodes_inner(value, origin, active));
                                }
                            }
                            if !result.is_empty() {
                                return result;
                            }
                        }
                    }
                }
            }
            Expr::BinOp(value) if matches!(value.op, Operator::Add | Operator::Mult) => {
                let left = self.slot_nodes_inner(&value.left, context.clone(), active);
                let right = self.slot_nodes_inner(&value.right, context.clone(), active);
                let mut result = Vec::new();
                for (left, origin) in left {
                    for (right, _) in &right {
                        let elements = match (&left, right, value.op) {
                            (Expr::Tuple(left), Expr::Tuple(right), Operator::Add) => {
                                Some([left.elts.clone(), right.elts.clone()].concat())
                            }
                            (Expr::List(left), Expr::List(right), Operator::Add) => {
                                Some([left.elts.clone(), right.elts.clone()].concat())
                            }
                            (Expr::Tuple(left), Expr::NumberLiteral(right), Operator::Mult) => {
                                match &right.value {
                                    Number::Int(number) => {
                                        number.to_string().parse::<usize>().ok().map(|number| {
                                            (0..number).flat_map(|_| left.elts.clone()).collect()
                                        })
                                    }
                                    _ => None,
                                }
                            }
                            _ => None,
                        };
                        if let Some(elements) = elements {
                            let mut sequence = left.clone();
                            match &mut sequence {
                                Expr::Tuple(value) => value.elts = elements,
                                Expr::List(value) => value.elts = elements,
                                _ => {}
                            }
                            result.push((sequence, origin.clone()));
                        }
                    }
                }
                if !result.is_empty() {
                    return result;
                }
            }
            _ => {}
        }
        vec![(expression.clone(), context)]
    }
    pub(super) fn slot_scalar(&self, expression: &Expr, context: Context) -> Value {
        match expression {
            Expr::UnaryOp(value) if matches!(value.op, UnaryOp::UAdd | UnaryOp::USub) => {
                self.slot_scalar(&value.operand, context)
            }
            Expr::BinOp(value) if value.op == Operator::Add => {
                match (
                    self.slot_scalar(&value.left, context.clone()),
                    self.slot_scalar(&value.right, context),
                ) {
                    (Value::String(left), Value::String(right)) => Value::String(left + &right),
                    (Value::Number(left), Value::Number(_)) => Value::Number(left),
                    _ => Value::Unknown,
                }
            }
            _ => match self
                .expression_target_in(expression, context.clone())
                .map(|value| value.target)
            {
                Some(Target::Value(value)) => {
                    self.expand_value((*value).clone(), &context, &mut BTreeSet::new())
                }
                _ => Value::Unknown,
            },
        }
    }
    pub(super) fn emit_slot_origin(
        &mut self,
        origin: &Context,
        rule: &str,
        name: &str,
        message: &str,
        range: ruff_text_size::TextRange,
    ) {
        let source = match origin {
            Context::Module(module) => self
                .modules
                .source_index(self.source.project, module)
                .and_then(|index| self.snapshots.get(index))
                .unwrap_or(self.source),
            Context::Scope(_) => self.source,
        };
        let diagnostic = super::super::diagnostics::at(
            source,
            crate::domain::model::QualityTool::Pylint,
            rule,
            message.into(),
            range,
        );
        if !super::directives::suppressed(&self.directives, rule, name, diagnostic.location.line) {
            self.diagnostics.push(diagnostic);
        }
    }
}
