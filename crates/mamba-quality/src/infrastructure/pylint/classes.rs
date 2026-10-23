//! Class declarations retain methods, descriptors, and instance assignments.
use super::{
    callables::{self, Signature, Target},
    values::{self, Value},
};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt, StmtClassDef, StmtFunctionDef,
};
use ruff_text_size::{Ranged, TextRange};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
pub(super) mod declarations;
mod mutations;
mod slots;
mod wrappers;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MethodKind {
    Instance,
    Static,
    Class,
    Property,
}

pub(super) fn method_kind(function: &StmtFunctionDef) -> MethodKind {
    if matches!(
        function.name.as_str(),
        "__new__" | "__init_subclass__" | "__class_getitem__"
    ) {
        return MethodKind::Class;
    }
    let names: Vec<_> = function
        .decorator_list
        .iter()
        .filter_map(|decorator| values::path(&decorator.expression))
        .filter_map(|names| names.last().cloned())
        .collect();
    if names.iter().any(|name| name == "staticmethod") {
        MethodKind::Static
    } else if names.iter().any(|name| name == "classmethod") {
        MethodKind::Class
    } else if names.iter().any(|name| name == "property") {
        MethodKind::Property
    } else {
        MethodKind::Instance
    }
}

#[derive(Clone)]
pub(super) struct Method {
    pub signature: Rc<Signature>,
    pub kind: MethodKind,
    pub value: Value,
}

#[derive(Clone)]
pub(super) struct AttributeDefinition {
    pub name: String,
    pub method: String,
    pub range: TextRange,
    pub setter: bool,
    pub module: Option<String>,
}

#[derive(Clone)]
pub(super) struct Class {
    pub name: String,
    pub identity: (String, u32),
    pub bases: Vec<Expr>,
    pub decorators: Vec<Expr>,
    pub metaclass: Option<Expr>,
    pub members: BTreeMap<String, Target>,
    pub first_members: BTreeMap<String, Target>,
    pub methods: BTreeMap<String, Method>,
    pub design_methods: BTreeSet<String>,
    pub attributes: BTreeMap<String, Target>,
    pub expressions: BTreeMap<String, Expr>,
    pub attribute_expressions: BTreeMap<String, Expr>,
    pub attribute_definitions: Vec<AttributeDefinition>,
    pub slot_values: Vec<Expr>,
    pub slot_check_values: Vec<Expr>,
    pub annotation_only: BTreeSet<String>,
    pub declared_members: BTreeSet<String>,
}

#[derive(Default)]
struct Attributes {
    receiver: String,
    values: BTreeMap<String, Target>,
    method: String,
    definitions: Vec<AttributeDefinition>,
    setter: bool,
    expressions: BTreeMap<String, Expr>,
}
impl Attributes {
    fn assign(&mut self, target: &Expr, value: Option<&Expr>) {
        let items = match target {
            Expr::Tuple(tuple) => Some(&tuple.elts),
            Expr::List(list) => Some(&list.elts),
            _ => None,
        };
        if let Some(items) = items {
            let values = match value {
                Some(Expr::Tuple(tuple)) => Some(&tuple.elts),
                Some(Expr::List(list)) => Some(&list.elts),
                _ => None,
            };
            for (index, target) in items.iter().enumerate() {
                self.assign(target, values.and_then(|values| values.get(index)));
            }
            return;
        }
        if let Expr::Starred(starred) = target {
            self.assign(&starred.value, None);
            return;
        }
        if let Expr::Attribute(attribute) = target {
            if matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == self.receiver)
            {
                self.definitions.push(AttributeDefinition {
                    module: None,
                    name: attribute.attr.to_string(),
                    method: self.method.clone(),
                    range: attribute.range(),
                    setter: self.setter,
                });
                self.values.insert(
                    attribute.attr.to_string(),
                    Target::Value(Rc::new(
                        value.map(Value::from_expression).unwrap_or(Value::Unknown),
                    )),
                );
                if let Some(value) = value {
                    self.expressions
                        .insert(attribute.attr.to_string(), value.clone());
                }
            }
        }
    }
}
impl<'a> Visitor<'a> for Attributes {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Assign(assignment) => {
                for target in &assignment.targets {
                    self.assign(target, Some(&assignment.value));
                }
            }
            Stmt::AnnAssign(assignment) => {
                self.assign(&assignment.target, assignment.value.as_deref())
            }
            Stmt::FunctionDef(function) => {
                if function
                    .parameters
                    .iter()
                    .any(|value| value.name().as_str() == self.receiver)
                {
                    return;
                }
                let previous = (self.method.clone(), self.setter);
                self.method = function.name.to_string();
                self.setter = false;
                self.visit_body(&function.body);
                (self.method, self.setter) = previous;
            }
            Stmt::ClassDef(_) => {}
            Stmt::AugAssign(_) => {}
            _ => walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if let Expr::Attribute(value) = expression {
            if value.ctx.is_store() {
                self.assign(expression, None);
            }
        }
        mamba_language::application::quality_syntax::python_ast::visitor::walk_expr(
            self, expression,
        );
    }
}

impl Class {
    pub(super) fn from_statement(
        statement: &StmtClassDef,
        current: &str,
        is_package: bool,
    ) -> Self {
        let (mut members, first_members) =
            callables::collect_with_first(&statement.body, current, is_package);
        let mut methods = BTreeMap::new();
        let mut attributes = BTreeMap::new();
        let mut attribute_definitions = Vec::new();
        let mut attribute_expressions = BTreeMap::new();
        let owner_name = statement.name.to_string();
        for function in declarations::methods(&statement.body) {
            let kind = wrappers::kind(&statement.body, function.name.as_str())
                .unwrap_or_else(|| method_kind(function));
            let mut mutations = mutations::Mutations::new(&owner_name, function, kind);
            mutations.visit_body(&function.body);
            members.extend(mutations.members);
            attributes.extend(mutations.attributes);
            attribute_definitions.extend(mutations.definitions);
            if kind == MethodKind::Instance || kind == MethodKind::Property {
                let mut collector = Attributes {
                        method: function.name.to_string(),
                        setter: function.decorator_list.iter().any(|decorator| matches!(&decorator.expression, Expr::Attribute(attribute) if attribute.attr.as_str() == "setter")),
                        receiver: function
                            .parameters
                            .iter()
                            .next()
                            .map(|parameter| parameter.name().to_string())
                            .unwrap_or_default(),
                        ..Attributes::default()
                    };
                collector.visit_body(&function.body);
                attributes.extend(collector.values);
                attribute_definitions.extend(collector.definitions);
                attribute_expressions.extend(collector.expressions);
            }
            let value = function
                .body
                .iter()
                .find_map(|statement| {
                    if let Stmt::Return(statement) = statement {
                        Some(
                            statement
                                .value
                                .as_deref()
                                .map(Value::from_expression)
                                .unwrap_or(Value::Scalar("NoneType")),
                        )
                    } else {
                        None
                    }
                })
                .unwrap_or(Value::Unknown);
            let mut signature = Signature::from_function(function);
            signature.owner = Some((current.into(), owner_name.clone()));
            methods.insert(
                function.name.to_string(),
                Method {
                    signature: Rc::new(signature),
                    kind,
                    value,
                },
            );
        }
        let design_methods = crate::infrastructure::project::symbols::Symbols::collect(
            &statement.body,
            current,
            is_package,
        )
        .bindings
        .into_iter()
        .filter_map(|(name, value)| {
            (name != "__module__"
                && matches!(
                    value.kind,
                    crate::infrastructure::project::symbols::DefinitionKind::Function
                ))
            .then_some(name)
        })
        .collect();
        let slots = slots::collect(&statement.body);
        Self {
            name: statement.name.to_string(),
            identity: (current.into(), statement.name.start().to_u32()),
            bases: statement
                .arguments
                .as_ref()
                .map(|arguments| arguments.args.to_vec())
                .unwrap_or_default(),
            decorators: statement
                .decorator_list
                .iter()
                .map(|value| value.expression.clone())
                .collect(),
            metaclass: statement
                .arguments
                .as_ref()
                .and_then(|arguments| {
                    arguments.keywords.iter().find(|value| {
                        value
                            .arg
                            .as_ref()
                            .is_some_and(|name| name.as_str() == "metaclass")
                    })
                })
                .map(|value| value.value.clone()),
            members,
            first_members,
            methods,
            design_methods,
            attributes,
            expressions: super::source_bindings::collect(&statement.body),
            attribute_expressions,
            attribute_definitions,
            slot_values: slots.slots,
            slot_check_values: slots.checks,
            annotation_only: slots.annotations,
            declared_members: slots.declared,
        }
    }
}
