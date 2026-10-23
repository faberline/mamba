//! Attribute resolution preserves class, instance, super, and receiver binding.
use super::{
    callables::Target,
    resolution::{Context, Resolved},
    Checker,
};

impl Checker<'_> {
    pub(super) fn resolved_attribute(&self, resolved: Resolved, name: &str) -> Option<Resolved> {
        if name == "__class__" {
            return match resolved.target {
                Target::Instance(class, origin) => Some(Resolved {
                    target: Target::Class(class),
                    context: origin,
                }),
                Target::Class(_) => Some(Resolved {
                    target: Target::Class(self.modules.builtin_class("type")?),
                    context: Context::Module("builtins".into()),
                }),
                _ => None,
            };
        }
        match resolved.target {
            Target::Class(class) => self.class_member(class, resolved.context, name, false),
            Target::Instance(class, origin) => self.class_member(class, origin, name, true),
            Target::Super(proxy) => self.super_member(&proxy, name),
            _ => None,
        }
    }
}
