//! Original ParamSpec declarations keep their identity and argument projection.
use super::*;

impl Reader<'_> {
    pub(super) fn parameter_spec_type(&mut self, value: &Value) -> Type {
        let key = format!(
            "param:{}:{}:{}",
            value["fullname"], value["namespace"], value["id"]
        );
        let ty = if let Some(&ty) = self.variables.get(&key) {
            ty
        } else {
            let default = if value["default"][".class"] == "AnyType"
                && value["default"]["type_of_any"] == 4
            {
                None
            } else {
                Some(self.ty(&value["default"]))
            };
            let ty = self
                .context
                .parameter_spec(value["name"].as_str().unwrap().into(), default);
            self.variables.insert(key, ty);
            ty
        };
        match value["flavor"].as_u64().unwrap() {
            0 => ty,
            1 => self.context.intern(Node::ParamSpecArgs(ty)),
            2 => self.context.intern(Node::ParamSpecKwargs(ty)),
            flavor => panic!("unsupported locked ParamSpec flavor: {flavor}"),
        }
    }

    pub(super) fn parameter_list_type(&mut self, value: &Value) -> Type {
        let parameters = value["arg_types"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let name = value["arg_names"][index].as_str().unwrap_or_default();
                let code = value["arg_kinds"][index].as_u64().unwrap();
                Parameter {
                    name: name.into(),
                    ty: Some(self.ty(item)),
                    required: matches!(code, 0 | 3),
                    kind: match code {
                        0 | 1 if name.is_empty() => Kind::PositionalOnly,
                        0 | 1 => Kind::PositionalOrKeyword,
                        2 => Kind::VariadicPositional,
                        3 | 5 => Kind::KeywordOnly,
                        4 => Kind::VariadicKeyword,
                        code => panic!("unsupported locked parameter kind: {code}"),
                    },
                }
            })
            .collect();
        self.context.intern(Node::Parameters(parameters, false))
    }
}
