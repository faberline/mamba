//! FString fields retain checked child types and the native parser's joined context.
use super::*;
use quality_syntax::python_ast::{
    ConversionFlag, FStringPartRef, InterpolatedStringElement, InterpolatedStringElements,
};
use ruff_text_size::{TextRange, TextSize};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};
use types::{Node, Type};
mod literal_format;

#[derive(Default)]
pub(super) struct Observations(Rc<RefCell<Option<types::ObservedExpressionTypes>>>);

struct ObservationGuard {
    slot: Rc<RefCell<Option<types::ObservedExpressionTypes>>>,
    previous: Option<types::ObservedExpressionTypes>,
}

impl Drop for ObservationGuard {
    fn drop(&mut self) {
        self.slot.replace(self.previous.take());
    }
}

impl Observations {
    fn scope(&self, values: Option<types::ObservedExpressionTypes>) -> ObservationGuard {
        ObservationGuard {
            slot: self.0.clone(),
            previous: self.0.replace(values),
        }
    }

    fn current(&self) -> Option<types::ObservedExpressionTypes> {
        self.0.borrow().clone()
    }
}

impl Checker<'_> {
    pub(super) fn observed_fstring_type(&self, expression: &Expr) -> Option<Option<Type>> {
        self.fstring_observed.current().and_then(|values| {
            let observed = values
                .lock()
                .expect("observed expression types")
                .get(&(expression as *const Expr as usize))
                .copied();
            observed
        })
    }

    pub(super) fn fstring_observed_environment(&self) -> Option<BTreeMap<String, Type>> {
        self.fstring_observed.current().map(|_| self.environment())
    }

    pub(super) fn record_fstring_type(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
    ) {
        let Some(values) = self.fstring_observed.current() else {
            return;
        };
        // The visitor has just checked this node. Retain its input environment
        // for callable lookup, and compose its result from saved checked children.
        let actual = if let Expr::Named(named) = expression {
            self.observed_fstring_type(&named.value).flatten()
        } else {
            self.context
                .infer_with_observed(
                    expression,
                    environment,
                    &|call| self.scopes.expression_type(call, environment),
                    &values,
                )
                .or_else(|| self.untyped_pattern_value(expression))
        };
        values
            .lock()
            .expect("observed expression types")
            .insert(expression as *const Expr as usize, actual);
    }

    fn visit_fstring_value<'a>(&mut self, value: &'a Expr) -> Option<Type>
    where
        Self: Visitor<'a>,
    {
        let _guard = self.fstring_observed.scope(Some(Default::default()));
        self.visit_expr(value);
        self.observed_fstring_type(value).flatten()
    }

    pub(super) fn visit_fstring_bytes_coercion<'a>(&mut self, expression: &'a Expr) -> bool
    where
        Self: Visitor<'a>,
    {
        let Expr::FString(string) = expression else {
            return false;
        };
        // A nested FString's fields have independent checked-value lifetimes.
        let _guard = self.fstring_observed.scope(None);
        let mut items = JoinedItems::default();
        for part in string.value.iter() {
            match part {
                FStringPartRef::Literal(_) => items.literal(),
                FStringPartRef::FString(part) => items.elements(&part.elements),
            }
        }
        let joined = (items.count == 1).then_some(expression.range());
        for part in string.value.iter() {
            match part {
                FStringPartRef::Literal(literal) => self.visit_string_literal(literal),
                FStringPartRef::FString(part) => {
                    self.visit_fstring_elements(&part.elements, joined);
                }
            }
        }
        true
    }

    fn visit_fstring_elements<'a>(
        &mut self,
        elements: &'a InterpolatedStringElements,
        joined: Option<TextRange>,
    ) where
        Self: Visitor<'a>,
    {
        for element in elements {
            let InterpolatedStringElement::Interpolation(field) = element else {
                self.visit_interpolated_string_element(element);
                continue;
            };
            let implicit_repr = field.debug_text.is_some() && field.format_spec.is_none();
            let actual = if field.conversion == ConversionFlag::None && !implicit_repr {
                self.visit_fstring_value(&field.expression)
            } else {
                self.visit_expr(&field.expression);
                None
            };
            // Upstream checks the spec after storing the value type, before its warning.
            if let Some(spec) = &field.format_spec {
                let mut items = JoinedItems::default();
                items.elements(&spec.elements);
                self.visit_fstring_elements(
                    &spec.elements,
                    (items.count == 1).then_some(spec.range),
                );
            }
            if actual.is_some_and(|actual| self.fstring_bytes_warning(actual, &mut BTreeSet::new()))
            {
                // Only a sole joined item inherits the enclosing joined range.
                // Other synthetic format calls have a start and a one-column end.
                let source = joined.unwrap_or_else(|| {
                    let start = field.expression.start();
                    TextRange::new(start, start + TextSize::from(1))
                });
                self.emit(
                    "str-bytes-safe",
                    "If x = b'abc' then f\"{x}\" or \"{}\".format(x) produces \"b'abc'\", not \"abc\". If this is desired behavior, use f\"{x!r}\" or \"{!r}\".format(x). Otherwise, decode the bytes".into(),
                    &source,
                );
            }
        }
    }

    fn fstring_bytes_warning(&self, ty: Type, active: &mut BTreeSet<Type>) -> bool {
        if !active.insert(ty) {
            return false;
        }
        let warning = match self.context.node(ty).as_deref() {
            Some(Node::Union(items)) => items
                .iter()
                .any(|&item| self.fstring_bytes_warning(item, active)),
            Some(Node::AliasInstance(id, arguments)) if arguments.is_empty() => self
                .fstring_alias_target(*id)
                .is_some_and(|target| self.fstring_bytes_warning(target, active)),
            Some(Node::InferredAlias { target, .. }) => self.fstring_bytes_warning(*target, active),
            _ => {
                !self.fstring_custom_method(ty, "__format__")
                    && !self.fstring_custom_method(ty, "__str__")
                    && self.fstring_bytes_component(ty, &mut BTreeSet::new())
            }
        };
        active.remove(&ty);
        warning
    }

    fn fstring_bytes_component(&self, ty: Type, active: &mut BTreeSet<Type>) -> bool {
        if ty == Type::Bytes {
            return true;
        }
        if !active.insert(ty) {
            return false;
        }
        let contains = match self.context.node(ty).as_deref() {
            Some(Node::Union(items)) => items
                .iter()
                .any(|&item| self.fstring_bytes_component(item, active)),
            Some(Node::Variable(variable))
                if !variable.parameter_spec && !variable.type_var_tuple =>
            {
                self.fstring_bytes_component(variable.bound, active)
                    || variable
                        .values
                        .iter()
                        .any(|&value| self.fstring_bytes_component(value, active))
            }
            Some(Node::AliasInstance(id, arguments)) if arguments.is_empty() => self
                .fstring_alias_target(*id)
                .is_some_and(|target| self.fstring_bytes_component(target, active)),
            Some(Node::InferredAlias { target, .. }) => {
                self.fstring_bytes_component(*target, active)
            }
            Some(
                Node::Instance(id)
                | Node::AppliedClass {
                    id, value: false, ..
                },
            ) => self.context.class_mro(*id).ok().is_some_and(|lineage| {
                lineage
                    .into_iter()
                    .any(|id| self.context.class(id).scalar_bases.contains(&Type::Bytes))
            }),
            _ => false,
        };
        active.remove(&ty);
        contains
    }

    fn fstring_alias_target(&self, id: u32) -> Option<Type> {
        let alias = self.context.alias(id);
        // Do not expand applications here: expansion records shared alias origins.
        // A runtime Alias marker is also not a value of its annotation target type.
        if !alias.parameters.is_empty()
            || alias.invalid_variable.is_some()
            || self.context.unguarded_alias_cycle(id)
        {
            return None;
        }
        alias.target
    }

    fn fstring_custom_method(&self, ty: Type, name: &str) -> bool {
        if ty.is_any() {
            return true;
        }
        // Upstream follows a TypeVar's bounds only for bytes-component detection,
        // not for its custom-special-method guard.
        let id = match self.context.node(ty).as_deref() {
            Some(
                Node::Instance(id)
                | Node::AppliedClass {
                    id, value: false, ..
                },
            ) => *id,
            _ => return false,
        };
        let Ok(lineage) = self.context.class_mro(id) else {
            return true;
        };
        for id in lineage {
            let class = self.context.class(id);
            if class.erased_members.contains(name) {
                return true;
            }
            if class.methods.contains_key(name) || class.fields.contains_key(name) {
                return class.namespace != "<builtins>"
                    && class.namespace != "builtins"
                    && !class.namespace.starts_with("builtins.")
                    && class.namespace != "typing"
                    && !class.namespace.starts_with("typing.");
            }
        }
        false
    }
}

// The native parser collapses consecutive literal items before choosing a context.
#[derive(Default)]
struct JoinedItems {
    count: usize,
    literal: bool,
}

impl JoinedItems {
    fn literal(&mut self) {
        if !self.literal {
            self.count += 1;
        }
        self.literal = true;
    }

    fn elements(&mut self, elements: &InterpolatedStringElements) {
        for element in elements {
            match element {
                InterpolatedStringElement::Literal(_) => self.literal(),
                InterpolatedStringElement::Interpolation(field) => {
                    // A debug field contributes its printed label before its value.
                    if field.debug_text.is_some() {
                        self.literal();
                    }
                    self.count += 1;
                    self.literal = false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
