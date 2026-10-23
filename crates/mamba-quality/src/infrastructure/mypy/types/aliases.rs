//! Alias declarations retain stable parameter identities and their expanded target.
use super::*;
use mamba_language::application::quality_syntax;
use quality_syntax::python_ast::{TypeParam, TypeParams};
use std::sync::Arc;

#[derive(Clone, PartialEq, Eq)]
pub(in crate::infrastructure::mypy) struct Alias {
    pub name: String,
    pub parameters: Vec<Type>,
    pub target: Option<Type>,
    pub modern: bool,
    pub implicit: bool,
    pub explicit_arguments: bool,
    pub bare_class: Option<u32>,
    pub invalid_variable: Option<(String, ruff_text_size::TextRange)>,
}

#[derive(Default)]
pub(super) struct Registry {
    keys: BTreeMap<(String, u32), u32>,
    entries: Vec<Arc<Alias>>,
    pub(super) active: std::collections::BTreeSet<u32>,
    pub(super) building: usize,
    pub(super) comparisons: std::collections::BTreeSet<(Type, Type)>,
    pub(super) constraints: std::collections::BTreeSet<(Type, Type, u8, bool)>,
    pub(super) reveals: std::collections::BTreeSet<u32>,
    pub(super) origins: BTreeMap<Type, (u32, Vec<Type>)>,
}

#[derive(PartialEq, Eq)]
pub(in crate::infrastructure::mypy) struct LoopAliasState {
    declarations: BTreeMap<u32, Arc<Alias>>,
}

impl Context {
    pub(in crate::infrastructure::mypy) fn loop_alias_state(
        &self,
        namespace: &str,
    ) -> LoopAliasState {
        let aliases = self.aliases.borrow();
        LoopAliasState {
            declarations: aliases
                .keys
                .range((namespace.to_owned(), 0)..=(namespace.to_owned(), u32::MAX))
                .map(|(_, &id)| (id, aliases.entries[id as usize].clone()))
                .collect(),
        }
    }

    // Stable declaration caches retain alias nodes, not these expansion origins.
    pub(in crate::infrastructure::mypy) fn trial_alias_origins<T>(
        &self,
        trial: impl FnOnce() -> (T, bool),
    ) -> T {
        let previous = self.aliases.borrow().origins.clone();
        let (result, commit) = trial();
        if !commit {
            self.aliases.borrow_mut().origins = previous;
        }
        result
    }

    pub(in crate::infrastructure::mypy) fn set_embedded_alias_parameters(
        &self,
        id: u32,
        parameters: Vec<Type>,
    ) {
        let old = self.alias(id);
        self.aliases.borrow_mut().entries[id as usize] = Arc::new(Alias {
            parameters,
            ..(*old).clone()
        });
    }
    pub(in crate::infrastructure::mypy) fn set_embedded_alias_target(&self, id: u32, target: Type) {
        let old = self.alias(id);
        self.aliases.borrow_mut().entries[id as usize] = Arc::new(Alias {
            target: Some(target),
            ..(*old).clone()
        });
    }

    pub(in crate::infrastructure::mypy) fn declare_alias(
        &self,
        namespace: &str,
        name: &str,
        offset: u32,
        modern: bool,
        implicit: bool,
    ) -> u32 {
        let key = (namespace.to_owned(), offset);
        if let Some(&id) = self.aliases.borrow().keys.get(&key) {
            return id;
        }
        let mut aliases = self.aliases.borrow_mut();
        let id = u32::try_from(aliases.entries.len()).expect("alias IDs fit in u32");
        aliases.keys.insert(key, id);
        aliases.entries.push(Arc::new(Alias {
            name: name.into(),
            parameters: Vec::new(),
            target: None,
            modern,
            implicit,
            explicit_arguments: false,
            bare_class: None,
            invalid_variable: None,
        }));
        id
    }

    pub(in crate::infrastructure::mypy) fn alias(&self, id: u32) -> Arc<Alias> {
        self.aliases.borrow().entries[id as usize].clone()
    }

    pub(in crate::infrastructure::mypy) fn alias_id(&self, ty: Type) -> Option<u32> {
        match self.node(ty)?.as_ref() {
            Node::Alias(id, _) => Some(*id),
            _ => None,
        }
    }

    pub(in crate::infrastructure::mypy) fn alias_marker(&self, id: u32) -> Type {
        self.intern(Node::Alias(id, self.alias(id).target))
    }

    pub(in crate::infrastructure::mypy) fn populate_alias(
        &self,
        id: u32,
        parameters: Option<&TypeParams>,
        value: &Expr,
        inherited: &Variables,
        source_class: Option<u32>,
    ) {
        let old = self.alias(id);
        let mut variables = inherited.clone();
        let explicit_arguments = matches!(value, Expr::Subscript(_))
            || self
                .annotation_binding(value, &variables)
                .and_then(|ty| self.alias_id(ty))
                .is_some_and(|id| self.alias(id).explicit_arguments);
        let bare_class = (!old.modern)
            .then(|| {
                source_class.or_else(|| {
                    self.annotation_binding(value, &variables)
                        .and_then(|ty| self.alias_id(ty))
                        .and_then(|id| self.alias(id).bare_class)
                })
            })
            .flatten();
        let mut generic = Vec::new();
        if let Some(parameters) = parameters {
            for parameter in &parameters.type_params {
                if let TypeParam::TypeVarTuple(parameter) = parameter {
                    let ty =
                        old.parameters
                            .get(generic.len())
                            .copied()
                            .filter(|&ty| self.is_tuple_variable(ty))
                            .unwrap_or_else(|| {
                                let default = parameter.default.as_deref().and_then(|value| {
                                    self.tuple_variable_default(value, &variables)
                                });
                                self.tuple_variable(parameter.name.to_string(), default)
                            });
                    variables.insert(parameter.name.to_string(), ty);
                    generic.push(ty);
                    continue;
                }
                if let TypeParam::ParamSpec(parameter) = parameter {
                    let ty =
                        old.parameters
                            .get(generic.len())
                            .copied()
                            .filter(|&ty| self.is_parameter_spec(ty))
                            .unwrap_or_else(|| {
                                let default = parameter.default.as_deref().and_then(|value| {
                                    self.parameter_spec_default(value, &variables)
                                });
                                self.parameter_spec(parameter.name.to_string(), default)
                            });
                    variables.insert(parameter.name.to_string(), ty);
                    generic.push(ty);
                    continue;
                }
                let TypeParam::TypeVar(parameter) = parameter else {
                    continue;
                };
                let ty = old
                    .parameters
                    .get(generic.len())
                    .copied()
                    .unwrap_or_else(|| {
                        let (bound, values) = match parameter.bound.as_deref() {
                            Some(Expr::Tuple(tuple)) => (
                                Type::Object,
                                tuple
                                    .elts
                                    .iter()
                                    .filter_map(|item| {
                                        self.annotation_with_variables(item, &variables)
                                    })
                                    .collect(),
                            ),
                            Some(bound) => (
                                self.annotation_with_variables(bound, &variables)
                                    .unwrap_or(Type::Object),
                                Vec::new(),
                            ),
                            None => (Type::Object, Vec::new()),
                        };
                        let default = parameter.default.as_deref().and_then(|default| {
                            self.annotation_with_variables(default, &variables)
                        });
                        self.variable(parameter.name.to_string(), bound, values, default)
                    });
                variables.insert(parameter.name.to_string(), ty);
                generic.push(ty);
            }
        }
        if old.modern {
            self.aliases.borrow_mut().entries[id as usize] = Arc::new(Alias {
                parameters: generic.clone(),
                ..(*old).clone()
            });
        }
        self.aliases.borrow_mut().building += 1;
        let target = if let Some(class) = bare_class
            .map(|id| self.class(id))
            .filter(|class| class.namespace.starts_with("typing."))
        {
            Some(self.applied_class(
                self.class_id(class.instance).unwrap(),
                class.generic.clone(),
                false,
            ))
        } else {
            self.annotation_with_variables(value, &variables)
        };
        self.aliases.borrow_mut().building -= 1;
        let invalid_variable = old
            .implicit
            .then(|| self.invalid_alias_variable(value, &variables))
            .flatten();
        if !old.modern {
            if let Some(target) = target {
                self.collect_variables(target, &mut generic);
            }
        }
        self.aliases.borrow_mut().entries[id as usize] = Arc::new(Alias {
            parameters: generic,
            target,
            invalid_variable,
            bare_class,
            explicit_arguments,
            ..(*old).clone()
        });
    }

    pub(in crate::infrastructure::mypy) fn expand_alias(
        &self,
        id: u32,
        mut arguments: Vec<Type>,
    ) -> Option<Type> {
        if !self.aliases.borrow_mut().active.insert(id) {
            return Some(self.intern(Node::AliasInstance(id, arguments)));
        }
        let alias = self.alias(id);
        if alias.invalid_variable.is_some() {
            self.aliases.borrow_mut().active.remove(&id);
            return Some(Type::Any);
        }
        if self.unguarded_alias_cycle(id) {
            self.aliases.borrow_mut().active.remove(&id);
            return Some(Type::Any);
        }
        let minimum = alias.parameters.iter().filter(|&&ty| self.node(ty).is_some_and(|node| matches!(node.as_ref(), Node::Variable(variable) if variable.default.is_none()))).count();
        if arguments.len() < minimum || arguments.len() > alias.parameters.len() {
            arguments.clear();
        }
        let mut solutions = BTreeMap::new();
        for (index, &parameter) in alias.parameters.iter().enumerate() {
            let argument = arguments.get(index).copied().unwrap_or_else(|| {
                self.node(parameter)
                    .and_then(|node| {
                        if let Node::Variable(variable) = node.as_ref() {
                            variable.default
                        } else {
                            None
                        }
                    })
                    .map(|default| self.substitute(default, &solutions))
                    .unwrap_or_else(|| self.default_generic_argument(parameter))
            });
            solutions.insert(parameter, argument);
        }
        let result = alias
            .target
            .map(|target| self.substitute(target, &solutions))
            .map(|target| self.expand_alias_nodes(target));
        self.aliases.borrow_mut().active.remove(&id);
        if let Some(target) = result.filter(|&target| !target.is_any()) {
            if self.recursive_alias(id) {
                self.aliases.borrow_mut().origins.insert(
                    target,
                    (
                        id,
                        alias
                            .parameters
                            .iter()
                            .map(|parameter| solutions[parameter])
                            .collect(),
                    ),
                );
            }
        }
        result
    }

    pub(in crate::infrastructure::mypy) fn annotation_binding(
        &self,
        expression: &Expr,
        variables: &VariableMap,
    ) -> Option<Type> {
        let name = match expression {
            Expr::Name(name) => name.id.to_string(),
            Expr::Attribute(value) => format!(
                "{}.{}",
                Self::annotation_binding_name(&value.value)?,
                value.attr
            ),
            _ => return None,
        };
        variables.get(&name).copied()
    }

    fn annotation_binding_name(expression: &Expr) -> Option<String> {
        match expression {
            Expr::Name(name) => Some(name.id.to_string()),
            Expr::Attribute(value) => Some(format!(
                "{}.{}",
                Self::annotation_binding_name(&value.value)?,
                value.attr
            )),
            _ => None,
        }
    }

    pub(super) fn annotation_alias_value(&self, ty: Type) -> Option<Type> {
        if self
            .node(ty)
            .is_some_and(|node| matches!(node.as_ref(), Node::TypingForm(form) if form == "Type"))
        {
            return Some(self.intern(Node::Container(Container::TypeObject, vec![Type::Any])));
        }
        if self.node(ty).is_some_and(
            |node| matches!(node.as_ref(), Node::TypingForm(form) if matches!(form.as_str(), "replace" | "fields" | "asdict" | "astuple") || form.starts_with("dataclass-transform:") || form.starts_with("dataclass-field:")),
        ) {
            return None;
        }
        self.alias_id(ty).map_or_else(
            || Some(self.annotation_value(ty)),
            |id| {
                if self.aliases.borrow().building != 0 {
                    Some(self.intern(Node::AliasInstance(id, Vec::new())))
                } else {
                    self.expand_alias(id, Vec::new())
                }
            },
        )
    }

    pub(super) fn alias_runtime_value(&self, ty: Type) -> Type {
        let Some(id) = self.alias_id(ty) else {
            return ty;
        };
        if self.alias(id).modern {
            return ty;
        }
        self.expand_alias(id, Vec::new())
            .filter(|&target| {
                self.class_id(target).is_some() || self.builtin_scalar(target).is_some()
            })
            .map_or(ty, |target| self.class_value_type(target))
    }

    fn invalid_alias_variable(
        &self,
        value: &Expr,
        variables: &VariableMap,
    ) -> Option<(String, ruff_text_size::TextRange)> {
        use ruff_text_size::Ranged;
        match value {
            Expr::Name(_) | Expr::Attribute(_) => self
                .annotation_binding(value, variables)
                .and_then(|ty| self.node(ty))
                .and_then(|node| {
                    if let Node::Variable(variable) = node.as_ref() {
                        Some((variable.name.clone(), value.range()))
                    } else {
                        None
                    }
                }),
            Expr::BinOp(binary) if binary.op == Operator::BitOr => self
                .invalid_alias_variable(&binary.left, variables)
                .or_else(|| self.invalid_alias_variable(&binary.right, variables)),
            _ => None,
        }
    }
}
