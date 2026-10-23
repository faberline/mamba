//! Attribute naming retains property categories and first instance declarations.
use super::{classes::Class, resolution::Context, Checker};
use std::{collections::BTreeSet, rc::Rc};
impl Checker<'_> {
    pub(super) fn inherited_class_name(&self, name: &str) -> bool {
        if !self.scopes.in_class() {
            return false;
        }
        self.class_stack.last().is_some_and(|(class, origin)| {
            self.lineage(class.clone(), origin.clone())
                .into_iter()
                .skip(1)
                .any(|(base, _)| base.declared_members.contains(name))
        })
    }
    pub(super) fn check_annotated_assignment_name(
        &mut self,
        value: &mamba_language::application::quality_syntax::python_ast::StmtAnnAssign,
    ) {
        if let Some(expression) = &value.value {
            self.check_assignment_name(&value.target, expression);
        } else if self.scopes.in_class() {
            if let mamba_language::application::quality_syntax::python_ast::Expr::Name(name) =
                value.target.as_ref()
            {
                if !self.inherited_class_name(name.id.as_str()) {
                    self.check_bad_name(name.id.as_str(), ruff_text_size::Ranged::range(name));
                }
            }
        }
    }

    pub(super) fn check_instance_attribute_names(&mut self, class: Rc<Class>, origin: Context) {
        let mut seen = BTreeSet::new();
        for value in &class.attribute_definitions {
            if !seen.insert(&value.name) {
                continue;
            }
            let inherited = self
                .lineage(class.clone(), origin.clone())
                .into_iter()
                .skip(1)
                .any(|(base, _)| base.attributes.contains_key(&value.name));
            if !inherited {
                let start = self.diagnostics.len();
                self.check_declaration_name(&value.name, "attr", value.range);
                self.check_non_ascii_name(&value.name, "Attribute", value.range);
                if let Some(module) = value.module.as_deref().filter(|m| *m != self.module_name) {
                    if let Some(source) = self
                        .modules
                        .source_index(self.source.project, module)
                        .and_then(|index| self.snapshots.get(index))
                    {
                        for diagnostic in &mut self.diagnostics[start..] {
                            *diagnostic = crate::infrastructure::diagnostics::at(
                                source,
                                crate::domain::model::QualityTool::Pylint,
                                &diagnostic.rule,
                                diagnostic.message.clone(),
                                value.range,
                            );
                        }
                    }
                }
            }
        }
    }
}
