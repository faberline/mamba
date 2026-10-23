//! The first inherited declaration must be compatible with unrelated later bases.
mod fields;
use super::*;
use quality_syntax::python_ast::StmtClassDef;
use std::collections::BTreeSet;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_inherited_methods(
        &mut self,
        statement: &StmtClassDef,
    ) {
        let Some(id) = self
            .scopes
            .class_object(statement.name.as_str())
            .and_then(|ty| self.context.class_id(ty))
        else {
            return;
        };
        let class = self.context.class(id);
        if class.bases.len() <= 1 {
            return;
        }
        self.check_inherited_class_values(statement, id);
        let bases: Vec<_> = self.context.class_lineage(id).into_iter().skip(1).collect();
        let names: BTreeSet<_> = bases
            .iter()
            .flat_map(|&id| {
                self.context
                    .class(id)
                    .methods
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();
        for name in names {
            if class.methods.contains_key(&name)
                || class.fields.contains_key(&name)
                || (name.starts_with("__") && !name.ends_with("__"))
                || matches!(
                    name.as_str(),
                    "__init__" | "__new__" | "__init_subclass__" | "__replace__"
                )
            {
                continue;
            }
            let Some((index, first)) = bases
                .iter()
                .enumerate()
                .find(|(_, id)| self.context.class(**id).methods.contains_key(&name))
            else {
                continue;
            };
            let first_class = self.context.class(*first);
            let first_method =
                self.context
                    .specialize_method(&first_class.methods[&name], class.instance, *first);
            for &second in &bases[index + 1..] {
                let second_class = self.context.class(second);
                let Some(second_method) = second_class.methods.get(&name) else {
                    continue;
                };
                let second_method =
                    self.context
                        .specialize_method(second_method, class.instance, second);
                if self.context.has_base(*first, second) {
                    continue;
                }
                let compatible = if first_method.kind == MethodKind::Property
                    && second_method.kind == MethodKind::Property
                {
                    self.context.accepts(
                        second_method.signature.returns.unwrap_or(Type::Any),
                        first_method.signature.returns.unwrap_or(Type::Any),
                    )
                } else if first_method.kind != MethodKind::Property
                    && second_method.kind != MethodKind::Property
                {
                    shape::compatible(
                        &self.context,
                        &second_method
                            .signature
                            .bound(usize::from(second_method.kind != MethodKind::Static)),
                        &first_method
                            .signature
                            .bound(usize::from(first_method.kind != MethodKind::Static)),
                    )
                } else {
                    false
                };
                if !compatible {
                    self.emit("misc", format!("Definition of \"{name}\" in base class \"{}\" is incompatible with definition in base class \"{}\"", first_class.name, second_class.name), statement);
                }
            }
        }
    }
}
