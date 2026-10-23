//! Function declarations retain lexical entry, annotation use, and leave ordering.
use super::{docstring, scopes::Kind, Checker};
use mamba_language::application::quality_syntax::python_ast::{visitor::Visitor, StmtFunctionDef};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn walk_function_declaration(&mut self, function: &StmtFunctionDef) {
        self.repeated_iterators.enter();
        self.check_mutable_defaults(function);
        self.check_function_semantics(function);
        self.check_redefinition(
            function.name.range(),
            if self.scopes.in_class() {
                "method"
            } else {
                "function"
            },
        );
        let method = self.check_method(function);
        if method.is_none() {
            self.check_declaration_name(
                function.name.as_str(),
                "function",
                self.declaration_range(function.name.range()),
            );
        }
        self.check_threading_function(function);
        if !self.scopes.in_function()
            && !function.name.as_str().starts_with('_')
            && !docstring(&function.body)
            && !self.inherited_method_docstring(function)
        {
            self.emit(
                "C0116",
                "missing-function-docstring",
                "Missing function or method docstring",
                self.declaration_range(function.name.range()),
            );
        }
        self.check_function_header(function);
        self.check_function_design_header(function, method);
        self.check_empty_docstring(
            &function.body,
            if method.is_some() {
                "method"
            } else {
                "function"
            },
            self.declaration_range(function.name.range()),
        );
        self.check_non_ascii_function(function);
        self.check_deprecated_abstract_decorators(function);
        self.check_stdlib_decorators(function);
        for decorator in &function.decorator_list {
            self.visit_expr(&decorator.expression);
        }
        self.check_generic_shadowing(function.type_params.as_deref());
        self.enter_generic_names(function.type_params.as_deref());
        if let Some(parameters) = &function.type_params {
            self.visit_type_params(parameters);
        }
        for parameter in function.parameters.iter() {
            if let Some(default) = parameter.default() {
                self.visit_expr(default);
            }
            if let Some(annotation) = parameter.annotation() {
                self.check_annotation_names(annotation);
                let previous = self.annotation_context;
                self.annotation_context = true;
                self.visit_expr(annotation);
                self.annotation_context = previous;
            }
        }
        if let Some(returns) = &function.returns {
            self.check_annotation_names(returns);
            let previous = self.annotation_context;
            self.annotation_context = true;
            self.visit_expr(returns);
            self.annotation_context = previous;
        }
        self.scopes
            .enter(Kind::Function, &function.body, Some(&function.parameters));
        self.check_generator_cleanup(function);
        self.scopes.set_receiver(
            function
                .parameters
                .iter()
                .next()
                .map(|value| value.name().as_str()),
            method,
        );
        let receiver = self.bind_receiver(function, method);
        let previous_receiver = self.mandatory_receiver.clone();
        if method.is_some() {
            self.mandatory_receiver = receiver.clone().or_else(|| {
                let first = function.parameters.iter().next()?.name().as_str();
                (method == Some(super::classes::MethodKind::Static)
                    && (first == "self" || super::defaults::fixed().class_receiver(first)))
                .then(|| first.to_string())
            });
        }
        let previous_kind = self.method_kind;
        self.method_kind = method;
        if let Some(kind) = method {
            self.check_parent_delegation(function);
            self.check_method_contract(function, kind);
            self.check_hidden_method(function, kind);
            if function.name.as_str() == "__init__" {
                self.check_constructor_calls(function);
            }
        }
        self.check_bad_super(function, method);
        let previous_method = self.method_context.clone();
        if method.is_some() {
            self.method_context = Some(function.name.to_string());
        }
        self.scopes.use_nonlocals();
        self.check_builtin_shadowing();
        self.check_outer_shadowing();
        let resource_context = (self.resources.suppressed, self.resources.returned);
        self.resources.suppressed = usize::from(
            function.name.as_str() == "__enter__"
                || function.decorator_list.iter().any(|value| {
                    self.stdlib_name(&value.expression).as_deref()
                        == Some("contextlib.contextmanager")
                }),
        );
        self.resources.returned = 0;
        self.visit_body(&function.body);
        self.check_function_design_end(function);
        if !function.is_async {
            self.finish_nested_blocks();
        }
        self.check_consistent_returns(function);
        self.check_useless_return(function);
        self.finish_resource_scope();
        (self.resources.suppressed, self.resources.returned) = resource_context;
        self.check_unused_function(function, receiver.as_deref());
        self.check_unused_type_parameters(function);
        self.check_unresolved_metaclasses(&function.body);
        self.leave_scope();
        self.generic_names.pop();
        self.method_context = previous_method;
        self.method_kind = previous_kind;
        self.mandatory_receiver = previous_receiver;
        self.repeated_iterators.leave();
    }
}
