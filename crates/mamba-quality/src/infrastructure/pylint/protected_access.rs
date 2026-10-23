//! Protected access retains receiver, class ancestry, and fixed exclusions.
use super::{callables::Target, classes::MethodKind, defaults, values, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprAttribute, ExprCall};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn check_super_brackets(&mut self, call: &ExprCall) {
        if self.method_context.is_none() || self.class_stack.is_empty() {
            return;
        }
        if let Expr::Attribute(attribute) = call.func.as_ref() {
            if let Expr::Name(receiver) = attribute.value.as_ref() {
                if receiver.id.as_str() == "super" {
                    self.emit(
                        "W0245",
                        "super-without-brackets",
                        "Super call without brackets",
                        receiver.range(),
                    );
                }
            }
        }
    }
    fn mandatory_receiver(&self, value: &Expr) -> bool {
        matches!(value, Expr::Name(name) if self.mandatory_receiver.as_deref() == Some(name.id.as_str()))
    }

    pub(super) fn check_protected_access(&mut self, attribute: &ExprAttribute) {
        let name = attribute.attr.as_str();
        if !name.starts_with('_')
            || name == "_"
            || (name.starts_with("__") && name.ends_with("__"))
            || defaults::fixed().protected_excluded(name)
            || self.annotation_context
            || self.property_assignment
            || self.mandatory_receiver(&attribute.value)
        {
            return;
        }
        let inferred = self.expression_target(&attribute.value);
        if inferred.as_ref().is_some_and(|value| {
            let owner = match &value.target {
                Target::Class(class) => class.name.as_str(),
                Target::Module(module) => module.as_str(),
                _ => return false,
            };
            defaults::fixed().protected_excluded(&format!("{owner}.{name}"))
        }) {
            return;
        }
        if let Some((class, origin)) = self.class_stack.last() {
            match attribute.value.as_ref() {
                Expr::Call(call) => {
                    if matches!(call.func.as_ref(), Expr::Name(value) if value.id.as_str() == "super")
                        || (matches!(call.func.as_ref(), Expr::Name(value) if value.id.as_str() == "type")
                            && call.arguments.args.len() == 1
                            && self.mandatory_receiver(&call.arguments.args[0]))
                    {
                        return;
                    }
                }
                Expr::Attribute(value)
                    if value.attr.as_str() == "__class__"
                        && self.mandatory_receiver(&value.value) =>
                {
                    return;
                }
                _ => {}
            }
            if let Some(path) = values::path(&attribute.value) {
                let inside = path.len() <= self.class_stack.len()
                    && path
                        .iter()
                        .rev()
                        .zip(self.class_stack.iter().rev())
                        .all(|(name, (class, _))| *name == class.name);
                let base = path.last().is_some_and(|name| {
                    class
                        .bases
                        .iter()
                        .any(|base| values::path(base).is_some_and(|path| path.join(".") == *name))
                });
                if inside || base {
                    return;
                }
            }
            if self.method_kind == Some(MethodKind::Class)
                && inferred.is_some_and(|value| matches!(value.target, Target::Instance(instance, _) if instance.identity == class.identity))
                && self.class_member(class.clone(), origin.clone(), name, true).is_some()
            {
                return;
            }
            if !name.starts_with("__")
                && !defaults::fixed().protected_special_methods()
                && self
                    .method_context
                    .as_deref()
                    .is_some_and(|name| defaults::fixed().special_method(name))
            {
                return;
            }
        }
        self.emit(
            "W0212",
            "protected-access",
            &format!("Access to a protected member {name} of a client class"),
            attribute.range(),
        );
    }
}
