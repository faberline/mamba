//! Dataclass declaration errors retain their source field location.
use super::*;
use quality_syntax::python_ast::StmtClassDef;
mod class_options;
mod comparisons;
mod fields;
mod ordering;
mod replace;
mod transforms;

impl Checker<'_> {
    pub(super) fn check_dataclass_declaration(&mut self, class: &StmtClassDef) {
        self.check_dataclass_transform_class(class);
        let Some(id) = self
            .scopes
            .class_object(class.name.as_str())
            .and_then(|ty| self.context.class_id(ty))
        else {
            return;
        };
        let model = self.context.class(id);
        let Some(definition) = model.dataclass.clone() else {
            return;
        };
        self.check_dataclass_transform_options(class, &definition);
        for (message, range) in definition.invalid_descriptors {
            self.emit("misc", message, &range);
        }
        if let Some(range) = definition.invalid_eq {
            self.emit(
                "misc",
                "\"eq\" must be True if \"order\" is True".into(),
                &range,
            );
        }
        for (name, range) in definition.order_conflicts {
            self.emit(
                "misc",
                format!("You may not have a custom \"{name}\" method when \"order\" is True"),
                &range,
            );
        }
        let prefix = &self.source.text[..class.name.start().to_usize()];
        let line_start = prefix.rfind('\n').map_or(0, |offset| offset + 1);
        let line = &prefix[line_start..];
        let start =
            ruff_text_size::TextSize::try_from(line_start + line.len() - line.trim_start().len())
                .expect("class offset fits in u32");
        if self
            .context
            .class_lineage(id)
            .into_iter()
            .skip(1)
            .any(|parent| {
                self.context
                    .class(parent)
                    .dataclass
                    .as_ref()
                    .is_some_and(|parent| {
                        !parent.neutral_frozen && parent.frozen != definition.frozen
                    })
            })
        {
            let message = if definition.frozen {
                "Frozen dataclass cannot inherit from a non-frozen dataclass"
            } else {
                "Non-frozen dataclass cannot inherit from a frozen dataclass"
            };
            self.emit(
                "misc",
                message.into(),
                &ruff_text_size::TextRange::new(start, class.end()),
            );
        }
        if definition.slots_conflict {
            self.emit(
                "misc",
                format!(
                    "\"{}\" both defines \"__slots__\" and is used with \"slots=True\"",
                    class.name
                ),
                &ruff_text_size::TextRange::new(start, class.end()),
            );
        }
        for range in definition.duplicate_kw_only {
            self.emit(
                "misc",
                "There may not be more than one field with the KW_ONLY type".into(),
                &ruff_text_size::TextRange::at(range.start(), ruff_text_size::TextSize::new(1)),
            );
        }
        for range in definition.invalid_order {
            self.emit(
                "misc",
                "Attributes without a default cannot follow attributes with one".into(),
                &ruff_text_size::TextRange::at(range.start(), ruff_text_size::TextSize::new(1)),
            );
        }
        if definition.transformed && definition.class_options {
            self.check_dataclass_class_keywords(class);
        }
    }
}
