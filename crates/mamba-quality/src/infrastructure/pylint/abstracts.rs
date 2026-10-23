//! Abstract construction and inherited method checks retain distinct policies.
use super::{
    callables::{Signature, Target},
    classes::{Class, MethodKind},
    resolution::{Context, Resolved},
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    Expr, ExprCall, StmtClassDef, StmtFunctionDef,
};
use ruff_text_size::Ranged;
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

type AbstractMethods = BTreeMap<String, (Rc<Signature>, Rc<Class>)>;

impl Checker<'_> {
    pub(super) fn qualified_decorator(
        &self,
        expression: &Expr,
        context: Context,
    ) -> Option<String> {
        let expression = if let Expr::Call(value) = expression {
            value.func.as_ref()
        } else {
            expression
        };
        let value = self.expression_target_in(expression, context)?;
        match value.target {
            Target::Class(class) => Some(format!("{}.{}", class.identity.0, class.name)),
            Target::Function(function) => {
                let module = if let Context::Module(module) = value.context {
                    module
                } else {
                    self.module_name.to_string()
                };
                Some(format!("{module}.{}", function.name))
            }
            _ => None,
        }
    }

    pub(super) fn inferred_method_kind<'a>(
        &self,
        decorators: impl IntoIterator<Item = &'a Expr>,
        context: Context,
        fallback: MethodKind,
    ) -> MethodKind {
        if fallback != MethodKind::Instance {
            return fallback;
        }
        for expression in decorators {
            match self
                .qualified_decorator(expression, context.clone())
                .as_deref()
            {
                Some("abc.abstractclassmethod" | "builtins.classmethod") => {
                    return MethodKind::Class
                }
                Some("abc.abstractstaticmethod" | "builtins.staticmethod") => {
                    return MethodKind::Static
                }
                Some("abc.abstractproperty" | "builtins.property") => return MethodKind::Property,
                _ => {}
            }
        }
        fallback
    }

    pub(super) fn check_deprecated_abstract_decorators(&mut self, function: &StmtFunctionDef) {
        self.check_deprecated_decorators(&function.decorator_list);
    }

    pub(super) fn abstract_function(
        &self,
        function: &Signature,
        context: Context,
        body: bool,
        class: Option<&Class>,
    ) -> bool {
        if function.category == "unbound method" {
            return false;
        }
        if body && function.abstract_body {
            return true;
        }
        function.decorators.iter().any(|decorator| {
            let name = self
                .qualified_decorator(decorator, context.clone())
                .or_else(|| {
                    let class = class?;
                    let expression = if let Expr::Call(call) = decorator {
                        call.func.as_ref()
                    } else {
                        decorator
                    };
                    let path = super::values::path(expression)?;
                    if !class.first_members.contains_key(path.first()?) {
                        return None;
                    }
                    let mut names = vec![class.name.clone()];
                    names.extend(path);
                    let value = self.reference_in(names, context.clone())?;
                    match value.target {
                        Target::Class(class) => {
                            Some(format!("{}.{}", class.identity.0, class.name))
                        }
                        Target::Function(function) => {
                            let module = if let Context::Module(module) = value.context {
                                module
                            } else {
                                self.module_name.to_string()
                            };
                            Some(format!("{module}.{}", function.name))
                        }
                        _ => None,
                    }
                })
                .or_else(|| {
                    let (module, _) = function.owner.as_ref()?;
                    self.qualified_decorator(decorator, Context::Module(module.clone()))
                });
            match name.as_deref() {
                Some("abc.abstractmethod" | "abc.abstractproperty") => true,
                Some("abc.abstractclassmethod" | "abc.abstractstaticmethod") => !body,
                _ => false,
            }
        })
    }

    fn first_member(
        &self,
        class: &Class,
        context: Context,
        name: &str,
        active: &mut BTreeSet<String>,
    ) -> Option<Resolved> {
        if !active.insert(name.into()) {
            return None;
        }
        let target = class.first_members.get(name)?.clone();
        match target {
            Target::Reference(names)
                if names.len() == 1 && class.first_members.contains_key(&names[0]) =>
            {
                self.first_member(class, context, &names[0], active)
            }
            Target::Reference(names) => self.reference_in(names, context),
            Target::Imported(import) => {
                self.reference_in(vec![import.member?], Context::Module(import.module))
            }
            Target::Contextual(target, context) => Some(Resolved {
                target: *target,
                context,
            }),
            target => Some(Resolved { target, context }),
        }
    }

    pub(super) fn abstract_methods(
        &self,
        class: Rc<Class>,
        context: Context,
        body: bool,
    ) -> AbstractMethods {
        let Ok(order) = self.mro(class, context) else {
            return BTreeMap::new();
        };
        let mut result = BTreeMap::new();
        for (class, context) in order.into_iter().rev() {
            for name in class.first_members.keys() {
                let value = self.first_member(&class, context.clone(), name, &mut BTreeSet::new());
                if let Some(Resolved {
                    target: Target::Function(function),
                    context,
                }) = value
                {
                    if self.abstract_function(&function, context, body, Some(&class)) {
                        result.insert(name.clone(), (function, class.clone()));
                        continue;
                    }
                }
                result.remove(name);
            }
        }
        result
    }

    fn explicit_abc_meta(&self, class: &Class, context: Context) -> bool {
        class.metaclass.as_ref().and_then(|value| self.expression_target_in(value, context))
            .is_some_and(|value| matches!(value.target, Target::Class(class) if class.name == "ABCMeta" && matches!(class.identity.0.as_str(), "abc" | "_py_abc")))
    }

    fn abc_metaclass(&self, class: Rc<Class>, context: Context) -> bool {
        for (class, context) in self.lineage(class, context) {
            if let Some(metaclass) = &class.metaclass {
                let value = self.expression_target_in(metaclass, context.clone());
                if value.is_some_and(|value| matches!(value.target, Target::Class(_))) {
                    return self.explicit_abc_meta(&class, context);
                }
            }
            if class.identity.0 == "abc" && class.name == "ABC" {
                return true;
            }
        }
        false
    }

    pub(super) fn check_abstract_construction(&mut self, call: &ExprCall) {
        let Some(value) = self.expression_target(&call.func) else {
            return;
        };
        let Target::Class(class) = value.target else {
            return;
        };
        if self
            .modules
            .source_index(self.source.project, &class.identity.0)
            .is_none()
            && super::stdlib_facts::member(
                &format!("{}.{}", class.identity.0, class.name),
                self.modules.target(),
            )
            .is_some_and(|member| member.r#abstract)
        {
            self.emit(
                "E0110",
                "abstract-class-instantiated",
                &format!(
                    "Abstract class '{}' with abstract methods instantiated",
                    class.name
                ),
                call.range(),
            );
            return;
        }
        if self
            .class_stack
            .last()
            .is_some_and(|(own, _)| own.identity == class.identity)
        {
            return;
        }
        if self.abc_metaclass(class.clone(), value.context.clone())
            && !self
                .abstract_methods(class.clone(), value.context, false)
                .is_empty()
        {
            self.emit(
                "E0110",
                "abstract-class-instantiated",
                &format!(
                    "Abstract class '{}' with abstract methods instantiated",
                    class.name
                ),
                call.range(),
            );
        }
    }

    pub(super) fn check_inherited_abstract_methods(
        &mut self,
        statement: &StmtClassDef,
        class: Rc<Class>,
        context: Context,
    ) {
        let direct_abc = class.bases.iter().any(|base| {
            self.inferred_base(base, context.clone())
                .is_some_and(|(class, _)| class.identity.0 == "abc" && class.name == "ABC")
        });
        let own_abstract = class.first_members.keys().any(|name| self.first_member(&class, context.clone(), name, &mut BTreeSet::new()).is_some_and(|value| matches!(value.target, Target::Function(function) if self.abstract_function(&function, value.context, true, Some(&class)))));
        if direct_abc || self.explicit_abc_meta(&class, context.clone()) || own_abstract {
            return;
        }
        for (name, (_, owner)) in self.abstract_methods(class.clone(), context, true) {
            if owner.identity != class.identity && !class.first_members.contains_key(&name) {
                self.emit("W0223", "abstract-method", &format!("Method '{name}' is abstract in class '{}' but is not overridden in child class '{}'", owner.name, class.name), self.declaration_range(statement.name.range()));
            }
        }
    }
    pub(super) fn unused_abstract_method(&self, function: &StmtFunctionDef) -> bool {
        if !self.scopes.parent_is_class() {
            return false;
        }
        let body = &function.body[usize::from(super::docstring(&function.body))..];
        body.first().is_none_or(|value| {
            matches!(
                value,
                mamba_language::application::quality_syntax::python_ast::Stmt::Pass(_)
            )
        }) || self.abstract_function(
            &Signature::from_function(function),
            Context::Scope(self.scopes.current_index().saturating_sub(1)),
            true,
            None,
        )
    }
}
