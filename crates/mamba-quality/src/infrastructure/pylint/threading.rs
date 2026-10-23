//! Thread contexts preserve the original extender and uncertain RLock result.
use super::{callables::Target, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtFunctionDef, StmtWith};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

const LOCKS: &[&str] = &[
    "threading.Lock",
    "threading.RLock",
    "threading.Condition",
    "threading.Semaphore",
    "threading.BoundedSemaphore",
];
impl Checker<'_> {
    pub(super) fn check_threading_function(&mut self, function: &StmtFunctionDef) {
        if self.module_name == "threading"
            && self.scopes.current_index() == 0
            && function.name.as_str() == "Lock"
        {
            self.emit(
                "E0102",
                "function-redefined",
                "function already defined line 14",
                self.declaration_range(function.name.range()),
            );
        }
    }
    fn threading_name(&self, expression: &Expr, origin: Context) -> Option<String> {
        self.stdlib_name_in(expression, origin.clone(), &mut BTreeSet::new())
            .or_else(|| {
                // The original threading extender replaces project Lock definitions too.
                if let Expr::Attribute(value) = expression {
                    if value.attr.as_str() == "Lock"
                        && self.expression_target_in(&value.value, origin.clone()).is_some_and(
                            |resolved| matches!(resolved.target, Target::Module(module) if module == "threading"),
                        )
                    {
                        return Some("threading.Lock".into());
                    }
                }
                if let Expr::Name(value) = expression {
                    let target = match &origin {
                        Context::Scope(index) => self.scopes.call_target_in(value.id.as_str(), *index).map(|(_, target)| target),
                        Context::Module(module) => self.modules.call_target(self.source.project, module, value.id.as_str()),
                    };
                    if matches!(target, Some(Target::Imported(import)) if import.module == "threading" && import.member.as_deref() == Some("Lock")) {
                        return Some("threading.Lock".into());
                    }
                }
                None
            })
    }
    pub(super) fn check_threading_with(&mut self, statement: &StmtWith) {
        if statement.is_async {
            return;
        }
        for item in &statement.items {
            let Expr::Call(call) = &item.context_expr else {
                continue;
            };
            let origin = Context::Scope(self.scopes.current_index());
            if let Some(name) = self.threading_name(&call.func, origin) {
                if LOCKS.contains(&name.as_str()) {
                    self.emit(
                        "W2101",
                        "useless-with-lock",
                        &format!("'{name}()' directly created in 'with' has no effect"),
                        statement.range(),
                    );
                }
            }
        }
    }
    pub(super) fn check_threading_context(
        &mut self,
        statement: &StmtWith,
        expression: &Expr,
    ) -> bool {
        let origin = Context::Scope(self.scopes.current_index());
        let (name, called) = match expression {
            Expr::Call(call) => {
                if let Expr::Attribute(attribute) = call.func.as_ref() {
                    if attribute.attr.as_str() == "__enter__" {
                        if let Expr::Call(lock) = attribute.value.as_ref() {
                            if self.threading_name(&lock.func, origin.clone()).as_deref()
                                == Some("threading.Lock")
                            {
                                self.emit_threading_context(statement, "bool");
                                return true;
                            }
                        }
                    }
                }
                (self.threading_name(&call.func, origin), true)
            }
            value => (self.threading_name(value, origin), false),
        };
        let Some(name) = name else { return false };
        let Some(member) = name.strip_prefix("threading.") else {
            return false;
        };
        if !LOCKS.contains(&name.as_str()) && !matches!(member, "Event" | "Barrier" | "Thread") {
            return false;
        }
        if !called || !LOCKS.contains(&name.as_str()) {
            self.emit_threading_context(statement, member);
        } else if statement.is_async && member != "RLock" {
            self.emit_threading_context(statement, if member == "Lock" { "lock" } else { member });
        }
        true
    }
    fn emit_threading_context(&mut self, statement: &StmtWith, name: &str) {
        if statement.is_async {
            self.emit(
                "E1701",
                "not-async-context-manager",
                &format!(
                    "Async context manager '{name}' doesn't implement __aenter__ and __aexit__."
                ),
                statement.range(),
            );
        } else {
            self.emit(
                "E1129",
                "not-context-manager",
                &format!("Context manager '{name}' doesn't implement __enter__ and __exit__."),
                statement.range(),
            );
        }
    }
}
