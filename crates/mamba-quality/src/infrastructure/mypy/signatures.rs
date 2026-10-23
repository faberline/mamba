//! Function declarations retain the parameter kinds independently of call binding.
use super::types::{self, Type};
use mamba_language::application::quality_syntax::python_ast::{
    Expr, Parameter as AstParameter, StmtFunctionDef,
};
use std::sync::Arc;
pub(super) mod compatibility;
pub(super) mod decorators;
mod generators;
pub(super) mod special_methods;
pub(super) mod suggestions;
mod type_parameters;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    PositionalOnly,
    PositionalOrKeyword,
    KeywordOnly,
    VariadicPositional,
    VariadicKeyword,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Parameter {
    pub name: String,
    pub ty: Option<Type>,
    pub kind: Kind,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum NarrowingKind {
    TypeGuard,
    TypeIs,
}

impl NarrowingKind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::TypeGuard => "TypeGuard",
            Self::TypeIs => "TypeIs",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Signature {
    pub name: Option<String>,
    pub parameters: Arc<[Parameter]>,
    pub returns: Option<Type>,
    pub narrowing: Option<(NarrowingKind, Type)>,
    pub return_explicit: bool,
    pub implicit: bool,
    // Positive provenance for a bare diagnostic name, not semantic ownership.
    pub plain_source_implicit_name: bool,
    pub ignored: bool,
    pub variables: types::Variables,
    pub generic: Arc<[Type]>,
    pub owner: Option<String>,
    pub constructor: bool,
    pub ellipsis: bool,
    pub overloads: Arc<[Arc<Signature>]>,
    pub bound_receiver: Option<String>,
    pub definition_module: Option<String>,
    pub native_symbol: Option<&'static str>,
}

impl Signature {
    pub(super) fn from_function(
        function: &StmtFunctionDef,
        context: &types::Context,
        inherited: &types::Variables,
        binders: &types::ReturnedCallableInput,
    ) -> Self {
        let variables = context.function_variables(function, inherited);
        let mut parameters = Vec::new();
        for (args, kind) in [
            (&function.parameters.posonlyargs, Kind::PositionalOnly),
            (&function.parameters.args, Kind::PositionalOrKeyword),
            (&function.parameters.kwonlyargs, Kind::KeywordOnly),
        ] {
            for parameter in args {
                let kind = if kind == Kind::PositionalOrKeyword
                    && (special_methods::positional_only(function.name.as_str())
                        || (parameter.parameter.name.as_str().starts_with("__")
                            && !parameter.parameter.name.as_str().ends_with("__")))
                {
                    Kind::PositionalOnly
                } else {
                    kind
                };
                parameters.push(Parameter::new(
                    &parameter.parameter,
                    kind,
                    parameter.default.is_none(),
                    context,
                    &variables,
                ));
            }
        }
        if let Some(parameter) = &function.parameters.vararg {
            parameters.push(Parameter::new(
                parameter,
                Kind::VariadicPositional,
                false,
                context,
                &variables,
            ));
        }
        if let Some(parameter) = &function.parameters.kwarg {
            parameters.push(Parameter::new(
                parameter,
                Kind::VariadicKeyword,
                false,
                context,
                &variables,
            ));
        }
        let ignored = no_type_check(function);
        let implicit = function.returns.is_none()
            && function
                .parameters
                .iter()
                .all(|parameter| parameter.annotation().is_none());
        let narrowing = function
            .returns
            .as_deref()
            .and_then(|expression| context.predicate_annotation(expression, &variables));
        let returns = if ignored || function.returns.is_none() {
            Some(Type::Any)
        } else {
            function
                .returns
                .as_deref()
                .and_then(|expression| context.annotation_with_variables(expression, &variables))
        };
        let (returns, returned_generic) = if ignored {
            (returns, None)
        } else {
            context.returned_callable_return(
                function,
                &variables,
                &parameters,
                returns,
                narrowing.map(|(_, ty)| ty),
                binders,
            )
        };
        let returns = if function.is_async && !Self::is_generator(function) && !ignored {
            returns.map(|result| context.coroutine(result))
        } else {
            returns
        };
        let mut generic = Vec::new();
        for parameter in &parameters {
            if let Some(ty) = parameter.ty {
                context.collect_variables(ty, &mut generic);
            }
        }
        if let Some(ty) = returns {
            context.collect_variables(ty, &mut generic);
        }
        if let Some((_, ty)) = narrowing {
            context.collect_variables(ty, &mut generic);
        }
        if let Some(returned_generic) = returned_generic {
            generic = returned_generic;
        }
        Self {
            name: Some(function.name.to_string()),
            generic: Self::bound_parameters(function, context, &parameters, generic).into(),
            parameters: parameters.into(),
            returns,
            narrowing,
            return_explicit: function.returns.is_some(),
            implicit,
            plain_source_implicit_name: implicit && function.decorator_list.is_empty(),
            ignored,
            variables,
            owner: None,
            constructor: false,
            ellipsis: false,
            overloads: Arc::from([]),
            bound_receiver: None,
            definition_module: None,
            native_symbol: None,
        }
    }

    pub(super) fn anonymous(&self) -> Self {
        if self.constructor {
            return self.clone();
        }
        Self {
            name: None,
            owner: None,
            plain_source_implicit_name: false,
            ..self.clone()
        }
    }

    pub(super) fn refresh_annotations(
        &self,
        function: &StmtFunctionDef,
        context: &types::Context,
        namespace: &types::Variables,
        binders: &types::ReturnedCallableInput,
    ) -> Self {
        let variables = context.refresh_function_variables(function, namespace, &self.variables);
        let parameters: Vec<_> = self
            .parameters
            .iter()
            .map(|parameter| {
                let mut parameter = parameter.clone();
                if let Some(annotation) = function
                    .parameters
                    .iter()
                    .find(|candidate| candidate.name().as_str() == parameter.name)
                    .and_then(|parameter| parameter.annotation())
                {
                    parameter.ty = context.annotation_with_variables(annotation, &variables);
                }
                parameter
            })
            .collect();
        let (returns, returned_generic) = if self.ignored || function.returns.is_none() {
            (self.returns, None)
        } else {
            let raw = function
                .returns
                .as_deref()
                .and_then(|annotation| context.annotation_with_variables(annotation, &variables));
            let (result, generic) = context.returned_callable_return(
                function,
                &variables,
                &parameters,
                raw,
                self.narrowing.map(|(_, ty)| ty),
                binders,
            );
            let result = if function.is_async && !Self::is_generator(function) {
                result.map(|ty| context.coroutine(ty))
            } else {
                result
            };
            (result, generic)
        };
        let mut generic = Vec::new();
        for parameter in &parameters {
            if let Some(ty) = parameter.ty {
                context.collect_variables(ty, &mut generic);
            }
        }
        if let Some(ty) = returns {
            context.collect_variables(ty, &mut generic);
        }
        if let Some(returned_generic) = returned_generic {
            generic = returned_generic;
        }
        Self {
            generic: Self::bound_parameters(function, context, &parameters, generic).into(),
            parameters: parameters.into(),
            returns,
            variables,
            ..self.clone()
        }
    }

    pub(super) fn for_function(&self) -> String {
        self.display_target()
            .map(|target| format!(" for {target}"))
            .unwrap_or_default()
    }

    pub(super) fn is_unbound_builtin_member(&self) -> bool {
        self.native_symbol == Some("builtins.unbound_member")
            || (self.definition_module.as_deref() == Some("builtins")
                && self.owner.is_some()
                && !self.constructor
                && self.bound_receiver.is_none())
    }

    pub(super) fn definition_note(&self) -> Option<String> {
        let module = self.definition_module.as_ref()?;
        if module == "builtins" {
            return None;
        }
        Some(self.name.as_ref().map_or_else(
            || format!("Called function defined in \"{module}\""),
            |name| format!("\"{name}\" defined in \"{module}\""),
        ))
    }

    pub(super) fn display_target(&self) -> Option<String> {
        let name = self.name.as_ref()?;
        Some(self.owner.as_ref().map_or_else(
            || format!("\"{name}\""),
            |owner| format!("\"{name}\" of \"{owner}\""),
        ))
    }

    pub(super) fn untyped_call_target(&self) -> Option<String> {
        if self.plain_source_implicit_name && self.implicit && !self.constructor {
            // Mypy's unannotated source callable starts with fdef.name.
            self.name.as_ref().map(|name| format!("\"{name}\""))
        } else {
            self.display_target()
        }
    }

    pub(super) fn bound(&self, implicit: usize) -> Self {
        Self {
            bound_receiver: if implicit > 0 {
                self.parameters
                    .first()
                    .map(|parameter| parameter.name.clone())
            } else {
                self.bound_receiver.clone()
            },
            parameters: self
                .parameters
                .iter()
                .skip(implicit)
                .cloned()
                .collect::<Vec<_>>()
                .into(),
            overloads: self
                .overloads
                .iter()
                .map(|signature| Arc::new(signature.bound(implicit)))
                .collect::<Vec<_>>()
                .into(),
            ..self.clone()
        }
    }

    pub(super) fn empty(name: String, returns: Type) -> Self {
        Self {
            name: Some(name),
            parameters: Arc::from([]),
            returns: Some(returns),
            narrowing: None,
            return_explicit: true,
            implicit: false,
            plain_source_implicit_name: false,
            ignored: false,
            variables: types::Variables::new(),
            generic: Arc::from([]),
            owner: None,
            constructor: true,
            ellipsis: false,
            overloads: Arc::from([]),
            bound_receiver: None,
            definition_module: None,
            native_symbol: None,
        }
    }

    pub(in crate::infrastructure::mypy) fn constructor_for(
        &self,
        name: &str,
        instance: Type,
    ) -> Self {
        Self {
            name: Some(name.into()),
            owner: None,
            plain_source_implicit_name: false,
            returns: Some(instance),
            constructor: true,
            bound_receiver: None,
            overloads: self
                .overloads
                .iter()
                .map(|variant| Arc::new(variant.constructor_for(name, instance)))
                .collect::<Vec<_>>()
                .into(),
            ..self.clone()
        }
    }
}

impl Parameter {
    fn new(
        parameter: &AstParameter,
        kind: Kind,
        required: bool,
        context: &types::Context,
        variables: &types::Variables,
    ) -> Self {
        Self {
            name: parameter.name.to_string(),
            ty: parameter
                .annotation
                .as_deref()
                .and_then(|expression| context.annotation_with_variables(expression, variables)),
            kind,
            required,
        }
    }
}

pub(super) fn no_type_check(function: &StmtFunctionDef) -> bool {
    function.decorator_list.iter().any(|decorator| {
        matches!(&decorator.expression, Expr::Name(name) if name.id.as_str() == "no_type_check")
    })
}
