//! Build-time inference precedes Pylint checker directives and class traversal.
use super::{
    build_attributes::{Frame, Stores},
    callables::Target,
    classes::Class,
    resolution::Context,
    scopes,
    slot_model::Slot,
    values::Value,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::rc::Rc;

struct Scan<'a, 'workspace> {
    checker: &'a mut Checker<'workspace>,
    failed: bool,
    stores: Stores,
    frame: Frame,
}

impl<'ast> Visitor<'ast> for Scan<'_, '_> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if self.failed {
            return;
        }
        match statement {
            Stmt::ClassDef(class) => {
                let model = Rc::new(Class::from_statement(
                    class,
                    self.checker.module_name,
                    self.checker.is_package,
                ));
                let context = Context::Scope(self.checker.scopes.current_index());
                self.checker.class_stack.push((model, context));
                self.checker
                    .scopes
                    .enter(scopes::Kind::Class, &class.body, None);
                let previous = std::mem::replace(
                    &mut self.frame,
                    Frame {
                        module: self.checker.module_name.into(),
                        name: class.name.to_string(),
                        setter: false,
                        annotation_only: false,
                        augmented: false,
                    },
                );
                self.visit_body(&class.body);
                self.frame = previous;
                self.checker.scopes.leave();
                self.checker.class_stack.pop();
            }
            Stmt::FunctionDef(function) => {
                let method = self.checker.declaration_method_kind(function);
                self.checker.scopes.enter(
                    scopes::Kind::Function,
                    &function.body,
                    Some(&function.parameters),
                );
                self.checker.bind_receiver(function, method);
                let previous = std::mem::replace(&mut self.frame, Frame {
                    module:self.checker.module_name.into(),
                    name:function.name.to_string(),
                    setter:function.decorator_list.iter().any(|value| matches!(&value.expression, Expr::Attribute(value) if value.attr.as_str()=="setter")),
                    annotation_only:false,
                    augmented:false,
                        });
                self.visit_body(&function.body);
                self.frame = previous;
                self.checker.scopes.leave();
            }
            Stmt::AugAssign(_) => {
                let previous = std::mem::replace(&mut self.frame.augmented, true);
                walk_stmt(self, statement);
                self.frame.augmented = previous;
            }
            Stmt::AnnAssign(value) => {
                let previous =
                    std::mem::replace(&mut self.frame.annotation_only, value.value.is_none());
                walk_stmt(self, statement);
                self.frame.annotation_only = previous;
            }
            _ => walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        if self.failed {
            return;
        }
        if let Expr::Attribute(attribute) = expression {
            if attribute.ctx.is_store() {
                if let Some(resolved) = self.checker.expression_target(&attribute.value) {
                    let owner = match resolved.target {
                        Target::Instance(class, origin) => {
                            let slots = self.checker.layout_slots(class.clone(), origin);
                            if let Some(slots) = &slots {
                                self.failed =
                                    slots.iter().any(|slot| matches!(slot, Slot::Bytes(_)))
                                        && slots.iter().any(|slot| matches!(slot, Slot::Name(_)));
                            }
                            if !self.failed
                                && slots.as_ref().is_none_or(|slots| {
                                    slots.is_empty()
                                        || slots
                                            .iter()
                                            .any(|slot| slot.named(attribute.attr.as_str()))
                                })
                            {
                                Some((class, true))
                            } else {
                                None
                            }
                        }
                        Target::Class(class) => Some((class, false)),
                        _ => None,
                    };
                    if let Some((class, instance)) = owner {
                        let value = self
                            .checker
                            .assigned_attributes
                            .get(&attribute.start().to_u32())
                            .map_or(Value::Unknown, |assignment| {
                                Value::from_expression(&assignment.value)
                            });
                        if class.identity.0 == self.checker.module_name {
                            self.stores
                                .record(&class, attribute, instance, value, &self.frame);
                        } else {
                            self.checker.modules.record_class_store(
                                &class,
                                attribute,
                                instance,
                                value,
                                &self.frame,
                            );
                        }
                    }
                }
            }
        }
        if !self.failed {
            walk_expr(self, expression);
        }
    }
}

impl Checker<'_> {
    pub(super) fn slot_build_failure(&mut self, body: &[Stmt]) -> bool {
        let original = std::mem::replace(
            &mut self.scopes,
            scopes::Scopes::new(body, self.module_name, self.is_package),
        );
        let mut scan = Scan {
            frame: Frame {
                module: self.module_name.into(),
                name: self.module_name.into(),
                setter: false,
                annotation_only: false,
                augmented: false,
            },
            checker: self,
            failed: false,
            stores: Stores::default(),
        };
        scan.visit_body(body);
        let failed = scan.failed;
        self.build_attributes = scan.stores;
        self.scopes = original;
        failed
    }
}
