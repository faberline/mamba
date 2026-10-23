//! Builtin isinstance checks use raw argument counts and conservative type inference.
use super::{callables::Target, operand_inference::Operand, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    BoolOp, Expr, ExprBoolOp, ExprCall, Operator,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

impl Checker<'_> {
    pub(super) fn check_merging_isinstance(&mut self, boolean: &ExprBoolOp) {
        if boolean.op != BoolOp::Or {
            return;
        }
        let mut groups: Vec<(String, BTreeSet<String>, usize)> = Vec::new();
        for expression in &boolean.values {
            let Expr::Call(call) = expression else {
                continue;
            };
            if call.arguments.args.len() != 2
                || !self.builtin_isinstance(&call.func, Context::Scope(self.scopes.current_index()))
            {
                continue;
            }
            let object = super::display::expression(&call.arguments.args[0], &self.source.text);
            let index = if let Some(index) = groups.iter().position(|(name, _, _)| name == &object)
            {
                index
            } else {
                groups.push((object, BTreeSet::new(), 0));
                groups.len() - 1
            };
            let (_, types, count) = &mut groups[index];
            *count += 1;
            let argument = &call.arguments.args[1];
            if let Expr::Tuple(tuple) = argument {
                types.extend(
                    tuple
                        .elts
                        .iter()
                        .map(|value| super::display::expression(value, &self.source.text)),
                );
            } else {
                types.insert(super::display::expression(argument, &self.source.text));
            }
        }
        for (object, types, count) in groups {
            if count < 2 {
                continue;
            }
            self.emit(
                "R1701",
                "consider-merging-isinstance",
                &format!(
                    "Consider merging these isinstance calls to isinstance({object}, ({}))",
                    types.into_iter().collect::<Vec<_>>().join(", ")
                ),
                boolean.range(),
            );
        }
    }
    fn builtin_isinstance(&self, expression: &Expr, origin: Context) -> bool {
        self.isinstance_reference(expression, origin, &mut BTreeSet::new())
    }
    fn isinstance_reference(
        &self,
        expression: &Expr,
        origin: Context,
        active: &mut BTreeSet<(Context, u32, u32)>,
    ) -> bool {
        let key = (
            origin.clone(),
            expression.start().to_u32(),
            expression.end().to_u32(),
        );
        if !active.insert(key.clone()) {
            return false;
        }
        let result = match expression {
            Expr::Name(name) => {
                let target = match &origin {
                    Context::Scope(index) => self.scopes.call_target_in(name.id.as_str(), *index),
                    Context::Module(module) => self.modules.call_target(self.source.project, module, name.id.as_str()).map(|target| (0, target)),
                };
                match target {
                    Some((_, Target::Imported(import))) => import.module == "builtins" && import.member.as_deref() == Some("isinstance"),
                    Some((_, Target::Reference(_))) => self.source_binding(name.id.as_str(), &origin).is_some_and(|(value, context)| self.isinstance_reference(&value, context, active)),
                    None => name.id.as_str() == "isinstance",
                    _ => false,
                }
            }
            Expr::Attribute(value) => value.attr.as_str() == "isinstance"
                && self.expression_target_in(&value.value, origin.clone()).is_some_and(|resolved| matches!(resolved.target, Target::Module(module) if module == "builtins")),
            _ => false,
        };
        active.remove(&key);
        result
    }
    pub(super) fn check_isinstance(&mut self, call: &ExprCall) {
        if !self.builtin_isinstance(&call.func, Context::Scope(self.scopes.current_index())) {
            return;
        }
        let count = call.arguments.args.len();
        if count > 2 {
            self.emit(
                "E1121",
                "too-many-function-args",
                "Too many positional arguments for function call",
                call.range(),
            );
        }
        for name in ["_obj", "__class_or_tuple"].into_iter().skip(count) {
            self.emit(
                "E1120",
                "no-value-for-parameter",
                &format!("No value for argument '{name}' in function call"),
                call.range(),
            );
        }
        if let Some(argument) = call.arguments.args.get(1) {
            if self.invalid_isinstance_type(
                argument,
                Context::Scope(self.scopes.current_index()),
                &mut BTreeSet::new(),
            ) {
                self.emit(
                    "W1116",
                    "isinstance-second-argument-not-valid-type",
                    "Second argument of isinstance is not a type",
                    call.range(),
                );
            }
        }
    }
    fn invalid_isinstance_type(
        &self,
        expression: &Expr,
        origin: Context,
        active: &mut BTreeSet<(Context, u32, u32)>,
    ) -> bool {
        let key = (
            origin.clone(),
            expression.start().to_u32(),
            expression.end().to_u32(),
        );
        if !active.insert(key.clone()) {
            return false;
        }
        let values = self.operand_nodes(expression, origin.clone());
        let operand = self.operand_in(expression, origin, &mut Default::default());
        let type_call = values.first().is_some_and(|(value, context)| {
            matches!(value, Expr::Call(call) if call.arguments.args.len() == 1 && self.expression_target_in(&call.func, context.clone()).is_some_and(|resolved| matches!(resolved.target, Target::Class(class) if class.identity.0 == "builtins" && class.name == "type")))
        });
        let result = if type_call
            || (matches!(operand, Operand::Unknown)
                && !matches!(expression, Expr::BinOp(value) if value.op == Operator::BitOr))
        {
            false
        } else if let Some((value, context)) = values.first() {
            match value {
                Expr::Tuple(tuple) => tuple
                    .elts
                    .iter()
                    .any(|value| self.invalid_isinstance_type(value, context.clone(), active)),
                Expr::BinOp(binary) if binary.op == Operator::BitOr => {
                    [&binary.left, &binary.right].iter().any(|value| {
                        !matches!(value.as_ref(), Expr::NoneLiteral(_))
                            && self.invalid_isinstance_type(value, context.clone(), active)
                    })
                }
                _ => {
                    !matches!(
                        operand,
                        Operand::Unknown | Operand::Class(_, _) | Operand::Literal("tuple")
                    ) && !matches!(operand, Operand::Instance(ref class, _) if class.identity.0 == "builtins" && class.name == "tuple")
                }
            }
        } else {
            false
        };
        active.remove(&key);
        result
    }

    pub(super) fn union_type_alias(&self, expression: &Expr) -> bool {
        let origin = Context::Scope(self.scopes.current_index());
        let nodes = self.operand_nodes(expression, origin);
        nodes.len() == 1 && nodes.first().is_some_and(|(value, context)| {
            matches!(value, Expr::BinOp(value) if value.op == Operator::BitOr && self.union_type_member(&value.left, context.clone()) && self.union_type_member(&value.right, context.clone()))
        })
    }
    fn union_type_member(&self, expression: &Expr, origin: Context) -> bool {
        if matches!(expression, Expr::NoneLiteral(_)) {
            return true;
        }
        if let Expr::BinOp(value) = expression {
            if value.op == Operator::BitOr {
                return self.union_type_member(&value.left, origin.clone())
                    && self.union_type_member(&value.right, origin);
            }
        }
        matches!(
            self.operand_in(expression, origin, &mut Default::default()),
            Operand::Class(_, _)
        )
    }
}
