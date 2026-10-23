//! Generated ordering methods share a self type with their second argument.
use super::*;

pub(in crate::infrastructure::mypy) fn order_symbol(name: &str) -> Option<&'static str> {
    match name {
        "__lt__" => Some("<"),
        "__gt__" => Some(">"),
        "__le__" => Some("<="),
        "__ge__" => Some(">="),
        _ => None,
    }
}

impl Context {
    pub(in crate::infrastructure::mypy) fn dataclass_order_variable(
        &self,
        id: u32,
        name: &str,
    ) -> Type {
        let key = (id, name.to_string());
        if let Some(&variable) = self.classes.borrow().dataclass_order_variables.get(&key) {
            return variable;
        }
        let variable = self.variable("_DT".into(), Type::Object, Vec::new(), None);
        self.classes
            .borrow_mut()
            .inference_defaults
            .insert(variable, Type::Any);
        self.classes
            .borrow_mut()
            .dataclass_order_variables
            .insert(key, variable);
        variable
    }

    pub(super) fn dataclass_order_methods(
        &self,
        class: &mut Class,
        statement: &StmtClassDef,
    ) -> Vec<(String, TextRange)> {
        let id = self.class_id(class.instance).unwrap();
        let conflicts = statement
            .body
            .iter()
            .filter_map(|member| {
                let Stmt::FunctionDef(function) = member else {
                    return None;
                };
                order_symbol(function.name.as_str())?;
                let range = function
                    .decorator_list
                    .first()
                    .map_or(function.range(), |decorator| {
                        TextRange::new(decorator.expression.start(), function.end())
                    });
                Some((function.name.to_string(), range))
            })
            .collect();
        for name in ["__lt__", "__gt__", "__le__", "__ge__"] {
            let variable = self.dataclass_order_variable(id, name);
            let mut signature = Signature::empty(name.into(), Type::Bool);
            signature.constructor = false;
            signature.owner = Some(class.name.clone());
            signature.generic = class
                .generic
                .iter()
                .copied()
                .chain([variable])
                .collect::<Vec<_>>()
                .into();
            signature.parameters = ["self", "other"]
                .into_iter()
                .map(|name| Parameter {
                    name: name.into(),
                    ty: Some(variable),
                    kind: Kind::PositionalOrKeyword,
                    required: true,
                })
                .collect::<Vec<_>>()
                .into();
            class.methods.insert(
                name.into(),
                Method {
                    signature: Arc::new(signature),
                    kind: MethodKind::Instance,
                    setter: None,
                    final_method: false,
                    abstractness: Abstractness::Concrete,
                },
            );
        }
        conflicts
    }
}
