//! Scalar constructors share their locked overload and conversion input types.
use super::super::signatures::{Kind, Parameter, Signature};
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;
use std::sync::Arc;

impl Context {
    pub(in crate::infrastructure::mypy) fn builtin_constructor(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        let support = |name| self.conversion_protocol(name);
        let buffer = || support("Buffer");
        let index = || support("SupportsIndex");
        let union = |items| self.union(items);
        let signature = match ty {
            Type::Object => Signature::empty("object".into(), ty),
            Type::Bool => scalar_signature(
                "bool",
                ty,
                vec![argument("o", Type::Object, Kind::PositionalOnly, false)],
            ),
            Type::Float => scalar_signature(
                "float",
                ty,
                vec![argument(
                    "x",
                    union(vec![Type::Str, buffer(), support("SupportsFloat"), index()]),
                    Kind::PositionalOnly,
                    false,
                )],
            ),
            Type::Int => {
                let mut inputs = vec![Type::Str, buffer(), support("SupportsInt"), index()];
                if target != PythonTarget::Py314 {
                    inputs.push(support("SupportsTrunc"));
                }
                let array = self.declare_class("<builtins>", "bytearray", 101);
                overloaded(
                    "int",
                    ty,
                    vec![
                        vec![argument("x", union(inputs), Kind::PositionalOnly, false)],
                        vec![
                            argument(
                                "x",
                                union(vec![Type::Str, Type::Bytes, self.class(array).instance]),
                                Kind::PositionalOnly,
                                true,
                            ),
                            argument("base", index(), Kind::PositionalOrKeyword, true),
                        ],
                    ],
                )
            }
            Type::Complex => overloaded(
                "complex",
                ty,
                vec![
                    vec![
                        argument(
                            "real",
                            union(vec![
                                ty,
                                support("SupportsComplex"),
                                support("SupportsFloat"),
                                index(),
                            ]),
                            Kind::PositionalOrKeyword,
                            false,
                        ),
                        argument(
                            "imag",
                            union(vec![ty, support("SupportsFloat"), index()]),
                            Kind::PositionalOrKeyword,
                            false,
                        ),
                    ],
                    vec![argument(
                        "real",
                        union(vec![
                            Type::Str,
                            support("SupportsComplex"),
                            support("SupportsFloat"),
                            index(),
                            ty,
                        ]),
                        Kind::PositionalOrKeyword,
                        true,
                    )],
                ],
            ),
            Type::Str => overloaded(
                "str",
                ty,
                vec![
                    vec![argument(
                        "object",
                        Type::Object,
                        Kind::PositionalOrKeyword,
                        false,
                    )],
                    vec![
                        argument("object", buffer(), Kind::PositionalOrKeyword, true),
                        argument("encoding", Type::Str, Kind::PositionalOrKeyword, false),
                        argument("errors", Type::Str, Kind::PositionalOrKeyword, false),
                    ],
                ],
            ),
            Type::Bytes => {
                let iterable = self.intern(Node::Container(Container::Iterable, vec![index()]));
                overloaded(
                    "bytes",
                    ty,
                    vec![
                        vec![argument(
                            "o",
                            union(vec![iterable, index(), support("SupportsBytes"), buffer()]),
                            Kind::PositionalOnly,
                            true,
                        )],
                        vec![
                            argument("string", Type::Str, Kind::PositionalOnly, true),
                            argument("encoding", Type::Str, Kind::PositionalOrKeyword, true),
                            argument("errors", Type::Str, Kind::PositionalOrKeyword, false),
                        ],
                        vec![],
                    ],
                )
            }
            _ => return None,
        };
        Some(Arc::new(signature))
    }
}

fn argument(name: &str, ty: Type, kind: Kind, required: bool) -> Parameter {
    Parameter {
        name: name.into(),
        ty: Some(ty),
        kind,
        required,
    }
}

fn scalar_signature(name: &str, returned: Type, parameters: Vec<Parameter>) -> Signature {
    let mut signature = Signature::empty(name.into(), returned);
    signature.parameters = parameters.into();
    signature
}

fn overloaded(name: &str, returned: Type, variants: Vec<Vec<Parameter>>) -> Signature {
    let mut signature = Signature::empty(name.into(), returned);
    signature.overloads = variants
        .into_iter()
        .map(|parameters| Arc::new(scalar_signature(name, returned, parameters)))
        .collect();
    signature
}
