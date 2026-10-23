//! Instance field declarations retain receiver identity and unpacked item types.
use super::*;
use mamba_language::application::quality_syntax::python_ast::visitor::walk_stmt;
use ruff_text_size::TextSize;

pub(super) struct Attributes<'a> {
    pub(super) receiver: &'a str,
    pub(super) context: &'a Context,
    pub(super) variables: &'a types::Variables,
    pub(super) environment: BTreeMap<String, Type>,
    pub(super) fields: &'a mut BTreeMap<String, Field>,
    pub(super) annotated: &'a mut std::collections::BTreeSet<String>,
    pub(super) first_declarations: &'a mut BTreeMap<String, TextSize>,
    pub(super) call_resolver: Option<&'a CallResolver<'a>>,
}

impl Attributes<'_> {
    fn infer(&self, expression: &Expr) -> Option<Type> {
        if let Some(resolve) = self.call_resolver {
            return resolve(expression, &self.environment);
        }
        self.context
            .infer_with_calls(expression, &self.environment, &|_| None)
    }

    fn assign(&mut self, target: &Expr, value: Option<&Expr>, annotation: Option<&Expr>) {
        if annotation.is_none() && matches!(target, Expr::Tuple(_) | Expr::List(_)) {
            self.assign_sequence(target, value);
            return;
        }
        let Expr::Attribute(attribute) = target else {
            return;
        };
        if !matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == self.receiver)
        {
            return;
        }
        let ty = if let Some(annotation) = annotation {
            self.context
                .annotation_with_variables(annotation, self.variables)
                .or_else(|| {
                    value.and_then(|value| {
                        self.context
                            .infer_with_calls(value, &self.environment, &|_| None)
                    })
                })
        } else {
            value.and_then(|value| self.infer(value))
        };
        self.record(attribute, ty, annotation);
    }

    fn record(
        &mut self,
        attribute: &mamba_language::application::quality_syntax::python_ast::ExprAttribute,
        ty: Option<Type>,
        annotation: Option<&Expr>,
    ) {
        let final_attribute = annotation.is_some_and(|annotation| {
            qualifiers::is_final(self.context, annotation, self.variables)
        });
        let needs_annotation = annotation.is_none() && ty.is_some_and(|ty| self.context.node(ty).is_some_and(|node| matches!(node.as_ref(), types::Node::Variable(variable) if !variable.values.is_empty())));
        if annotation.is_some() {
            self.annotated.insert(attribute.attr.to_string());
        }
        // Only a later inferred value may fill a declaration owned by this collector.
        if annotation.is_none()
            && ty.is_some()
            && !self.annotated.contains(attribute.attr.as_str())
            && self
                .first_declarations
                .get(attribute.attr.as_str())
                .is_some_and(|&first| first < attribute.start())
        {
            if let Some(field) = self.fields.get_mut(attribute.attr.as_str()) {
                if field.ty.is_none()
                    && !field.needs_annotation
                    && !field.class_access
                    && !field.readonly
                    && !field.final_attribute
                    && !field.class_var
                    && !field.abstract_field
                {
                    field.ty = if needs_annotation {
                        Some(Type::Any)
                    } else {
                        ty
                    };
                    field.needs_annotation = needs_annotation;
                }
            }
        }
        use std::collections::btree_map::Entry;
        if let Entry::Vacant(entry) = self.fields.entry(attribute.attr.to_string()) {
            self.first_declarations
                .insert(attribute.attr.to_string(), attribute.start());
            entry.insert(Field {
                ty: if needs_annotation {
                    Some(Type::Any)
                } else {
                    ty
                },
                needs_annotation,
                class_access: false,
                readonly: final_attribute,
                final_attribute,
                class_var: false,
                abstract_field: false,
            });
        }
    }

    fn assign_sequence(&mut self, target: &Expr, value: Option<&Expr>) {
        if !self.has_receiver_field(target) {
            return;
        }
        let Some(ty) = value.and_then(|value| self.infer(value)) else {
            self.declare_unknown_sequence(target);
            return;
        };
        let Some(bindings) = self.sequence_bindings(target, ty) else {
            self.declare_unknown_sequence(target);
            return;
        };
        for (target, ty) in bindings {
            let Expr::Attribute(attribute) = target else {
                continue;
            };
            if matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == self.receiver)
            {
                self.record(attribute, Some(ty), None);
            }
        }
    }

    fn declare_unknown_sequence(&mut self, target: &Expr) {
        // Check the complete target shape before recording any declaration.
        let Some(targets) = Self::sequence_targets(target) else {
            return;
        };
        for target in targets {
            let Expr::Attribute(attribute) = target else {
                continue;
            };
            if matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == self.receiver)
            {
                self.record(attribute, None, None);
            }
        }
    }

    fn sequence_targets(target: &Expr) -> Option<Vec<&Expr>> {
        let targets: &[Expr] = match target {
            Expr::Tuple(tuple) => tuple.elts.as_ref(),
            Expr::List(list) => list.elts.as_ref(),
            Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) => {
                return Some(vec![target]);
            }
            // Keep starred and other unsupported targets out of the whole plan.
            _ => return None,
        };
        let mut leaves = Vec::new();
        for target in targets {
            leaves.extend(Self::sequence_targets(target)?);
        }
        Some(leaves)
    }

    fn has_receiver_field(&self, target: &Expr) -> bool {
        match target {
            Expr::Attribute(attribute) => {
                matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == self.receiver)
            }
            Expr::Tuple(tuple) => tuple.elts.iter().any(|item| self.has_receiver_field(item)),
            Expr::List(list) => list.elts.iter().any(|item| self.has_receiver_field(item)),
            _ => false,
        }
    }

    fn sequence_bindings<'a>(&self, target: &'a Expr, ty: Type) -> Option<Vec<(&'a Expr, Type)>> {
        let targets: &[Expr] = match target {
            Expr::Tuple(tuple) => tuple.elts.as_ref(),
            Expr::List(list) => list.elts.as_ref(),
            Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) => {
                return Some(vec![(target, ty)]);
            }
            // A starred target needs its own length and list-item binding rules.
            _ => return None,
        };
        let items = match self.context.node(ty).as_deref() {
            Some(types::Node::Tuple(items)) => {
                if items.len() != targets.len() || self.context.tuple_pack_position(items).is_some()
                {
                    return None;
                }
                items.clone()
            }
            _ if ty.is_any() => vec![Type::Any; targets.len()],
            Some(types::Node::VariadicTuple(item)) => vec![*item; targets.len()],
            _ => vec![self.context.iterable_item(ty)?; targets.len()],
        };
        let mut bindings = Vec::new();
        for (target, ty) in targets.iter().zip(items) {
            bindings.extend(self.sequence_bindings(target, ty)?);
        }
        Some(bindings)
    }
}

impl<'a> Visitor<'a> for Attributes<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Assign(assignment) => {
                for target in &assignment.targets {
                    self.assign(target, Some(&assignment.value), None);
                }
            }
            Stmt::AnnAssign(assignment) => self.assign(
                &assignment.target,
                assignment.value.as_deref(),
                Some(&assignment.annotation),
            ),
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => walk_stmt(self, statement),
        }
    }
}
