//! Copy replacement captures the object's structural replacement method domain.
use super::super::super::types::{Node, ParameterVariance};
use super::*;

impl Context {
    pub(in crate::infrastructure::mypy) fn copy_replace_function(&self) -> Arc<Signature> {
        if let Some(signature) = self.classes.borrow().copy_replace_function.clone() {
            return signature;
        }
        let id = self.declare_class("copy", "_SupportsReplace", 0);
        let parameters = self.parameter_spec("_P".into(), None);
        let returns = self.variable_with_variance(
            "_RT_co".into(),
            Type::Object,
            Vec::new(),
            None,
            ParameterVariance::Covariant,
        );
        let receiver = self.applied_class(id, vec![parameters, returns], false);
        let tail = [
            Parameter {
                name: "_".into(),
                ty: Some(self.intern(Node::ParamSpecArgs(parameters))),
                kind: Kind::VariadicPositional,
                required: false,
            },
            Parameter {
                name: "changes".into(),
                ty: Some(self.intern(Node::ParamSpecKwargs(parameters))),
                kind: Kind::VariadicKeyword,
                required: false,
            },
        ];
        let mut method = Signature::empty("__replace__".into(), returns);
        method.constructor = false;
        method.parameters = [Parameter {
            name: "self".into(),
            ty: Some(receiver),
            kind: Kind::PositionalOnly,
            required: true,
        }]
        .into_iter()
        .chain(tail.iter().cloned())
        .collect::<Vec<_>>()
        .into();
        let mut class = (*self.class(id)).clone();
        class.protocol = true;
        class.generic = vec![parameters, returns];
        class.methods.insert(
            "__replace__".into(),
            Method {
                signature: Arc::new(method),
                kind: MethodKind::Instance,
                setter: None,
                final_method: false,
                abstractness: Abstractness::Concrete,
            },
        );
        self.replace_class(id, class);
        let mut signature = Signature::empty("replace".into(), returns);
        signature.constructor = false;
        signature.definition_module = Some("copy".into());
        signature.generic = vec![parameters, returns].into();
        signature.parameters = [Parameter {
            name: String::new(),
            ty: Some(receiver),
            kind: Kind::PositionalOnly,
            required: true,
        }]
        .into_iter()
        .chain(tail)
        .map(|mut parameter| {
            parameter.name.clear();
            parameter
        })
        .collect::<Vec<_>>()
        .into();
        let signature = Arc::new(signature);
        self.classes.borrow_mut().copy_replace_function = Some(signature.clone());
        signature
    }
}
