//! Each restricted type-variable combination checks a fresh function flow.
use super::*;
use signatures::Signature;
use types::{Node, Type};

struct VariantFrame {
    environment: BTreeMap<String, Option<Type>>,
    variables: types::Variables,
    returns: Option<Type>,
    suppress_comparison_overlap: bool,
}

impl Checker<'_> {
    pub(super) fn check_function_variants<'a>(
        &mut self,
        function: &'a StmtFunctionDef,
        signature: &Signature,
        environment: BTreeMap<String, Option<Type>>,
        returns: Option<Type>,
        implicit_receiver: bool,
        protocol_stub: bool,
    ) where
        Self: Visitor<'a>,
    {
        let previous_annotations = self.scopes.annotation_scope(
            signature.variables.clone(),
            signature.generic.iter().copied().collect(),
        );
        self.check_defaults(function);
        self.scopes
            .annotation_scope(previous_annotations.0, previous_annotations.1);
        let mut variables = signature.generic.to_vec();
        if self.in_class {
            if let Some(id) = self
                .class_types
                .last()
                .copied()
                .flatten()
                .and_then(|ty| self.context.class_id(ty))
            {
                variables.extend(self.context.class(id).generic.iter().copied());
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut variants = vec![BTreeMap::new()];
        for variable in variables {
            if !seen.insert(variable) {
                continue;
            }
            let Some(Node::Variable(definition)) = self.context.node(variable).as_deref().cloned()
            else {
                continue;
            };
            if definition.values.is_empty() {
                continue;
            }
            variants = variants
                .into_iter()
                .flat_map(|mapping| {
                    definition.values.iter().map(move |&value| {
                        let mut mapping = mapping.clone();
                        mapping.insert(variable, value);
                        mapping
                    })
                })
                .collect();
        }
        let expanded = variants.len() > 1;
        let first = self.diagnostics.len();
        let mut diagnostics = Vec::new();
        let previous_calls = self.expected_calls.clone();
        for mapping in variants {
            let variables: types::Variables = signature
                .variables
                .iter()
                .map(|(name, &ty)| (name.clone(), self.context.substitute(ty, &mapping)))
                .collect();
            let environment = environment
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        ty.map(|ty| self.context.substitute(ty, &mapping)),
                    )
                })
                .collect();
            let returns = returns.map(|ty| self.context.substitute(ty, &mapping));
            self.expected_calls = previous_calls.clone();
            self.check_function_variant(
                function,
                signature,
                VariantFrame {
                    environment,
                    variables,
                    returns,
                    suppress_comparison_overlap: expanded,
                },
                implicit_receiver,
                protocol_stub,
            );
            if expanded {
                let variant = self.take_variant_diagnostics(first);
                super::super::report::append_variant(&mut diagnostics, variant);
            }
        }
        self.expected_calls = previous_calls;
        if expanded {
            self.diagnostics.extend(diagnostics);
            self.settled_diagnostics.push(first..self.diagnostics.len());
        }
    }

    fn check_function_variant<'a>(
        &mut self,
        function: &'a StmtFunctionDef,
        signature: &Signature,
        frame: VariantFrame,
        implicit_receiver: bool,
        protocol_stub: bool,
    ) where
        Self: Visitor<'a>,
    {
        let VariantFrame {
            environment,
            variables,
            returns,
            suppress_comparison_overlap,
        } = frame;
        let generator = Signature::is_generator(function)
            .then_some((returns.unwrap_or(Type::Any), function.is_async));
        let returns = generator.map_or(returns, |(declared, asynchronous)| {
            Some(if asynchronous {
                Type::Any
            } else {
                self.context.generator_parts(declared).returning
            })
        });
        self.functions.push(returns);
        let previous_partial_set_scope = self.enter_partial_set_scope();
        let previous_partial_dict_dynamic =
            std::mem::replace(&mut self.partial_dict_dynamic, signature.implicit);
        let super_receiver = self
            .class_types
            .last()
            .copied()
            .flatten()
            .and_then(|ty| self.context.class_id(ty))
            .zip(function.parameters.iter().next().and_then(|parameter| {
                if parameter.is_variadic() {
                    None
                } else {
                    environment
                        .get(parameter.name().as_str())
                        .copied()
                        .flatten()
                }
            }));
        self.scopes.enter_with_bound(
            scopes::Kind::Function,
            &function.body,
            environment,
            variables,
            &signature.generic,
        );
        self.scopes.set_super_receiver(super_receiver);
        self.enter_constructor_none(function, signature);
        self.scopes.set_generator(generator, &function.body);
        self.scopes.bind_type_variables(&signature.generic);
        self.instances.push(BTreeMap::new());
        self.annotations.push(
            function
                .parameters
                .iter()
                .map(|parameter| (parameter.name().to_string(), parameter.name().start()))
                .collect(),
        );
        for parameter in function
            .parameters
            .iter()
            .enumerate()
            .filter(|(index, _)| !implicit_receiver || *index != 0)
            .map(|(_, parameter)| parameter)
        {
            if let Some(annotation) = parameter.annotation() {
                self.remember_name(parameter.name().as_str(), annotation);
            }
        }
        let previous = self.in_class;
        self.in_class = false;
        let previous_inputs = std::mem::take(&mut self.try_inputs);
        let previous_flow = std::mem::take(&mut self.flow);
        let previous_overlap = std::mem::replace(
            &mut self.suppress_comparison_overlap,
            suppress_comparison_overlap,
        );
        let previous_loops = std::mem::take(&mut self.loops);
        self.visit_block(&function.body);
        if !protocol_stub {
            self.check_return_paths(function, returns);
        }
        self.finish_partial_sets();
        self.finish_constructor_none();
        self.flow = previous_flow;
        self.try_inputs = previous_inputs;
        self.suppress_comparison_overlap = previous_overlap;
        self.loops = previous_loops;
        self.in_class = previous;
        self.functions.pop();
        self.scopes.leave();
        self.partial_set_scope = previous_partial_set_scope;
        self.partial_dict_dynamic = previous_partial_dict_dynamic;
        self.instances.pop();
        self.annotations.pop();
    }
    fn enter_constructor_none(&mut self, function: &StmtFunctionDef, signature: &Signature) {
        let plan = self
            .in_class
            .then(|| self.constructor_none_plans.last().cloned().flatten())
            .flatten();
        let frame = plan.and_then(|plan| {
            if plan.function != function as *const StmtFunctionDef as usize {
                self.context.constructor_none_order(&plan);
                return None;
            }
            if !signature.implicit
                || signature.ignored
                || !signature.generic.is_empty()
                || !signature.overloads.is_empty()
                || !self.scopes.constructor_none_scope(&plan)
            {
                self.context.constructor_none_gap(
                    plan.id,
                    "actual function scope is not the admitted constructor",
                );
                return None;
            }
            self.context.begin_constructor_none(plan)
        });
        // Every ordinary function visit shields any enclosing receipt.
        self.constructor_none_frames.push(frame);
    }

    pub(in crate::infrastructure::mypy::checker) fn record_constructor_none_assignment(
        &mut self,
        assignment: &quality_syntax::python_ast::StmtAssign,
        actual: Option<Type>,
    ) {
        if actual != Some(Type::None) || !matches!(assignment.value.as_ref(), Expr::NoneLiteral(_))
        {
            return;
        }
        let Some(plan) = self
            .constructor_none_frames
            .last()
            .and_then(Option::as_ref)
            .map(|frame| frame.plan.clone())
        else {
            return;
        };
        // Lambda/comprehension scopes and nested functions cannot borrow this
        // frame. The source target/value pointers are checked by the recorder.
        if !self.flow.reachable
            || !self.scopes.constructor_none_scope(&plan)
            || self.environment().get(&plan.receiver) != Some(&plan.instance)
        {
            return;
        }
        let frame = self
            .constructor_none_frames
            .last_mut()
            .unwrap()
            .as_mut()
            .unwrap();
        self.context
            .record_constructor_none(frame, assignment, actual);
    }

    fn finish_constructor_none(&mut self) {
        if let Some(mut frame) = self.constructor_none_frames.pop().flatten() {
            if !self.scopes.constructor_none_scope(&frame.plan) {
                frame.reached = false;
            }
            self.context.finish_constructor_none(frame);
        }
    }
}
