//! Model fields use the original reversed MRO and stable insertion order.
use super::{Field, Model};
use crate::infrastructure::mypy::types::{Context, Type};
use std::collections::BTreeMap;

pub(super) fn inherited_fields(
    context: &Context,
    instance: Option<Type>,
    models: &BTreeMap<String, Model>,
    variables: &super::types::Variables,
    direct: &[Model],
) -> Vec<Field> {
    let inherited: Vec<_> = instance
        .and_then(|ty| context.class_id(ty))
        .map(|id| {
            context
                .class_lineage(id)
                .into_iter()
                .skip(1)
                .rev()
                .filter_map(|id| {
                    let class = context.class(id);
                    models
                        .get(&class.name)
                        .filter(|_| {
                            variables
                                .get(&class.name)
                                .and_then(|&ty| context.class_id(ty))
                                == Some(id)
                        })
                        .cloned()
                        .or_else(|| class.pydantic.clone())
                        .or_else(|| {
                            context
                                .classes
                                .borrow()
                                .embedded_pydantic_models
                                .get(&id)
                                .cloned()
                        })
                })
                .collect()
        })
        .unwrap_or_else(|| direct.iter().rev().cloned().collect());
    let mut fields: Vec<Field> = Vec::new();
    for model in inherited {
        for field in model.fields {
            if let Some(previous) = fields
                .iter_mut()
                .find(|previous| previous.name == field.name)
            {
                *previous = field;
            } else {
                fields.push(field);
            }
        }
    }
    fields
}
