//! Pattern metadata retains literal values without changing ordinary field types.
use super::*;

impl Context {
    pub(super) fn populate_class_pattern_values(
        &self,
        id: u32,
        statement: &StmtClassDef,
        class: &Class,
    ) {
        let mut values = BTreeMap::new();
        for member in &statement.body {
            let (targets, value): (Vec<&Expr>, &Expr) = match member {
                Stmt::Assign(assignment) => (
                    assignment.targets.iter().collect(),
                    assignment.value.as_ref(),
                ),
                Stmt::AnnAssign(assignment) => {
                    let Some(value) = assignment.value.as_deref() else {
                        continue;
                    };
                    (vec![assignment.target.as_ref()], value)
                }
                _ => continue,
            };
            for target in targets {
                let Expr::Name(name) = target else { continue };
                let literal = if name.id.as_str() == "__match_args__" {
                    if let Expr::Tuple(tuple) = value {
                        tuple
                            .elts
                            .iter()
                            .map(|item| self.literal_expression(item))
                            .collect::<Option<Vec<_>>>()
                            .map(|items| self.intern(types::Node::Tuple(items)))
                    } else {
                        None
                    }
                } else if class
                    .fields
                    .get(name.id.as_str())
                    .is_some_and(|field| field.final_attribute)
                {
                    self.literal_expression(value)
                } else {
                    None
                };
                if let Some(literal) = literal {
                    values.insert((id, name.id.to_string()), literal);
                }
            }
        }
        let mut registry = self.classes.borrow_mut();
        registry.pattern_values.retain(|(owner, _), _| *owner != id);
        registry.pattern_values.extend(values);
    }

    pub(in crate::infrastructure::mypy) fn class_pattern_value(
        &self,
        ty: Type,
        name: &str,
    ) -> Option<Type> {
        for id in self.class_lineage(self.class_id(ty)?) {
            if let Some(&ty) = self.classes.borrow().pattern_values.get(&(id, name.into())) {
                return Some(ty);
            }
        }
        None
    }
}
