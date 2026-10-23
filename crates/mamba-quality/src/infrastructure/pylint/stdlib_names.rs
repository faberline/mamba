//! Standard-library names retain import aliases and project-module ownership.
use super::{callables::Target, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::Expr;
use ruff_text_size::Ranged;
use std::collections::BTreeSet;
impl Checker<'_> {
    pub(super) fn stdlib_name(&self, expression: &Expr) -> Option<String> {
        self.stdlib_name_in(
            expression,
            Context::Scope(self.scopes.current_index()),
            &mut BTreeSet::new(),
        )
    }
    pub(super) fn stdlib_name_in(
        &self,
        expression: &Expr,
        origin: Context,
        active: &mut BTreeSet<(Context, u32, u32)>,
    ) -> Option<String> {
        let key = (
            origin.clone(),
            expression.start().to_u32(),
            expression.end().to_u32(),
        );
        if !active.insert(key.clone()) {
            return None;
        }
        let result = match expression {
            Expr::Name(name) => {
                let raw = match &origin {
                    Context::Scope(index) => self
                        .scopes
                        .call_target_in(name.id.as_str(), *index)
                        .map(|(_, value)| value),
                    Context::Module(module) => {
                        self.modules
                            .call_target(self.source.project, module, name.id.as_str())
                    }
                };
                match raw {
                    Some(Target::Imported(import)) => {
                        import.member.filter(|name| name != "*").and_then(|name| {
                            self.standard_module(&import.module)
                                .then(|| format!("{}.{name}", import.module))
                        })
                    }
                    Some(Target::Reference(_))
                    | Some(Target::Value(_))
                    | Some(Target::Construct(_)) => self
                        .source_binding(name.id.as_str(), &origin)
                        .and_then(|(value, context)| self.stdlib_name_in(&value, context, active))
                        .or_else(|| self.stdlib_with_name(name.id.as_str(), &origin)),
                    None if self.modules.builtin(name.id.as_str())
                        && !self.scopes.declared(name.id.as_str()) =>
                    {
                        Some(format!("builtins.{}", name.id))
                    }
                    _ => self.stdlib_with_name(name.id.as_str(), &origin),
                }
            }
            Expr::Attribute(value) => self
                .expression_target_in(&value.value, origin.clone())
                .and_then(|resolved| match resolved.target {
                    Target::Module(module) if self.standard_module(&module) => {
                        Some(format!("{module}.{}", value.attr))
                    }
                    Target::Instance(class, _) if self.standard_module(&class.identity.0) => Some(
                        format!("{}.{}.{}", class.identity.0, class.name, value.attr),
                    ),
                    _ => None,
                })
                .or_else(|| {
                    self.stdlib_name_in(&value.value, origin, active)
                        .map(|name| format!("{name}.{}", value.attr))
                }),
            Expr::Call(call) => self
                .stdlib_name_in(&call.func, origin, active)
                .filter(|name| {
                    matches!(
                        name.as_str(),
                        "pathlib.Path"
                            | "pathlib.PosixPath"
                            | "pathlib.WindowsPath"
                            | "pathlib.PurePath"
                            | "pathlib.PurePosixPath"
                            | "pathlib.PureWindowsPath"
                    )
                }),
            _ => None,
        };
        active.remove(&key);
        result
    }
    fn standard_module(&self, name: &str) -> bool {
        !self.modules.native(name)
            && self.modules.exists(self.source.project, name)
            && (name == "subprocess"
                || self
                    .modules
                    .source_index(self.source.project, name)
                    .is_none())
    }
}
