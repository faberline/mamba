//! Member-before-definition checks preserve first-scope and constructor-call gates.
use super::{
    classes::Class,
    defaults,
    member_access_context::exclusive,
    member_access_scan::{self, Node},
    resolution::Context,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::StmtClassDef;
use ruff_source_file::LineIndex;
use std::{collections::BTreeSet, rc::Rc};

impl Checker<'_> {
    pub(super) fn check_member_access(
        &mut self,
        statement: &StmtClassDef,
        class: Rc<Class>,
        origin: Context,
    ) {
        if defaults::fixed().ignored_mixin(&class.name, "attribute-defined-outside-init") {
            return;
        }
        let lineage = self.lineage(class, origin);
        if lineage
            .iter()
            .any(|(class, _)| class.identity.0 == "builtins" && class.name == "type")
        {
            return;
        }
        let data = member_access_scan::collect(&statement.body);
        let index = LineIndex::from_source_text(&self.source.text);
        for (name, accesses) in &data.accesses {
            if lineage.iter().any(|(class, origin)| {
                class.declared_members.contains(name)
                    || self
                        .local_class_member(class, origin.clone(), name, false)
                        .is_some()
            }) || lineage
                .iter()
                .skip(1)
                .any(|(class, _)| class.attributes.contains_key(name))
            {
                continue;
            }
            let Some(definitions) = data.definitions.get(name) else {
                continue;
            };
            let Some(first) = definitions.first() else {
                continue;
            };
            if definitions
                .iter()
                .skip(1)
                .any(|node| node.frame != first.frame)
            {
                continue;
            }
            let line = index
                .line_column(first.range.start(), &self.source.text)
                .line
                .get();
            let mut candidates: Vec<&Node> = accesses.iter().map(|access| &access.node).collect();
            if data
                .frames
                .get(&first.frame)
                .is_some_and(|(name, _)| name == "__init__")
            {
                for access in accesses {
                    let Some((method, true)) = data.frames.get(&access.node.frame) else {
                        continue;
                    };
                    if let Some((_, calls)) = data.accesses.iter().find(|(name, _)| name == method)
                    {
                        candidates.extend(
                            calls
                                .iter()
                                .filter(|access| access.call)
                                .map(|access| &access.node),
                        );
                    }
                }
            }
            let mut seen = BTreeSet::new();
            for access in candidates {
                if !seen.insert(access.range.start())
                    || access.frame != first.frame
                    || index
                        .line_column(access.range.start(), &self.source.text)
                        .line
                        .get()
                        >= line
                    || exclusive(&access.parents, &first.parents)
                {
                    continue;
                }
                self.emit(
                    "E0203",
                    "access-member-before-definition",
                    &format!("Access to member '{name}' before its definition line {line}"),
                    access.range,
                );
            }
        }
    }
}
