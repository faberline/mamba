//! Context values retain Python type identity and generator decorators.
use super::operand_inference::Operand;

#[derive(Clone)]
pub(super) enum Inferred {
    Operand(Operand),
    Function {
        signature: std::rc::Rc<super::callables::Signature>,
        origin: super::resolution::Context,
        bound: bool,
    },
    Generator {
        asynchronous: bool,
        name: String,
        decorators: Vec<String>,
        wrappers: Vec<String>,
        signature: std::rc::Rc<super::callables::Signature>,
        origin: super::resolution::Context,
    },
}
impl Inferred {
    pub(super) fn unknown() -> Self {
        Self::Operand(Operand::Unknown)
    }
    fn identity(&self) -> Option<String> {
        match self {
            Self::Function { bound, .. } => Some(
                if *bound {
                    "builtins.instancemethod"
                } else {
                    "builtins.function"
                }
                .into(),
            ),
            Self::Generator { asynchronous, .. } => Some(
                if *asynchronous {
                    "builtins.async_generator"
                } else {
                    "builtins.generator"
                }
                .into(),
            ),
            Self::Operand(Operand::Literal(name)) => Some(format!("builtins.{name}")),
            Self::Operand(Operand::Instance(class, _)) => {
                Some(format!("{}.{}", class.identity.0, class.name))
            }
            Self::Operand(Operand::Class(_, _)) => Some("builtins.type".into()),
            Self::Operand(Operand::Function(_)) => Some("builtins.function".into()),
            Self::Operand(Operand::Generator) => Some("builtins.generator".into()),
            _ => None,
        }
    }
    pub(super) fn join(values: Vec<Self>) -> Self {
        let Some(first) = values.first() else {
            return Self::unknown();
        };
        let Some(identity) = first.identity() else {
            return Self::unknown();
        };
        if values
            .iter()
            .all(|value| value.identity().as_deref() == Some(identity.as_str()))
        {
            first.clone()
        } else {
            Self::unknown()
        }
    }
}
