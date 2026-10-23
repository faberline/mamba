//! Class keyword checks bind the inherited subclass hook without running it.
use super::super::*;
use quality_syntax::python_ast::{ExprCall, StmtClassDef};

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_dataclass_class_keywords(
        &mut self,
        class: &StmtClassDef,
    ) {
        let Some(ty) = self.scopes.class_object(class.name.as_str()) else {
            return;
        };
        let Some(id) = self.context.class_id(ty) else {
            return;
        };
        if self.context.dataclass_transform_metaclass(id) {
            return;
        }
        let Some(arguments) = class.arguments.as_deref() else {
            return;
        };
        let Some(function) = arguments.args.first() else {
            return;
        };
        let mut arguments = arguments.clone();
        arguments.args.clear();
        arguments.keywords.retain(|keyword| {
            keyword
                .arg
                .as_ref()
                .is_none_or(|name| name.as_str() != "metaclass")
        });
        let first = class
            .body
            .first()
            .map_or(class.end(), Ranged::start)
            .to_usize();
        let prefix = &self.source.text[..class.start().to_usize()];
        let line_start = prefix.rfind('\n').map_or(0, |position| position + 1);
        let indent = prefix.len() - line_start;
        let end = self.source.text[..first]
            .rfind('\n')
            .map_or(first, |position| position + 1)
            + indent
            + 1;
        arguments.range = ruff_text_size::TextRange::new(
            class.start(),
            ruff_text_size::TextSize::try_from(end).expect("class header fits in u32"),
        );
        let call = ExprCall {
            node_index: Default::default(),
            range_start: class.start(),
            func: Box::new(function.clone()),
            arguments,
        };
        let signature = self
            .context
            .instance_type(ty)
            .and_then(|instance| self.context.class_method(instance, "__init_subclass__"))
            .unwrap_or_else(|| {
                let mut signature =
                    signatures::Signature::empty("__init_subclass__".into(), types::Type::None);
                signature.owner = Some("object".into());
                signature.constructor = false;
                std::sync::Arc::new(signature)
            });
        self.bind_call(&call, &signature);
    }
}
