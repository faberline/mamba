//! Resource assignments retain insertion order until their declaring scope exits.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    Expr, ExprCall, StmtAssign, StmtWith,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub(super) struct Resources {
    scopes: BTreeMap<usize, Vec<(String, TextRange)>>,
    pub suppressed: usize,
    pub returned: usize,
    with_values: BTreeMap<usize, BTreeSet<String>>,
    released: BTreeSet<u32>,
    with_types: BTreeMap<usize, BTreeMap<String, String>>,
}
impl Checker<'_> {
    pub(super) fn resource_constructor(&self, call: &ExprCall) -> bool {
        self.stdlib_name(&call.func).is_some_and(|name| {
            matches!(
                name.as_str(),
                "pathlib.Path.open"
                    | "pathlib.PosixPath.open"
                    | "pathlib.WindowsPath.open"
                    | "builtins.open"
                    | "io.open"
                    | "_io.open"
                    | "subprocess.Popen"
                    | "codecs.open"
                    | "urllib.request.urlopen"
                    | "tempfile.NamedTemporaryFile"
                    | "tempfile.SpooledTemporaryFile"
                    | "tempfile.TemporaryDirectory"
                    | "tempfile.TemporaryFile"
                    | "zipfile.ZipFile"
                    | "zipfile.PyZipFile"
                    | "tarfile.TarFile"
                    | "tarfile.TarFile.open"
            )
        })
    }
    fn resource_warning(&mut self, range: TextRange) {
        self.emit(
            "R1732",
            "consider-using-with",
            "Consider using 'with' for resource-allocating operations",
            range,
        );
    }
    pub(super) fn track_resource_assignment(&mut self, statement: &StmtAssign) {
        if self.resources.suppressed > 0 {
            return;
        }
        let Some(target) = statement.targets.first() else {
            return;
        };
        self.track_resource_target(target, &statement.value);
    }
    fn track_resource_target(&mut self, target: &Expr, value: &Expr) {
        if let (Expr::Tuple(target), Expr::Tuple(value)) = (target, value) {
            for (target, value) in target.elts.iter().zip(&value.elts) {
                self.track_resource_target(target, value);
            }
            return;
        }
        let Expr::Call(call) = value else {
            return;
        };
        if !self.resource_constructor(call) {
            return;
        }
        let name = match target {
            Expr::Name(value) => value.id.to_string(),
            Expr::Attribute(value) => value.attr.to_string(),
            _ => return,
        };
        let stack = self
            .resources
            .scopes
            .entry(self.scopes.current_index())
            .or_default();
        let old = stack
            .iter()
            .position(|(value, _)| *value == name)
            .map(|index| stack.remove(index).1);
        stack.push((name, call.range()));
        if let Some(range) = old {
            self.resource_warning(range);
        }
    }
    pub(super) fn use_resource_with(&mut self, statement: &StmtWith) {
        for item in &statement.items {
            if let Some(Expr::Name(target)) = item.optional_vars.as_deref() {
                if let Expr::Call(call) = &item.context_expr {
                    if let Some(name) = self.stdlib_name(&call.func) {
                        if name == "contextlib.ExitStack" {
                            self.resources
                                .with_types
                                .entry(self.scopes.current_index())
                                .or_default()
                                .insert(target.id.to_string(), name);
                        }
                    }
                }
                let known = match &item.context_expr {
                    Expr::Call(call) => self.resource_constructor(call),
                    Expr::Name(value) => self
                        .resources
                        .scopes
                        .values()
                        .any(|stack| stack.iter().any(|(name, _)| name == value.id.as_str())),
                    _ => false,
                };
                if known {
                    self.resources
                        .with_values
                        .entry(self.scopes.current_index())
                        .or_default()
                        .insert(target.id.to_string());
                }
            }
            if let Expr::Name(value) = &item.context_expr {
                for stack in self.resources.scopes.values_mut() {
                    if let Some(index) =
                        stack.iter().position(|(name, _)| name == value.id.as_str())
                    {
                        stack.remove(index);
                        break;
                    }
                }
            }
        }
    }
    pub(super) fn check_resource_call(&mut self, call: &ExprCall) {
        if self.stdlib_name(&call.func).is_some_and(|name| {
            matches!(
                name.as_str(),
                "contextlib.ExitStack.enter_context" | "contextlib._BaseExitStack.enter_context"
            )
        }) {
            for value in call
                .arguments
                .args
                .iter()
                .chain(call.arguments.keywords.iter().map(|value| &value.value))
            {
                if let Expr::Call(value) = value {
                    self.resources.released.insert(value.start().to_u32());
                }
            }
        }
        if self.resources.suppressed > 0
            || self.resources.returned > 0
            || self.resources.released.contains(&call.start().to_u32())
        {
            return;
        }
        if self
            .resources
            .scopes
            .get(&self.scopes.current_index())
            .is_some_and(|stack| stack.iter().any(|(_, range)| *range == call.range()))
        {
            return;
        }
        if self.resource_constructor(call) {
            self.resource_warning(call.range());
        }
    }
    pub(super) fn finish_resource_scope(&mut self) {
        self.resources
            .with_values
            .remove(&self.scopes.current_index());
        self.resources
            .with_types
            .remove(&self.scopes.current_index());
        for (_, range) in self
            .resources
            .scopes
            .remove(&self.scopes.current_index())
            .unwrap_or_default()
        {
            self.resource_warning(range);
        }
    }
    pub(super) fn stdlib_with_name(
        &self,
        name: &str,
        origin: &super::resolution::Context,
    ) -> Option<String> {
        let super::resolution::Context::Scope(index) = origin else {
            return None;
        };
        let (owner, _) = self.scopes.expression_in(name, *index)?;
        self.resources.with_types.get(&owner)?.get(name).cloned()
    }
}
