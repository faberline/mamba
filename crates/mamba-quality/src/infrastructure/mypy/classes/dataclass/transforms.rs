//! Decorator transform declarations retain defaults and field specifier identities.
use super::super::super::types::Node;
use super::*;
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct Spec {
    pub transformed: bool,
    pub eq_default: bool,
    pub order_default: bool,
    pub keyword_only_default: bool,
    pub frozen_default: bool,
    pub field_specifiers: Vec<String>,
}

impl Default for Spec {
    fn default() -> Self {
        Self {
            transformed: false,
            eq_default: true,
            order_default: false,
            keyword_only_default: false,
            frozen_default: false,
            field_specifiers: vec!["field".into()],
        }
    }
}

#[derive(Default)]
pub(in crate::infrastructure::mypy) struct Registry {
    keys: BTreeMap<(String, u32), usize>,
    pub(super) specs: Vec<Spec>,
    field_init: BTreeMap<String, bool>,
    pub(super) class_specs: BTreeMap<u32, usize>,
    pub(super) metaclasses: BTreeMap<u32, u32>,
    pub(super) direct_metaclasses: BTreeMap<u32, u32>,
}

#[derive(PartialEq, Eq)]
pub(in crate::infrastructure::mypy) struct LoopState {
    declarations: BTreeMap<(String, u32), (usize, SpecState)>,
    field_init: BTreeMap<String, bool>,
    class_specs: BTreeMap<u32, (usize, SpecState)>,
    metaclasses: BTreeMap<u32, u32>,
    direct_metaclasses: BTreeMap<u32, u32>,
}

#[derive(PartialEq, Eq)]
struct SpecState {
    transformed: bool,
    eq_default: bool,
    order_default: bool,
    keyword_only_default: bool,
    frozen_default: bool,
    field_specifiers: Vec<String>,
}

impl From<&Spec> for SpecState {
    fn from(spec: &Spec) -> Self {
        Self {
            transformed: spec.transformed,
            eq_default: spec.eq_default,
            order_default: spec.order_default,
            keyword_only_default: spec.keyword_only_default,
            frozen_default: spec.frozen_default,
            field_specifiers: spec.field_specifiers.clone(),
        }
    }
}

impl Registry {
    pub(in crate::infrastructure::mypy) fn loop_state(
        &self,
        namespace: &str,
        classes: std::collections::BTreeSet<u32>,
    ) -> LoopState {
        let field_prefix = format!("dataclass-field:{namespace}:");
        LoopState {
            declarations: self
                .keys
                .range((namespace.to_owned(), 0)..=(namespace.to_owned(), u32::MAX))
                .map(|(key, &index)| (key.clone(), (index, SpecState::from(&self.specs[index]))))
                .collect(),
            field_init: self
                .field_init
                .iter()
                .filter(|(key, _)| key.starts_with(&field_prefix))
                .map(|(key, &value)| (key.clone(), value))
                .collect(),
            class_specs: self
                .class_specs
                .iter()
                .filter(|(id, _)| classes.contains(id))
                .map(|(&id, &index)| (id, (index, SpecState::from(&self.specs[index]))))
                .collect(),
            metaclasses: self
                .metaclasses
                .iter()
                .filter(|(id, _)| classes.contains(id))
                .map(|(&id, &value)| (id, value))
                .collect(),
            direct_metaclasses: self
                .direct_metaclasses
                .iter()
                .filter(|(id, _)| classes.contains(id))
                .map(|(&id, &value)| (id, value))
                .collect(),
        }
    }
}

fn reference(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Name(name) => Some(name.id.to_string()),
        Expr::Attribute(value) => Some(format!("{}.{}", reference(&value.value)?, value.attr)),
        _ => None,
    }
}

impl Context {
    pub(in crate::infrastructure::mypy) fn collect_dataclass_transforms(
        &self,
        body: &[Stmt],
        namespace: &str,
        variables: &mut Variables,
        enclosing: &std::collections::BTreeSet<Type>,
    ) {
        for statement in body {
            let (name, decorators, class) = match statement {
                Stmt::FunctionDef(function) => (&function.name, &function.decorator_list, None),
                Stmt::ClassDef(class) => (
                    &class.name,
                    &class.decorator_list,
                    Some(self.declare_class(
                        namespace,
                        class.name.as_str(),
                        class.name.start().to_u32(),
                    )),
                ),
                _ => continue,
            };
            let Some(call) = decorators.iter().find_map(|decorator| {
                let Expr::Call(call) = &decorator.expression else {
                    return None;
                };
                (self.typing_form(&call.func, variables).as_deref() == Some("dataclass_transform"))
                    .then_some(call)
            }) else {
                continue;
            };
            let mut fields = Vec::new();
            if let Some(Expr::Tuple(tuple)) = call
                .arguments
                .keywords
                .iter()
                .find(|keyword| {
                    keyword
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "field_specifiers")
                })
                .map(|keyword| &keyword.value)
            {
                for expression in &tuple.elts {
                    if let Some(form) = self.typing_form(expression, variables) {
                        fields.push(form);
                    } else if let Some(name) = reference(expression) {
                        let form = format!("dataclass-field:{namespace}:{name}");
                        variables.insert(name, self.intern(Node::TypingForm(form.clone())));
                        fields.push(form);
                    } else {
                        fields.clear();
                        break;
                    }
                }
            }
            let spec = Spec {
                transformed: true,
                eq_default: flag(Some(&call.arguments), "eq_default", true),
                order_default: flag(Some(&call.arguments), "order_default", false),
                keyword_only_default: flag(Some(&call.arguments), "kw_only_default", false),
                frozen_default: flag(Some(&call.arguments), "frozen_default", false),
                field_specifiers: fields,
            };
            let key = (namespace.to_owned(), name.start().to_u32());
            let index = {
                let mut classes = self.classes.borrow_mut();
                let registry = &mut classes.dataclass_transforms;
                if let Some(&index) = registry.keys.get(&key) {
                    registry.specs[index] = spec;
                    index
                } else {
                    let index = registry.specs.len();
                    registry.keys.insert(key, index);
                    registry.specs.push(spec);
                    index
                }
            };
            if let Some(class) = class {
                self.classes
                    .borrow_mut()
                    .dataclass_transforms
                    .class_specs
                    .insert(class, index);
            } else {
                variables.insert(
                    name.to_string(),
                    self.intern(Node::TypingForm(format!("dataclass-transform:{index}"))),
                );
            }
        }
        let mut literals = variables.clone();
        self.literal_forms(body, &mut literals);
        for statement in body {
            let Stmt::FunctionDef(function) = statement else {
                continue;
            };
            let Some(form) = variables
                .get(function.name.as_str())
                .and_then(|&ty| self.node(ty))
                .and_then(|node| match node.as_ref() {
                    Node::TypingForm(form) if form.starts_with("dataclass-field:") => {
                        Some(form.clone())
                    }
                    _ => None,
                })
            else {
                continue;
            };
            let binders = self.returned_callable_input(
                function,
                namespace,
                enclosing.clone(),
                false,
                "dataclass",
                &|_| false,
            );
            let signature = Signature::from_function(function, self, &literals, &binders);
            let value = signature
                .parameters
                .iter()
                .find(|parameter| parameter.name == "init")
                .and_then(|parameter| parameter.ty)
                .and_then(|ty| self.node(ty))
                .and_then(|node| match node.as_ref() {
                    Node::Literal(super::super::super::types::Literal::Bool(value)) => Some(*value),
                    _ => None,
                });
            if let Some(value) = value {
                self.classes
                    .borrow_mut()
                    .dataclass_transforms
                    .field_init
                    .insert(form, value);
            }
        }
    }

    pub(super) fn dataclass_field_init(
        &self,
        field: Option<&ExprCall>,
        variables: &Variables,
    ) -> bool {
        let Some(field) = field else { return true };
        if let Some(keyword) = field.arguments.keywords.iter().find(|keyword| {
            keyword
                .arg
                .as_ref()
                .is_some_and(|name| name.as_str() == "init")
        }) {
            return matches!(&keyword.value, Expr::BooleanLiteral(value) if value.value);
        }
        self.typing_form(&field.func, variables)
            .and_then(|form| {
                self.classes
                    .borrow()
                    .dataclass_transforms
                    .field_init
                    .get(&form)
                    .copied()
            })
            .unwrap_or(true)
    }

    pub(super) fn dataclass_transform_spec(
        &self,
        expression: &Expr,
        variables: &Variables,
    ) -> Option<Spec> {
        let form = self.typing_form(expression, variables)?;
        if form == "dataclass" {
            return Some(Spec::default());
        }
        let index = form
            .strip_prefix("dataclass-transform:")?
            .parse::<usize>()
            .ok()?;
        self.classes
            .borrow()
            .dataclass_transforms
            .specs
            .get(index)
            .cloned()
    }
}
