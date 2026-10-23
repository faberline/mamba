//! Async standard forms retain their locked arity and generator defaults.
use super::*;

impl Context {
    pub(in crate::infrastructure::mypy) fn canonical_coroutine_diagnostic_view(
        &self,
        ty: Type,
        target: mamba_language::application::quality_syntax::PythonTarget,
    ) -> Option<Type> {
        let node = self.node(ty)?;
        if matches!(node.as_ref(), Node::Container(Container::Coroutine, _)) {
            return Some(ty);
        }
        let Node::AppliedClass {
            id,
            arguments,
            value: false,
        } = node.as_ref()
        else {
            return None;
        };
        if arguments.len() != 3 || self.class(*id).generic.len() != 3 {
            return None;
        }
        let embedded = self.embedded_source_class_name(*id).as_deref() == Some("typing.Coroutine")
            && self.module_target(&self.class(*id).namespace) == target;
        let native = self
            .builtin_value_class_instance("typing.Coroutine", target)
            .and_then(|instance| self.class_id(instance))
            == Some(*id);
        (embedded || native)
            .then(|| self.intern(Node::Container(Container::Coroutine, arguments.clone())))
    }

    pub(in crate::infrastructure::mypy) fn asynchronous_kind(name: &str) -> Option<Container> {
        Some(match name {
            "Awaitable" => Container::Awaitable,
            "Coroutine" => Container::Coroutine,
            "Generator" => Container::Generator,
            "Iterator" => Container::Iterator,
            "AsyncGenerator" => Container::AsyncGenerator,
            "AsyncIterator" => Container::AsyncIterator,
            "AsyncIterable" => Container::AsyncIterable,
            _ => return None,
        })
    }

    pub(in crate::infrastructure::mypy) fn asynchronous_arity(kind: Container) -> (usize, usize) {
        match kind {
            Container::Coroutine => (3, 3),
            Container::Generator => (1, 3),
            Container::AsyncGenerator => (1, 2),
            _ => (1, 1),
        }
    }

    pub(super) fn asynchronous_annotation(
        &self,
        expression: &Expr,
        variables: &VariableMap,
    ) -> Option<Type> {
        let (base, expressions) = match expression {
            Expr::Subscript(subscript) => {
                let args: Vec<_> = match subscript.slice.as_ref() {
                    Expr::Tuple(tuple) => tuple.elts.iter().collect(),
                    argument => vec![argument],
                };
                (subscript.value.as_ref(), args)
            }
            Expr::Name(_) | Expr::Attribute(_) => (expression, Vec::new()),
            _ => return None,
        };
        let name = self.typing_form(base, variables)?;
        let kind = Self::asynchronous_kind(&name)?;
        let (minimum, maximum) = Self::asynchronous_arity(kind);
        let mut arguments = if (minimum..=maximum).contains(&expressions.len()) {
            expressions
                .into_iter()
                .map(|value| self.annotation_with_variables(value, variables))
                .collect::<Option<Vec<_>>>()?
        } else {
            vec![Type::Any]
        };
        if matches!(kind, Container::Generator | Container::AsyncGenerator) {
            arguments.resize(maximum, Type::None);
        } else {
            arguments.resize(maximum, Type::Any);
        }
        Some(self.intern(Node::Container(kind, arguments)))
    }
}
