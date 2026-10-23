//! Source function attributes preserve identity, assignment data, and guard policy.
use super::{
    callables::{Signature, Target},
    resolution::Context,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::ExprAttribute;
use ruff_text_size::Ranged;
use std::collections::{BTreeMap, BTreeSet};

pub(super) type Cache = Option<BTreeMap<(String, u32, String), BTreeSet<String>>>;
mod assignments;

fn scope_member(name: &str) -> bool {
    matches!(
        name,
        "__name__"
            | "__doc__"
            | "__qualname__"
            | "__defaults__"
            | "__annotations__"
            | "__dict__"
            | "__globals__"
            | "__kwdefaults__"
            | "__module__"
            | "__get__"
            | "__call__"
            | "__builtins__"
            | "__closure__"
            | "__code__"
            | "__type_params__"
    )
}
impl Checker<'_> {
    pub(super) fn source_function_member(
        &self,
        function: &Signature,
        origin: &Context,
        name: &str,
    ) -> Option<bool> {
        if function.category != "lambda" && self.source_function(function, origin).is_none() {
            return None;
        }
        let method = matches!(
            function.category,
            "method" | "unbound method" | "classmethod"
        );
        if scope_member(name)
            || (method
                && matches!(
                    name,
                    "__class__" | "__self__" | "__func__" | "im_class" | "im_self" | "im_func"
                ))
        {
            return Some(true);
        }
        let module = match origin {
            Context::Module(module) => module.as_str(),
            Context::Scope(_) => self.module_name,
        };
        let key = (
            module.to_string(),
            function.definition_range.start().to_u32(),
            function.name.clone(),
        );
        if self.native_member_guards.functions.borrow().is_none() {
            let mut index: BTreeMap<_, BTreeSet<String>> = BTreeMap::new();
            for source in self
                .index
                .modules
                .iter()
                .filter(|source| source.project == self.source.project)
            {
                let Some(document) = &source.document else {
                    continue;
                };
                for write in assignments::collect(
                    &document.parsed.syntax().body,
                    &source.name,
                    source.is_package,
                ) {
                    let identity = match write.owner {
                        assignments::Owner::Function(offset, name) => {
                            Some((source.name.clone(), offset, name))
                        }
                        assignments::Owner::Path(path) => {
                            self.function_write_identity(path, &source.name)
                        }
                        assignments::Owner::Unknown => None,
                    };
                    if let Some(identity) = identity {
                        index.entry(identity).or_default().insert(write.name);
                    }
                }
            }
            *self.native_member_guards.functions.borrow_mut() = Some(index);
        }
        Some(
            self.native_member_guards
                .functions
                .borrow()
                .as_ref()
                .and_then(|index| index.get(&key))
                .is_some_and(|names| names.contains(name)),
        )
    }

    fn function_write_identity(
        &self,
        path: Vec<String>,
        current: &str,
    ) -> Option<(String, u32, String)> {
        let mut resolved = None;
        for split in (1..path.len()).rev() {
            let module = path[..split].join(".");
            if self.modules.exists(self.source.project, &module) {
                resolved = self.reference_in(path[split..].to_vec(), Context::Module(module));
                break;
            }
        }
        let resolved =
            resolved.or_else(|| self.reference_in(path, Context::Module(current.into())))?;
        let Target::Function(function) = resolved.target else {
            return None;
        };
        if matches!(
            function.category,
            "method" | "unbound method" | "classmethod"
        ) {
            return None;
        }
        let module = match resolved.context {
            Context::Module(module) => module,
            Context::Scope(_) => self.module_name.into(),
        };
        Some((
            module,
            function.definition_range.start().to_u32(),
            function.name.clone(),
        ))
    }

    pub(super) fn check_function_member(
        &mut self,
        attribute: &ExprAttribute,
        function: &Signature,
        origin: &Context,
    ) {
        if !function.decorators.is_empty()
            || function.pass_body
            || function.abstract_body
            || self.source_function_member(function, origin, attribute.attr.as_str()) != Some(false)
        {
            return;
        }
        self.emit(
            "E1101",
            "no-member",
            &format!(
                "{} '{}' has no '{}' member",
                if matches!(
                    function.category,
                    "method" | "unbound method" | "classmethod"
                ) {
                    "Method"
                } else {
                    "Function"
                },
                function.name,
                attribute.attr
            ),
            attribute.range(),
        );
    }
}
