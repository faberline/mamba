//! Async iteration resolves the iterator member and awaits each next result.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::infrastructure::mypy) enum AsyncIterationError {
    MissingIterator,
    MissingNext(Type),
    NonAwaitableNext(Type),
}

impl Context {
    pub(in crate::infrastructure::mypy) fn async_iterable_item(
        &self,
        ty: Type,
    ) -> Result<Type, AsyncIterationError> {
        if ty.is_any() {
            return Ok(Type::Any);
        }
        let expanded = self.expanded_alias_instance(ty);
        match self.node(expanded).as_deref() {
            Some(Node::Container(
                Container::AsyncIterator | Container::AsyncIterable | Container::AsyncGenerator,
                arguments,
            )) => return Ok(arguments[0]),
            Some(Node::Variable(variable)) => {
                return self.async_iterable_item(variable.bound);
            }
            Some(Node::Union(items)) => {
                let items = items
                    .iter()
                    .map(|&item| self.async_iterable_item(item))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(self.union(items));
            }
            _ => {}
        }
        let iterator = self
            .class_method(expanded, "__aiter__")
            .and_then(|signature| signature.returns)
            .ok_or(AsyncIterationError::MissingIterator)?;
        let expanded_iterator = self.expanded_alias_instance(iterator);
        if iterator.is_any() {
            return Ok(Type::Any);
        }
        if let Some(Node::Container(
            Container::AsyncIterator | Container::AsyncGenerator,
            arguments,
        )) = self.node(expanded_iterator).as_deref()
        {
            return Ok(arguments[0]);
        }
        let next = self
            .class_method(expanded_iterator, "__anext__")
            .and_then(|signature| signature.returns)
            .ok_or(AsyncIterationError::MissingNext(iterator))?;
        self.await_result(next)
            .ok_or(AsyncIterationError::NonAwaitableNext(next))
    }
}
