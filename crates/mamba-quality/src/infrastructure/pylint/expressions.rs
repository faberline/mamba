//! Expression checks retain separate lambda and comprehension namespaces.
use super::{scopes::Resolution, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, Visitor},
    Expr,
};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn visit_expression(&mut self, expression: &Expr) {
        self.check_semantic_expression(expression);
        if let Expr::Yield(value) = expression {
            self.check_yield_from(value);
        }
        if let Expr::YieldFrom(value) = expression {
            self.check_sync_yield_iteration(&value.value);
        }
        self.check_implicit_boolean(expression);
        self.check_refactoring_boolean(expression);
        self.check_comparison_graph(expression);
        self.check_simplifiable_condition(expression);
        self.check_string_constants(expression);
        if matches!(expression, Expr::Await(_))
            && self.invalid_awaits.contains(&expression.start().to_u32())
        {
            self.emit(
                "E1142",
                "await-outside-async",
                "'await' should be used within an async function",
                expression.range(),
            );
        }
        if let Expr::Compare(comparison) = expression {
            self.check_chained_comparison(comparison);
            self.check_comparison(comparison);
        }
        if let Expr::Set(value) = expression {
            self.check_duplicate_set(value);
            self.check_hashable_set(value);
        }
        if let Expr::If(expression) = expression {
            self.check_constant_condition(&expression.test, expression.range());
            self.check_refactoring_if_expression(expression);
        }
        if let Expr::Subscript(value) = expression {
            self.check_subscript(value);
        }
        if let Expr::UnaryOp(value) = expression {
            self.check_unary_operand(value);
        }
        if let Expr::Call(call) = expression {
            self.check_nested_minmax(call);
            self.check_basic_builtin_call(call);
            self.check_misplaced_format(call);
            self.check_dictionary_iteration(call);
            self.check_split_recommendation(call);
            self.check_generator_recommendation(call);
            self.check_stop_call(call);
            self.check_dict_comprehension_constructor(call);
            self.check_list_literal(call);
            self.check_dict_literal(call);
            self.check_super_brackets(call);
            self.check_abstract_construction(call);
            if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "NotImplemented" && !self.scopes.declared(name.id.as_str()))
            {
                self.emit(
                    "E1102",
                    "not-callable",
                    "NotImplemented is not callable",
                    call.range(),
                );
            }
            self.check_flow_call(call);
            self.check_quit_exit(call);
            self.check_resource_call(call);
            self.check_timeout_call(call);
            self.check_repeated_iterator_call(call);
            self.check_dataclass_field(call);
            self.check_deprecated_call(call);
            self.check_stdlib_call(call);
            self.check_call(call);
            self.check_stdlib_signature(call);
            self.check_logging(call);
            self.check_dunder_call(call);
            for keyword in &call.arguments.keywords {
                if let Some(name) = &keyword.arg {
                    self.check_non_ascii_name(name.as_str(), "Argument", keyword.range());
                }
            }
            for argument in &call.arguments.args {
                if let Expr::Starred(starred) = argument {
                    if matches!(
                        self.static_value(&starred.value),
                        super::values::Value::Scalar(_)
                            | super::values::Value::Number(_)
                            | super::values::Value::Boolean(_)
                    ) {
                        let value = &self.source.text[starred.value.range()];
                        self.emit(
                            "E1133",
                            "not-an-iterable",
                            &format!("Non-iterable value {value} is used in an iterating context"),
                            starred.value.range(),
                        );
                    }
                }
            }
            for keyword in &call.arguments.keywords {
                if keyword.arg.is_none()
                    && matches!(
                        self.static_value(&keyword.value),
                        super::values::Value::Scalar(_)
                            | super::values::Value::Number(_)
                            | super::values::Value::Boolean(_)
                            | super::values::Value::String(_)
                            | super::values::Value::Bytes(_)
                            | super::values::Value::Sequence(_, _)
                    )
                {
                    let value = &self.source.text[keyword.value.range()];
                    self.emit(
                        "E1134",
                        "not-a-mapping",
                        &format!("Non-mapping value {value} is used in a mapping context"),
                        keyword.value.range(),
                    );
                }
            }
        }
        self.check_string_format(expression);
        self.check_logging_string_interactions(expression);
        match expression {
            Expr::Lambda(lambda) => {
                self.check_lambda_effect(lambda);
                if let Some(parameters) = &lambda.parameters {
                    for parameter in parameters.iter() {
                        if let Some(default) = parameter.default() {
                            self.visit_expr(default);
                        }
                    }
                }
                self.scopes.enter_expression(lambda.parameters.as_deref());
                let resource_context = (self.resources.suppressed, self.resources.returned);
                self.resources.suppressed = 0;
                self.resources.returned = 0;
                self.visit_expr(&lambda.body);
                self.leave_scope();
                (self.resources.suppressed, self.resources.returned) = resource_context;
                return;
            }
            Expr::ListComp(comp) => {
                self.check_iteration_comprehensions(&comp.generators, &[&comp.elt]);
                self.check_comprehension(&comp.elt, &comp.generators, "list", comp.range());
                self.comprehension_body(&comp.generators, &[&comp.elt]);
                return;
            }
            Expr::SetComp(comp) => {
                self.check_iteration_comprehensions(&comp.generators, &[&comp.elt]);
                self.check_comprehension(&comp.elt, &comp.generators, "set", comp.range());
                self.comprehension_body(&comp.generators, &[&comp.elt]);
                return;
            }
            Expr::Generator(comp) => {
                self.check_iteration_comprehensions(&comp.generators, &[&comp.elt]);
                self.comprehension_body(&comp.generators, &[&comp.elt]);
                return;
            }
            Expr::DictComp(comp) => {
                let mut values = vec![comp.value.as_ref()];
                if let Some(key) = &comp.key {
                    values.insert(0, key.as_ref());
                }
                self.check_iteration_comprehensions(&comp.generators, &values);
                self.comprehension_body(&comp.generators, &values);
                return;
            }
            Expr::Attribute(attr) => {
                self.check_deprecated_attribute(attr);
                if attr.ctx.is_store() {
                    self.check_slot_store(attr);
                }
                if attr.ctx.is_load() || attr.ctx.is_del() {
                    self.check_protected_access(attr);
                }
                if !attr.ctx.is_store() || self.augmented_assignment {
                    self.check_object_member(attr);
                }
                if let Some(module) = self.module_expression(&attr.value) {
                    if (!attr.ctx.is_store() || self.augmented_assignment)
                        && self.modules.exists(self.source.project, &module)
                        && !self.native_member_guarded(attr)
                        && (self.modules.project_module(self.source.project, &module)
                            || !super::stdlib_facts::module(&module, self.modules.target())
                                .is_some_and(|module| module.dynamic))
                        && !self
                            .modules
                            .member(self.source.project, &module, attr.attr.as_str())
                    {
                        let (rule, symbol, message) = if self.modules.native(&module)
                            || self.modules.namespace(self.source.project, &module)
                        {
                            ("I1101", "c-extension-no-member", format!("Module '{module}' has no '{}' member, but source is unavailable. Consider adding this module to extension-pkg-allow-list if you want to perform analysis based on run-time introspection of living objects.", attr.attr))
                        } else {
                            (
                                "E1101",
                                "no-member",
                                format!(
                                    "Module '{module}' has no '{}' member{}",
                                    attr.attr,
                                    if self.modules.project_module(self.source.project, &module) {
                                        String::new()
                                    } else {
                                        super::stdlib_facts::missing_hint(
                                            &module,
                                            attr.attr.as_str(),
                                            self.modules.target(),
                                        )
                                    }
                                ),
                            )
                        };
                        self.emit(rule, symbol, &message, attr.range());
                    }
                }
            }
            Expr::Dict(dict) => {
                self.check_duplicate_dictionary(dict);
                self.check_hashable_dictionary(dict);
            }
            _ => {}
        }
        if let Expr::Name(name) = expression {
            if name.ctx.is_store() && self.augmented_assignment {
                self.scopes.use_name(name.id.as_str());
            }
            if name.ctx.is_load() || name.ctx.is_del() {
                if name.ctx.is_load() && self.loop_closures.contains(&name.start().to_u32()) {
                    self.emit(
                        "W0640",
                        "cell-var-from-loop",
                        &format!("Cell variable {} defined in loop", name.id),
                        name.range(),
                    );
                }
                self.check_loop_variable(name);
                let resolution =
                    if self.generic_declared(name.id.as_str()) { self.scopes.use_name(name.id.as_str()); Resolution::Defined }
                    else if self.initializing_tuple.contains(name.id.as_str()) { Resolution::Before }
                    else if self.initializing.contains(name.id.as_str()) && self.scopes.in_class() {
                        self.scopes.resolve_outer(name.id.as_str(), name.start())
                    } else if self.initializing.contains(name.id.as_str())
                        || (self.scopes.in_class()
                            && self
                                .class_stack
                                .last()
                                .is_some_and(|(class, _)| class.name == name.id.as_str())
                            && !(self.annotation_context
                                && self.modules.target() == mamba_language::application::quality_syntax::PythonTarget::Py314))
                    {
                        Resolution::Missing
                    } else if self.definitions.expired_handler_use(name.range()) {
                        self.scopes.resolve_unconsumed(name.id.as_str(),name.start())
                    } else if name.ctx.is_del() {
                        self.scopes.resolve_delete(name.id.as_str(),name.range())
                    } else {
                        self.scopes.resolve(name.id.as_str(), name.start())
                    };
                match resolution {
                    Resolution::Defined
                        if self.definitions.conditional_use(name.range()).is_some() =>
                    {
                        let possible = self.definitions.conditional_use(name.range()).unwrap();
                        self.emit(
                            if possible { "E0606" } else { "E0601" },
                            if possible {
                                "possibly-used-before-assignment"
                            } else {
                                "used-before-assignment"
                            },
                            &format!(
                                "{} variable '{}' before assignment",
                                if possible { "Possibly using" } else { "Using" },
                                name.id
                            ),
                            name.range(),
                        );
                    }
                    Resolution::Missing
                        if !self.modules.builtin(name.id.as_str())
                            && !self.generic_declared(name.id.as_str())
                            && !super::defaults::fixed().module_attribute(name.id.as_str()) =>
                    {
                        self.emit(
                            "E0602",
                            "undefined-variable",
                            &format!("Undefined variable '{}'", name.id),
                            name.range(),
                        )
                    }
                    Resolution::Before => self.emit(
                        "E0601",
                        "used-before-assignment",
                        &format!("Using variable '{}' before assignment", name.id),
                        name.range(),
                    ),
                    _ => {}
                }
            }
        }
        walk_expr(self, expression);
    }
}
