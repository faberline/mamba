//! Expression checks use the shared visitor order and keep original spans.
use super::*;
use quality_syntax::python_ast::{CmpOp, Number};
mod version_info;

impl Checker<'_> {
    pub(super) fn candidate_partial_read_is_stable(
        &self,
        name: &quality_syntax::python_ast::ExprName,
    ) -> bool {
        let identity = self.scopes.inferred_binding_identity(name.id.as_str());
        let location = self.scopes.binding_location(name.id.as_str());
        for (key, partial) in &self.partial_sets {
            if !partial.active || partial.name != name.id.as_str() || location != Some(*key) {
                continue;
            }
            let matched = partial.kind == types::Container::Set
                || matches!(
                    partial.kind,
                    types::Container::List | types::Container::Dict
                ) && identity == partial.binding_owner.map(|owner| (owner, *key));
            if matched
                && (partial.inferred.is_none()
                    || partial.kind == types::Container::List
                        && self.partial_list_unknown.contains(key))
            {
                return false;
            }
        }
        !self.partial_defaultdicts.iter().any(|(key, partial)| {
            partial.active
                && partial.name == name.id.as_str()
                && identity == Some((partial.binding_owner, *key))
                && partial.inferred.is_none()
        })
    }

    pub(super) fn check_candidate_name_read(
        &mut self,
        name: &quality_syntax::python_ast::ExprName,
    ) -> Result<(), super::generator_inputs::CandidateFailure> {
        self.check_name(name);
        if self.scopes.unavailable_inferred_local(name) {
            self.emit(
                "has-type",
                format!("Cannot determine type of \"{}\"", name.id),
                name,
            );
        }
        if self
            .scopes
            .before_definition(name.id.as_str(), name.start())
        {
            self.emit(
                "used-before-def",
                format!("Name \"{}\" is used before definition", name.id),
                name,
            );
        }
        if self.typed_dict_trial_failed == Some(true) {
            Err(super::generator_inputs::CandidateFailure::Rejected)
        } else {
            Ok(())
        }
    }

    pub(super) fn check_candidate_expression(
        &mut self,
        expression: &Expr,
        expected: Option<types::Type>,
    ) -> super::generator_inputs::CheckedExpression {
        use super::generator_inputs::{unknown_expression, CandidateFailure, CheckedValue};
        if self.contextual_candidate_depth == 0 {
            return unknown_expression(expression, "missing candidate frame");
        }
        let actual = match expression {
            Expr::Name(name) if name.ctx.is_load() => {
                let recovery = if self.candidate_partial_read_is_stable(name) {
                    None
                } else {
                    let Some(receipt) = self.committed_partial_dict_read(name) else {
                        return unknown_expression(expression, "active partial Name");
                    };
                    Some(receipt)
                };
                if recovery.is_none()
                    && self
                        .environment()
                        .get(name.id.as_str())
                        .copied()
                        .is_none_or(|ty| !self.context.candidate_value_role(ty, 0))
                {
                    return unknown_expression(expression, "unproved Name value role");
                }
                self.check_candidate_name_read(name)?;
                match recovery {
                    Some(receipt) => Some(CheckedValue::Recovered {
                        actual: receipt.actual,
                        origins: vec![super::generator_inputs::RecoveryOrigin::PartialRead(
                            receipt,
                        )],
                    }),
                    None => self.infer(expression).map(CheckedValue::Clean),
                }
            }
            Expr::NumberLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_) => {
                if expected.is_some_and(|expected| self.context.contains_literal(expected)) {
                    self.context
                        .literal_expression(expression)
                        .or_else(|| self.infer(expression))
                        .map(CheckedValue::Clean)
                } else {
                    self.infer(expression).map(CheckedValue::Clean)
                }
            }
            Expr::Tuple(tuple) if tuple.ctx.is_load() => {
                let child_contexts: Vec<Option<types::Type>> = match expected {
                    None => vec![None; tuple.elts.len()],
                    Some(expected) => match self.context.node(expected).as_deref() {
                        Some(types::Node::Tuple(arguments))
                            if arguments.len() == tuple.elts.len() =>
                        {
                            arguments.iter().copied().map(Some).collect()
                        }
                        Some(types::Node::VariadicTuple(item)) => {
                            vec![Some(*item); tuple.elts.len()]
                        }
                        _ => {
                            return unknown_expression(
                                expression,
                                "unproved tuple argument context",
                            );
                        }
                    },
                };
                let mut items = Vec::new();
                for (value, expected) in tuple.elts.iter().zip(child_contexts) {
                    items.push(self.check_candidate_expression(value, expected)?);
                }
                let actual = self.context.intern(types::Node::Tuple(
                    items.iter().map(CheckedValue::actual).collect(),
                ));
                Some(CheckedValue::combine(actual, items))
            }
            Expr::Generator(generator) => {
                Some(self.check_candidate_generator(generator, expected)?)
            }
            Expr::DictComp(dictionary) => {
                Some(self.check_candidate_dictionary(dictionary, expected)?)
            }
            Expr::Call(call) => Some(self.check_candidate_call(call, expected)?),
            Expr::Subscript(subscript) if subscript.ctx.is_load() => {
                let receiver = self.check_candidate_expression(&subscript.value, None)?;
                Some(self.check_candidate_subscript(subscript, receiver)?)
            }
            Expr::BinOp(binary) => {
                let left = self.check_candidate_expression(&binary.left, None)?;
                let right = self.check_candidate_expression(&binary.right, None)?;
                match self.context.candidate_binary_result(
                    left.actual(),
                    right.actual(),
                    binary.op,
                    self.python_target,
                ) {
                    Some(actual) => Some(CheckedValue::combine(actual, [left, right])),
                    None => return unknown_expression(expression, "unproved binary operation"),
                }
            }
            _ => return unknown_expression(expression, "expression producer is not covered"),
        };
        let Some(actual) = actual else {
            return unknown_expression(expression, "checked expression has no type");
        };
        if self.typed_dict_trial_failed == Some(true) && !actual.is_recovered() {
            return Err(CandidateFailure::Rejected);
        }
        if !self.context.candidate_value_role(actual.actual(), 0) {
            return unknown_expression(expression, "unproved expression result role");
        }
        let Some(values) = &self.checked_expression_types else {
            return unknown_expression(expression, "missing local expression map");
        };
        values
            .lock()
            .expect("owned candidate expression types")
            .insert(expression as *const Expr as usize, Some(actual.actual()));
        Ok(actual)
    }

    pub(super) fn visit_expression<'a>(&mut self, expression: &'a Expr)
    where
        Self: Visitor<'a>,
    {
        if self.visit_partial_defaultdict_call(expression) {
            return;
        }
        if self.visit_conditional_value(expression) {
            return;
        }
        if self.visit_template_string(expression) {
            return;
        }
        if self.visit_generator_lambda(expression) {
            return;
        }
        if self.visit_name_scope(expression) {
            return;
        }
        if let Expr::Named(named) = expression {
            self.visit_expr(&named.value);
            let actual = self
                .observed_fstring_type(&named.value)
                .unwrap_or_else(|| self.infer(&named.value));
            // Mypy check_simple_assignment checks these context-independent
            // scalar literals against the declared type. Keep the first value
            // visit and actual; do not infer the RHS a second time.
            let scalar = match named.value.as_ref() {
                Expr::NumberLiteral(number) => match number.value {
                    Number::Int(_) => Some(types::Type::Int),
                    Number::Float(_) => Some(types::Type::Float),
                    _ => None,
                },
                Expr::StringLiteral(_) => Some(types::Type::Str),
                Expr::BytesLiteral(_) => Some(types::Type::Bytes),
                _ => None,
            };
            let accepted = match (
                self.scopes.named_expression_declared_type(&named.target),
                scalar,
                actual,
            ) {
                (Some(expected), Some(scalar), Some(actual)) if scalar == actual => {
                    self.check_assignment_type(expected, actual, named.value.as_ref())
                }
                _ => false,
            };
            self.scopes
                .bind_assignment_expression(&named.target, actual);
            self.flow_assignment(&named.target, actual);
            // Unsupported writes and failed checks refuse active try frames.
            // A successful bind or flow update is never the receipt.
            self.record_try_input_commit(&named.target, actual, accepted);
            return;
        }
        if let Expr::Compare(compare) = expression {
            self.check_version_info_equality(compare);
            if self.visit_dataclass_comparison(compare) {
                return;
            }
        }
        if let Expr::BoolOp(boolean) = expression {
            self.visit_boolean(boolean);
            return;
        }
        if let Expr::Name(name) = expression {
            self.read_partial_set(name);
            self.check_name(name);
            if self.scopes.unavailable_inferred_local(name) {
                self.emit(
                    "has-type",
                    format!("Cannot determine type of \"{}\"", name.id),
                    name,
                );
            }
            if name.ctx.is_load()
                && self
                    .scopes
                    .before_definition(name.id.as_str(), name.start())
            {
                self.emit(
                    "used-before-def",
                    format!("Name \"{}\" is used before definition", name.id),
                    name,
                );
            }
        }
        if self.visit_reveal_value(expression) {
            return;
        }
        let deferred = self
            .partial_defaultdict_deferred
            .contains(&(expression.start(), expression.end()));
        if !deferred {
            self.check_model_call(expression);
            self.check_function_call(expression);
        }
        if let Expr::BinOp(binary) = expression {
            self.check_template_operator(binary);
            if binary.op == quality_syntax::python_ast::Operator::Sub {
                let left = self.infer(&binary.left);
                if left == Some(types::Type::Str) {
                    if self.infer(&binary.right) == Some(types::Type::Str) {
                        self.emit(
                            "operator",
                            "Unsupported left operand type for - (\"str\")".into(),
                            binary,
                        );
                    }
                } else if self.template_depth == 0 {
                    if let Some(left) =
                        left.filter(|&ty| ty == types::Type::Int || self.is_optional_integer(ty))
                    {
                        if let Some(right) = self.infer(&binary.right) {
                            self.check_optional_integer_arithmetic(binary, left, right);
                        }
                    }
                }
            }
        }
        if let Expr::Subscript(subscript) = expression {
            if !deferred {
                self.check_nominal_subscript(subscript);
                self.check_class_index(subscript);
                self.check_typed_dict_index(subscript);
                self.check_dictionary_index(subscript);
            }
        }
        let mut attribute_receiver_visited = false;
        if let Expr::Attribute(attribute) = expression {
            if attribute.ctx.is_load() {
                self.visit_expr(&attribute.value);
                attribute_receiver_visited = true;
            }
            if !deferred {
                self.check_expression_attribute(expression, attribute);
            }
        }
        if self.visit_fstring_bytes_coercion(expression) {
            return;
        }
        if self.visit_literal_format(expression) {
            return;
        }
        if let Expr::Compare(compare) = expression {
            self.visit_expr(compare.first_operand());
            for operator in &compare.ops {
                self.visit_cmp_op(operator);
            }
            for (index, (operator, operand)) in
                compare.ops.iter().zip(compare.comparators()).enumerate()
            {
                // A middle operand is also read as the next comparison's left side.
                let membership_rhs =
                    index + 1 == compare.ops.len() && matches!(operator, CmpOp::In | CmpOp::NotIn);
                if membership_rhs {
                    self.visit_partial_set_membership_rhs(operand);
                } else {
                    self.visit_expr(operand);
                }
            }
        } else if !attribute_receiver_visited {
            walk_expr(self, expression);
        }
        if let Expr::BinOp(binary) = expression {
            self.check_addition_operator(binary);
        }
    }

    pub(super) fn check_expression_attribute(
        &mut self,
        expression: &Expr,
        attribute: &quality_syntax::python_ast::ExprAttribute,
    ) {
        self.check_object_attribute(attribute);
        if let Some(module) = self.scopes.module_expression(&attribute.value) {
            match self.scopes.module_member(expression) {
                Some(export) if export.is_hidden() => self.emit(
                    "attr-defined",
                    format!(
                        "Module \"{module}\" does not explicitly export attribute \"{}\"",
                        attribute.attr
                    ),
                    attribute,
                ),
                None => self.emit_missing_module_attribute(&module, attribute),
                _ => {}
            }
        }
    }

    fn is_optional_integer(&self, ty: types::Type) -> bool {
        matches!(
            self.context.node(ty).as_deref(),
            Some(types::Node::Union(items))
                if items.len() == 2
                    && items.contains(&types::Type::Int)
                    && items.contains(&types::Type::None)
        )
    }

    fn check_optional_integer_arithmetic(
        &mut self,
        binary: &quality_syntax::python_ast::ExprBinOp,
        left: types::Type,
        right: types::Type,
    ) -> bool {
        if self.template_depth > 0 {
            return false;
        }
        let symbol = match binary.op {
            quality_syntax::python_ast::Operator::Add => "+",
            quality_syntax::python_ast::Operator::Sub => "-",
            _ => return false,
        };
        let (left, right, side, original) =
            if left == types::Type::Int && self.is_optional_integer(right) {
                (left, types::Type::None, "Right", right)
            } else if self.is_optional_integer(left) && right == types::Type::Int {
                (types::Type::None, right, "Left", left)
            } else {
                return false;
            };
        self.emit_with_hint(
            "operator",
            format!(
                "Unsupported operand types for {symbol} (\"{}\" and \"{}\")",
                self.context.name(left),
                self.context.name(right),
            ),
            &ruff_text_size::TextRange::at(binary.start(), ruff_text_size::TextSize::new(1)),
            Some(format!(
                "{side} operand is of type \"{}\"",
                self.context.name(original),
            )),
        );
        true
    }

    fn check_addition_operator(&mut self, binary: &quality_syntax::python_ast::ExprBinOp) {
        if binary.op != quality_syntax::python_ast::Operator::Add || self.template_depth > 0 {
            return;
        }
        let (Some(left), Some(right)) = (self.infer(&binary.left), self.infer(&binary.right))
        else {
            return;
        };
        // Template checks keep their existing implementation and diagnostic.
        if self.context.template_addition(left, right).is_some() {
            return;
        }
        let invalid = self
            .context
            .addition_result(left, right)
            .is_some_and(|(_, invalid)| invalid);
        if self.check_optional_integer_arithmetic(binary, left, right) {
            return;
        }
        if invalid
            || (left == types::Type::Str
                && matches!(
                    self.context.node(right).as_deref(),
                    Some(types::Node::Container(types::Container::List, _))
                ))
        {
            self.emit(
                "operator",
                format!(
                    "Unsupported operand types for + (\"{}\" and \"{}\")",
                    self.context.name(left),
                    self.context.name(right),
                ),
                binary.right.as_ref(),
            );
        }
    }
}
