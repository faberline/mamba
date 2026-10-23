//! Generated class attributes keep their literal tuple or slot element types.
use super::super::super::types::{Container, Literal, Node};
use super::*;

fn generated(ty: Type) -> Field {
    Field {
        ty: Some(ty),
        needs_annotation: false,
        class_access: true,
        readonly: false,
        final_attribute: false,
        class_var: false,
        abstract_field: false,
    }
}

impl Context {
    pub(super) fn dataclass_metadata(
        &self,
        class: &mut Class,
        statement: &StmtClassDef,
        options: Option<&Arguments>,
        attributes: &[Attribute],
        sentinels: &[String],
        transformed: bool,
    ) -> bool {
        let mut fields = generated(if transformed {
            self.intern(Node::Container(Container::Dict, vec![Type::Str, Type::Any]))
        } else {
            self.dataclass_fields_type()
        });
        fields.class_var = true;
        class.fields.insert("__dataclass_fields__".into(), fields);
        if flag(options, "match_args", true) && !class.fields.contains_key("__match_args__") {
            let items = attributes
                .iter()
                .filter(|attribute| attribute.in_init && !attribute.keyword_only)
                .map(|attribute| self.intern(Node::Literal(Literal::Str(attribute.name.clone()))))
                .collect();
            class.fields.insert(
                "__match_args__".into(),
                generated(self.intern(Node::Tuple(items))),
            );
        } else {
            for member in &statement.body {
                let Stmt::Assign(assignment) = member else {
                    continue;
                };
                if !assignment.targets.iter().any(|target| {
                    matches!(target, Expr::Name(name) if name.id.as_str() == "__match_args__")
                }) {
                    continue;
                }
                let Expr::Tuple(tuple) = assignment.value.as_ref() else {
                    continue;
                };
                let items: Option<Vec<_>> = tuple
                    .elts
                    .iter()
                    .map(|value| {
                        let literal = self.literal_expression(value)?;
                        let node = self.node(literal)?;
                        let Node::Literal(value) = node.as_ref() else {
                            return None;
                        };
                        Some(self.intern(Node::LiteralValue(value.clone())))
                    })
                    .collect();
                if let Some(items) = items {
                    class.fields.insert(
                        "__match_args__".into(),
                        generated(self.intern(Node::Tuple(items))),
                    );
                }
            }
        }
        if !flag(options, "slots", false) {
            return false;
        }
        if class.fields.contains_key("__slots__") {
            return true;
        }
        let id = self.class_id(class.instance).unwrap();
        if self.class_lineage(id).into_iter().skip(1).any(|parent| {
            let parent = self.class(parent);
            parent.namespace != "<builtins>" && !parent.fields.contains_key("__slots__")
        }) {
            return false;
        }
        let count = attributes.len() + sentinels.len();
        class.fields.insert(
            "__slots__".into(),
            generated(self.intern(Node::Tuple(vec![Type::Str; count]))),
        );
        false
    }
}
