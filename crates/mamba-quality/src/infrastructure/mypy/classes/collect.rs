//! Fields and method declarations are inferred without running class bodies.
use super::super::types;
use super::*;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::Visitor, Expr, Stmt, StmtClassDef, StmtFunctionDef,
};
use ruff_text_size::Ranged;
mod attributes;
mod layout;
pub(super) mod partial_none;
mod patterns;
mod qualifiers;
mod values;
use attributes::Attributes;

type CallResolver<'a> = dyn Fn(&Expr, &BTreeMap<String, Type>) -> Option<Type> + 'a;

// This callback borrows the live source Scope only for the current population.
type MethodBinderInput<'a> = dyn Fn(
        u32,
        &StmtClassDef,
        &StmtFunctionDef,
        &types::Variables,
        &[Type],
    ) -> types::ReturnedCallableInput
    + 'a;

pub(super) fn last_name(expression: &Expr) -> Option<&str> {
    match expression {
        Expr::Name(name) => Some(name.id.as_str()),
        Expr::Attribute(attribute) => Some(attribute.attr.as_str()),
        _ => None,
    }
}

pub(super) fn method_kind(function: &StmtFunctionDef) -> MethodKind {
    let names: Vec<_> = function
        .decorator_list
        .iter()
        .filter_map(|decorator| last_name(&decorator.expression))
        .collect();
    if names.contains(&"staticmethod")
        || (function.name.as_str() == "__new__" && !names.contains(&"classmethod"))
    {
        MethodKind::Static
    } else if names.contains(&"classmethod") {
        MethodKind::Class
    } else if names.contains(&"property") {
        MethodKind::Property
    } else {
        MethodKind::Instance
    }
}

impl Context {
    pub(in crate::infrastructure::mypy) fn abstract_method(
        &self,
        function: &StmtFunctionDef,
        variables: &types::Variables,
    ) -> bool {
        function.decorator_list.iter().any(|decorator| {
            self.typing_form(&decorator.expression, variables)
                .as_deref()
                == Some("abstractmethod")
        })
    }
    pub(in crate::infrastructure::mypy) fn populate_class(
        &self,
        id: u32,
        statement: &StmtClassDef,
        variables: &types::Variables,
        overload_aliases: &std::collections::BTreeSet<String>,
    ) -> CollectionMetadata {
        self.populate_class_with_missing_bases(
            id,
            statement,
            variables,
            overload_aliases,
            &BTreeMap::new(),
            &[],
        )
    }

    pub(in crate::infrastructure::mypy) fn populate_class_with_missing_bases(
        &self,
        id: u32,
        statement: &StmtClassDef,
        variables: &types::Variables,
        overload_aliases: &std::collections::BTreeSet<String>,
        missing_bases: &BTreeMap<u32, AnyBaseOrigin>,
        generic_bases: &[TextSize],
    ) -> CollectionMetadata {
        self.populate_class_with_source_policy(
            id,
            statement,
            variables,
            overload_aliases,
            (missing_bases, generic_bases),
            false,
        )
    }

    pub(in crate::infrastructure::mypy) fn populate_class_with_source_policy(
        &self,
        id: u32,
        statement: &StmtClassDef,
        variables: &types::Variables,
        overload_aliases: &std::collections::BTreeSet<String>,
        bases: (&BTreeMap<u32, AnyBaseOrigin>, &[TextSize]),
        terminal_none_source: bool,
    ) -> CollectionMetadata {
        self.populate_class_with_method_inputs(
            id,
            statement,
            variables,
            overload_aliases,
            bases,
            (terminal_none_source, None),
        )
    }

    pub(in crate::infrastructure::mypy) fn populate_class_with_method_inputs(
        &self,
        id: u32,
        statement: &StmtClassDef,
        variables: &types::Variables,
        overload_aliases: &std::collections::BTreeSet<String>,
        bases: (&BTreeMap<u32, AnyBaseOrigin>, &[TextSize]),
        inputs: (bool, Option<&MethodBinderInput<'_>>),
    ) -> CollectionMetadata {
        let (terminal_none_source, method_input) = inputs;
        let (missing_bases, generic_bases) = bases;
        self.classes.borrow_mut().populated_class_bases.remove(&id);
        let previous = self.class(id);
        let (variables, generic) = self.class_variables(id, statement, variables);
        let variables = &variables;
        self.populate_class_layout(id, statement, variables, &previous.namespace);
        let mut class = Class {
            name: previous.name.clone(),
            namespace: previous.namespace.clone(),
            instance: previous.instance,
            object: previous.object,
            bases: Vec::new(),
            base_types: Vec::new(),
            any_base_origins: Vec::new(),
            scalar_bases: Vec::new(),
            offset: previous.offset,
            fields: BTreeMap::new(),
            decorated_fields: std::collections::BTreeSet::new(),
            cached_properties: std::collections::BTreeSet::new(),
            erased_members: std::collections::BTreeSet::new(),
            methods: BTreeMap::new(),
            plugin: false,
            pydantic: None,
            final_class: statement
                .decorator_list
                .iter()
                .any(|decorator| last_name(&decorator.expression) == Some("final")),
            typed_dict: self.class_typed_dict(statement, variables),
            dataclass: None,
            protocol: statement.arguments.as_ref().is_some_and(|arguments| {
                arguments.args.iter().any(|base| {
                    self.typing_form(
                        if let Expr::Subscript(base) = base {
                            &base.value
                        } else {
                            base
                        },
                        variables,
                    )
                    .as_deref()
                        == Some("Protocol")
                })
            }),
            runtime_protocol: statement.decorator_list.iter().any(|decorator| {
                self.typing_form(&decorator.expression, variables)
                    .as_deref()
                    == Some("runtime_checkable")
            }),
            generic,
        };
        self.replace_class(id, class.clone());
        let bases_known = self.populate_class_bases(
            &mut class,
            statement,
            variables,
            missing_bases,
            generic_bases,
        );
        let environment = BTreeMap::new();
        let overload_aliases = super::super::signatures::decorators::aliases(
            &statement.body,
            overload_aliases.clone(),
        );
        let mut metadata = CollectionMetadata::default();
        let declarations =
            super::statements::collect(&statement.body, self.module_target(&class.namespace));
        for statement in &declarations {
            match statement {
                Stmt::ClassDef(nested) => {
                    let id = self.declare_class(
                        &class.namespace,
                        &format!("{}.{}", class.name, nested.name),
                        nested.name.start().to_u32(),
                    );
                    let nested_class = self.class(id);
                    let mut variables = variables.clone();
                    variables.insert(nested.name.to_string(), nested_class.instance);
                    self.populate_class(id, nested, &variables, &overload_aliases);
                    class.fields.insert(
                        nested.name.to_string(),
                        Field {
                            ty: Some(nested_class.object),
                            needs_annotation: false,
                            class_access: true,
                            readonly: false,
                            final_attribute: false,
                            class_var: false,
                            abstract_field: false,
                        },
                    );
                }
                Stmt::AnnAssign(assignment) => {
                    if let Expr::Name(name) = assignment.target.as_ref() {
                        metadata.annotated_fields.insert(name.id.to_string());
                        let final_attribute =
                            qualifiers::is_final(self, &assignment.annotation, variables);
                        let class_var = matches!(
                            assignment.annotation.as_ref(),
                            Expr::Subscript(subscript)
                                if self.typing_form(&subscript.value, variables).as_deref()
                                    == Some("ClassVar")
                        );
                        class.fields.entry(name.id.to_string()).or_insert(Field {
                            needs_annotation: false,
                            ty: self
                                .annotation_with_variables(&assignment.annotation, variables)
                                .or_else(|| {
                                    final_attribute
                                        .then(|| {
                                            assignment.value.as_deref().and_then(|value| {
                                                self.infer_with_calls(value, &environment, &|_| {
                                                    None
                                                })
                                            })
                                        })
                                        .flatten()
                                }),
                            class_access: assignment.value.is_some() || class_var,
                            readonly: final_attribute,
                            final_attribute,
                            class_var,
                            abstract_field: class.protocol && assignment.value.is_none(),
                        });
                    }
                }
                Stmt::Assign(assignment) => {
                    let ty = self
                        .infer_with_calls(&assignment.value, &environment, &|_| None)
                        .or_else(|| self.declared_class_value(&assignment.value, variables));
                    for target in &assignment.targets {
                        if let Expr::Name(name) = target {
                            class.fields.entry(name.id.to_string()).or_insert(Field {
                                ty,
                                needs_annotation: false,
                                class_access: true,
                                readonly: false,
                                final_attribute: false,
                                class_var: false,
                                abstract_field: false,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        let definition = statement;
        for statement in declarations {
            let Stmt::FunctionDef(function) = statement else {
                continue;
            };
            if function.name.as_str() == "__init__"
                && !super::super::signatures::decorators::overload(function, &overload_aliases)
            {
                metadata.constructor_offset = Some(function.name.start());
            }
            let kind = super::properties::kind(&mut class, self, function, variables);
            let binders = method_input.map_or_else(
                || {
                    self.returned_callable_input(
                        function,
                        &class.namespace,
                        class.generic.iter().copied().collect(),
                        false,
                        "class",
                        &|_| false,
                    )
                },
                |input| input(id, definition, function, variables, &class.generic),
            );
            let mut signature = Signature::from_function(function, self, variables, &binders);
            signature.generic = signature
                .generic
                .iter()
                .copied()
                .filter(|ty| !class.generic.contains(ty))
                .collect::<Vec<_>>()
                .into();
            signature.owner = Some(class.name.clone());
            self.bind_source_self_receiver(&mut signature, function, kind);
            let mut parameters = signature.parameters.to_vec();
            if kind != MethodKind::Static || function.name.as_str() == "__new__" {
                if let Some(first) = parameters.first_mut() {
                    first.ty = first.ty.or(Some(
                        if kind == MethodKind::Class || function.name.as_str() == "__new__" {
                            class.object
                        } else {
                            class.instance
                        },
                    ));
                }
            }
            signature.parameters = parameters.clone().into();
            let variants = class
                .methods
                .get(function.name.as_str())
                .map(|method| method.signature.overloads.to_vec())
                .unwrap_or_default();
            if super::super::signatures::decorators::overload(function, &overload_aliases) {
                signature.overloads = variants
                    .into_iter()
                    .chain(std::iter::once(Arc::new(signature.clone())))
                    .collect::<Vec<_>>()
                    .into();
            } else {
                signature.overloads = variants.into();
            }
            if function.name.as_str() == "__init__" && function.returns.is_none() {
                signature.returns = Some(Type::None);
            }
            if kind == MethodKind::Instance || kind == MethodKind::Property {
                if let Some(receiver) = function.parameters.iter().next() {
                    let mut collector = Attributes {
                        receiver: receiver.name().as_str(),
                        context: self,
                        variables: &signature.variables,
                        environment: parameters
                            .iter()
                            .map(|parameter| {
                                (parameter.name.clone(), parameter.ty.unwrap_or(Type::Any))
                            })
                            .collect(),
                        fields: &mut class.fields,
                        annotated: &mut metadata.annotated_fields,
                        first_declarations: &mut metadata.first_field_declarations,
                        call_resolver: None,
                    };
                    collector.visit_body(&function.body);
                }
            }
            let setter = function.decorator_list.iter().any(|decorator| {
                matches!(&decorator.expression, Expr::Attribute(attribute)
                    if attribute.attr.as_str() == "setter"
                    && matches!(attribute.value.as_ref(), Expr::Name(name)
                        if name.id.as_str() == function.name.as_str()))
            });
            if setter {
                if let Some(property) = class.methods.get_mut(function.name.as_str()) {
                    if property.kind == MethodKind::Property {
                        property.setter = Some(Arc::new(signature));
                        continue;
                    }
                }
            }
            class.methods.insert(
                function.name.to_string(),
                Method {
                    signature: Arc::new(signature),
                    kind,
                    setter: None,
                    final_method: function
                        .decorator_list
                        .iter()
                        .any(|decorator| last_name(&decorator.expression) == Some("final")),
                    abstractness: if self.abstract_method(function, variables) {
                        Abstractness::Explicit
                    } else if class.protocol
                        && !self.module_is_stub(&class.namespace)
                        && function.body.iter().all(|statement| {
                            matches!(statement, Stmt::Pass(_))
                                || matches!(statement, Stmt::Expr(expression)
                                    if matches!(expression.value.as_ref(), Expr::EllipsisLiteral(_) | Expr::StringLiteral(_)))
                        })
                    {
                        Abstractness::Implicit
                    } else {
                        Abstractness::Concrete
                    },
                },
            );
        }
        self.replace_class(id, class.clone());
        self.populate_class_pattern_values(id, statement, &class);
        self.populate_dataclass(&mut class, statement, variables);
        partial_none::collected(
            self,
            id,
            definition,
            &metadata,
            &mut class,
            terminal_none_source && bases_known,
        );
        self.replace_class(id, class);
        self.classes
            .borrow_mut()
            .populated_class_bases
            .insert(id, bases_known);
        metadata
    }

    pub(in crate::infrastructure::mypy) fn constructor_none_plan(
        &self,
        id: u32,
        definition: &StmtClassDef,
        sha256: &str,
    ) -> Option<ConstructorNonePlan> {
        partial_none::admit(self, id, definition, sha256)
    }

    pub(in crate::infrastructure::mypy) fn begin_constructor_none(
        &self,
        plan: ConstructorNonePlan,
    ) -> Option<ConstructorNoneFrame> {
        partial_none::begin(self, plan)
    }

    pub(in crate::infrastructure::mypy) fn record_constructor_none(
        &self,
        frame: &mut ConstructorNoneFrame,
        assignment: &mamba_language::application::quality_syntax::python_ast::StmtAssign,
        actual: Option<Type>,
    ) {
        partial_none::reached(self, frame, assignment, actual);
    }

    pub(in crate::infrastructure::mypy) fn finish_constructor_none(
        &self,
        frame: ConstructorNoneFrame,
    ) {
        partial_none::finish(self, frame);
    }

    pub(in crate::infrastructure::mypy) fn constructor_none_order(
        &self,
        plan: &ConstructorNonePlan,
    ) {
        partial_none::before_other_method(self, plan);
    }

    pub(in crate::infrastructure::mypy) fn constructor_none_gap(&self, id: u32, reason: &str) {
        partial_none::gap(self, id, reason);
    }

    pub(in crate::infrastructure::mypy) fn populate_constructor_fields(
        &self,
        id: u32,
        function: &StmtFunctionDef,
        variables: &types::Variables,
        environment: &BTreeMap<String, Type>,
        metadata: &CollectionMetadata,
        call_resolver: &CallResolver<'_>,
    ) {
        let Some(receiver) = function.parameters.iter().next() else {
            return;
        };
        let mut inferred = BTreeMap::new();
        let mut candidate_metadata = CollectionMetadata::default();
        {
            let mut collector = Attributes {
                receiver: receiver.name().as_str(),
                context: self,
                variables,
                environment: environment.clone(),
                fields: &mut inferred,
                annotated: &mut candidate_metadata.annotated_fields,
                first_declarations: &mut candidate_metadata.first_field_declarations,
                call_resolver: Some(call_resolver),
            };
            collector.visit_body(&function.body);
        }

        let mut class = (*self.class(id)).clone();
        let class_variables: std::collections::BTreeSet<_> =
            class.generic.iter().copied().collect();
        for (name, candidate) in inferred {
            if candidate_metadata.annotated_fields.contains(&name)
                || metadata.annotated_fields.contains(&name)
            {
                continue;
            }
            if metadata.first_field_declarations.get(&name)
                != candidate_metadata.first_field_declarations.get(&name)
            {
                continue;
            }
            let Some(ty) = candidate.ty else {
                continue;
            };
            if candidate.class_access || candidate.class_var || candidate.needs_annotation {
                continue;
            }
            if self.class_id(ty).is_none() || self.is_class_value(ty) {
                continue;
            }
            let mut variables = Vec::new();
            self.collect_variables(ty, &mut variables);
            if variables
                .iter()
                .any(|variable| !class_variables.contains(variable))
            {
                continue;
            }
            let Some(field) = class.fields.get_mut(&name) else {
                continue;
            };
            if field.ty.is_some() || field.class_access || field.class_var || field.needs_annotation
            {
                continue;
            }
            field.ty = Some(ty);
        }
        self.replace_class(id, class);
    }
}
