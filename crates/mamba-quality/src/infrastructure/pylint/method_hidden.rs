//! Method hiding follows inferred instance assignments and ancestor exemptions.
use super::{classes::MethodKind, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtFunctionDef};
use ruff_source_file::LineIndex;
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn check_hidden_method(&mut self, function: &StmtFunctionDef, kind: MethodKind) {
        if kind == MethodKind::Property || function.decorator_list.iter().any(|decorator| matches!(&decorator.expression, Expr::Attribute(value) if matches!(value.attr.as_str(), "getter" | "setter" | "deleter"))) { return; }
        let Some((class, origin)) = self.class_stack.last().cloned() else {
            return;
        };
        let name = function.name.as_str();
        let lineage = self.lineage(class.clone(), origin.clone());
        if lineage.iter().skip(1).any(|(class, _)| {
            class.methods.contains_key(name)
                || (name.starts_with("__")
                    && !name.ends_with("__")
                    && class.attributes.contains_key(name))
        }) {
            return;
        }
        if let Some(slots) = self.layout_slots(class.clone(), origin) {
            if !slots
                .iter()
                .any(|slot| slot.named(name) || slot.named("__dict__"))
            {
                return;
            }
        }
        for (class, _) in lineage {
            if let Some(attribute) = class
                .attribute_definitions
                .iter()
                .find(|value| value.name == name)
            {
                let source = self
                    .modules
                    .source(self.source.project, &class.identity.0)
                    .unwrap_or(&self.source.text);
                let line = LineIndex::from_source_text(source)
                    .line_column(attribute.range.start(), source)
                    .line
                    .get();
                self.emit(
                    "E0202",
                    "method-hidden",
                    &format!(
                        "An attribute defined in {} line {line} hides this method",
                        class.identity.0
                    ),
                    self.declaration_range(function.name.range()),
                );
                return;
            }
        }
    }
}
