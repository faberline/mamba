//! Context-manager checks retain original protocol and decorator inference.
use super::{
    callables::{Signature, Target},
    operand_inference::Operand,
    resolution::Context,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtWith};
use ruff_text_size::Ranged;

impl Checker<'_> {
    fn context_decorated(&self, function: &Signature, origin: &Context, names: &[&str]) -> bool {
        function.decorators.iter().any(|decorator| {
            self.qualified_decorator(decorator, origin.clone())
                .is_some_and(|name| names.contains(&name.as_str()))
        })
    }
    fn context_operand(&self, operand: Operand, asynchronous: bool) -> Option<String> {
        match operand {
            Operand::Unknown => None,
            Operand::Instance(class, origin) => {
                let name = class.name.clone();
                let mixin = if asynchronous {
                    super::defaults::fixed().ignored_mixin(&name, "not-async-context-manager")
                } else {
                    name.to_ascii_lowercase().ends_with("mixin")
                };
                if mixin
                    || !self.operand_known_bases(
                        class.clone(),
                        origin.clone(),
                        &mut Default::default(),
                    )
                {
                    return None;
                }
                self.context_class_missing(class, origin, asynchronous)
                    .then_some(name)
            }
            Operand::Class(class, origin) => {
                let name = class.name.clone();
                self.context_class_missing(class, origin, asynchronous)
                    .then_some(name)
            }
            value => value.name().map(str::to_string),
        }
    }
    fn context_class_missing(
        &self,
        class: std::rc::Rc<super::classes::Class>,
        origin: Context,
        asynchronous: bool,
    ) -> bool {
        let names = if asynchronous {
            ["__aenter__", "__aexit__"]
        } else {
            ["__enter__", "__exit__"]
        };
        names.iter().any(|name| {
            self.class_member(class.clone(), origin.clone(), name, true)
                .is_none()
        })
    }
    pub(super) fn check_context_manager(&mut self, statement: &StmtWith) {
        let asynchronous = statement.is_async;
        for item in &statement.items {
            if self.check_threading_context(statement, &item.context_expr) {
                continue;
            }
            let origin = Context::Scope(self.scopes.current_index());
            let inferred = self.infer_context(&item.context_expr, origin.clone());
            let result = match inferred {
                super::context_values::Inferred::Function {
                    signature,
                    origin,
                    bound,
                } => {
                    if asynchronous
                        && signature.is_async
                        && self.context_decorated(
                            &signature,
                            &origin,
                            &["contextlib.asynccontextmanager"],
                        )
                    {
                        None
                    } else {
                        Some((
                            if bound {
                                "instancemethod".into()
                            } else {
                                signature.name.clone()
                            },
                            false,
                        ))
                    }
                }
                super::context_values::Inferred::Generator {
                    asynchronous: generator_async,
                    name,
                    decorators,
                    wrappers,
                    ..
                } => {
                    let async_decorated = decorators
                        .iter()
                        .any(|name| name == "contextlib.asynccontextmanager");
                    let sync_decorated = decorators
                        .iter()
                        .any(|name| super::defaults::fixed().contextmanager(name));
                    let wrapper = !self
                        .cleanup_inferred
                        .contains(&item.context_expr.start().to_u32())
                        && wrappers
                            .iter()
                            .any(|name| super::defaults::fixed().contextmanager(name));
                    if (asynchronous && generator_async && async_decorated)
                        || (!asynchronous && (sync_decorated || wrapper))
                    {
                        None
                    } else if !asynchronous && generator_async && async_decorated {
                        Some((name, true))
                    } else {
                        Some((
                            if generator_async {
                                "async_generator"
                            } else {
                                "generator"
                            }
                            .into(),
                            false,
                        ))
                    }
                }
                super::context_values::Inferred::Operand(operand) => {
                    let direct_async = asynchronous && !matches!(item.context_expr, Expr::Call(_))
                        && self.expression_target_in(&item.context_expr, origin).is_some_and(|resolved| matches!(resolved.target, Target::Function(function) if function.is_async && self.context_decorated(&function, &resolved.context, &["contextlib.asynccontextmanager"])));
                    if direct_async {
                        None
                    } else {
                        self.context_operand(operand, asynchronous)
                            .map(|name| (name, false))
                    }
                }
            };
            let Some((name, regular_async)) = result else {
                continue;
            };
            let (rule, symbol, message) = if regular_async {
                (
                    "E1145",
                    "async-context-manager-with-regular-with",
                    format!(
                        "Context manager '{name}' is async and should be used with 'async with'."
                    ),
                )
            } else if asynchronous {
                ("E1701", "not-async-context-manager", format!("Async context manager '{name}' doesn't implement __aenter__ and __aexit__."))
            } else {
                (
                    "E1129",
                    "not-context-manager",
                    format!("Context manager '{name}' doesn't implement __enter__ and __exit__."),
                )
            };
            self.emit(rule, symbol, &message, statement.range());
        }
    }
}
