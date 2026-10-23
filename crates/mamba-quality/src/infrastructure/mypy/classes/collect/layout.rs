//! Declared class layout facts control whether nominal intersections can exist.
use super::*;

impl Context {
    pub(super) fn populate_class_layout(
        &self,
        id: u32,
        statement: &StmtClassDef,
        variables: &types::Variables,
        namespace: &str,
    ) {
        let disjoint_base = statement.decorator_list.iter().any(|decorator| {
            self.typing_form(&decorator.expression, variables)
                .as_deref()
                == Some("disjoint_base")
        });
        let mut slots = std::collections::BTreeSet::new();
        for member in
            super::super::statements::collect(&statement.body, self.module_target(namespace))
        {
            if let Stmt::Assign(assignment) = member {
                if assignment.targets.iter().any(
                    |target| matches!(target, Expr::Name(name) if name.id.as_str() == "__slots__"),
                ) {
                    let items: Vec<&Expr> = match assignment.value.as_ref() {
                        Expr::Tuple(tuple) => tuple.elts.iter().collect(),
                        Expr::List(list) => list.elts.iter().collect(),
                        value => vec![value],
                    };
                    for item in items {
                        if let Expr::StringLiteral(string) = item {
                            slots.insert(string.value.to_str().to_string());
                        }
                    }
                }
            }
        }
        {
            let mut registry = self.classes.borrow_mut();
            if disjoint_base {
                registry.declared_disjoint_bases.insert(id);
            } else {
                registry.declared_disjoint_bases.remove(&id);
            }
            registry.declared_slots.insert(id, slots);
        }
    }
}
