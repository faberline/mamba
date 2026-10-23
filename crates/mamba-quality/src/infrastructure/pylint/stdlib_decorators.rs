//! Cache and dispatch decorators retain class ownership and raw-name behavior.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtFunctionDef};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;
impl Checker<'_> {
    pub(super) fn check_stdlib_decorators(&mut self, function: &StmtFunctionDef) {
        let method = self.scopes.in_class();
        let non_instance = function.decorator_list.iter().any(|value| {
            matches!(
                self.stdlib_name(&value.expression).as_deref(),
                Some("builtins.staticmethod" | "builtins.classmethod")
            )
        });
        let enumeration = self.class_stack.last().is_some_and(|(class, origin)| {
            self.lineage(class.clone(), origin.clone())
                .iter()
                .any(|(class, origin)| {
                    class.bases.iter().any(|base| {
                        self.stdlib_name_in(base, origin.clone(), &mut BTreeSet::new())
                            .as_deref()
                            == Some("enum.Enum")
                    })
                })
        });
        for decorator in &function.decorator_list {
            let expression = &decorator.expression;
            if method && !non_instance && !enumeration {
                let cache = match expression {
                    Expr::Call(call)
                        if self.stdlib_name(&call.func).as_deref()
                            == Some("functools.lru_cache") =>
                    {
                        self.stdlib_argument(call, 0, "maxsize")
                            .is_some_and(|(value, origin)| {
                                matches!(
                                    self.stdlib_constant(&value, origin),
                                    Some(Expr::NoneLiteral(_))
                                )
                            })
                    }
                    _ => self.stdlib_name(expression).as_deref() == Some("functools.cache"),
                };
                if cache {
                    self.emit("W1518","method-cache-max-size-none","'lru_cache(maxsize=None)' or 'cache' will keep all method args alive indefinitely, including 'self'",expression.range());
                }
            }
            if let Expr::Name(name) = expression {
                if method && name.id.as_str() == "singledispatch" {
                    self.emit("E1519","singledispatch-method","singledispatch decorator should not be used with methods, use singledispatchmethod instead.",expression.range());
                }
                if !method && name.id.as_str() == "singledispatchmethod" {
                    self.emit("E1520","singledispatchmethod-function","singledispatchmethod decorator should not be used with functions, use singledispatch instead.",expression.range());
                }
            }
            let registration = match expression {
                Expr::Call(call) => call.func.as_ref(),
                value => value,
            };
            if let Expr::Attribute(value) = registration {
                if value.attr.as_str() == "register" {
                    if let Some(super::callables::Target::Function(signature)) =
                        self.callable_target(&value.value)
                    {
                        let decorators = signature
                            .decorators
                            .iter()
                            .filter_map(|value| self.stdlib_name(value))
                            .collect::<Vec<_>>();
                        if method
                            && decorators
                                .iter()
                                .any(|name| name == "functools.singledispatch")
                        {
                            self.emit("E1519","singledispatch-method","singledispatch decorator should not be used with methods, use singledispatchmethod instead.",expression.range());
                        }
                        if !method
                            && decorators
                                .iter()
                                .any(|name| name == "functools.singledispatchmethod")
                        {
                            self.emit("E1520","singledispatchmethod-function","singledispatchmethod decorator should not be used with functions, use singledispatch instead.",expression.range());
                        }
                    }
                }
            }
        }
    }
}
