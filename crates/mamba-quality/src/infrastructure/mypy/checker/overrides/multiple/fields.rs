//! Nested class values retain their original multiple-inheritance compatibility.
use super::*;
use quality_syntax::python_ast::StmtClassDef;
use std::collections::BTreeSet;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_inherited_class_values(
        &mut self,
        statement: &StmtClassDef,
        id: u32,
    ) {
        let class = self.context.class(id);
        let bases: Vec<_> = self.context.class_lineage(id).into_iter().skip(1).collect();
        let names: BTreeSet<_> = bases
            .iter()
            .flat_map(|&id| {
                self.context
                    .class(id)
                    .fields
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();
        for name in names {
            if class.fields.contains_key(&name)
                || class.methods.contains_key(&name)
                || (name.starts_with("__") && !name.ends_with("__"))
            {
                continue;
            }
            let Some((index, first)) = bases
                .iter()
                .enumerate()
                .find(|(_, id)| self.context.class(**id).fields.contains_key(&name))
            else {
                continue;
            };
            let first_class = self.context.class(*first);
            let Some(first_type) = first_class.fields[&name]
                .ty
                .filter(|&ty| self.context.is_class_value(ty))
            else {
                continue;
            };
            let Some(first_instance) = self.context.instance_type(first_type) else {
                continue;
            };
            for &second in &bases[index + 1..] {
                if self.context.has_base(*first, second) {
                    continue;
                }
                let second_class = self.context.class(second);
                let Some(second_type) = second_class
                    .fields
                    .get(&name)
                    .and_then(|field| field.ty)
                    .filter(|&ty| self.context.is_class_value(ty))
                else {
                    continue;
                };
                let Some(second_instance) = self.context.instance_type(second_type) else {
                    continue;
                };
                if !self.context.accepts(second_instance, first_instance) {
                    self.emit("misc", format!("Definition of \"{name}\" in base class \"{}\" is incompatible with definition in base class \"{}\"",first_class.name,second_class.name),statement);
                }
            }
        }
    }
}
