//! Generator body values differ from the callable's declared container type.
use super::*;

#[derive(Clone, Copy)]
pub(in crate::infrastructure::mypy) struct GeneratorParts {
    pub yielding: Type,
    pub receiving: Type,
    pub returning: Type,
}

impl GeneratorParts {
    fn any() -> Self {
        Self {
            yielding: Type::Any,
            receiving: Type::Any,
            returning: Type::Any,
        }
    }
}

impl Context {
    pub(in crate::infrastructure::mypy) fn generator_return_valid(
        &self,
        declared: Type,
        asynchronous: bool,
    ) -> bool {
        let (kind, count) = if asynchronous {
            (Container::AsyncGenerator, 2)
        } else {
            (Container::Generator, 3)
        };
        self.accepts(
            declared,
            self.intern(Node::Container(kind, vec![Type::Any; count])),
        )
    }

    pub(in crate::infrastructure::mypy) fn generator_parts(
        &self,
        declared: Type,
    ) -> GeneratorParts {
        let expanded = self.expanded_alias_instance(declared);
        if expanded != declared {
            return self.generator_parts(expanded);
        }
        if declared == Type::Object {
            return GeneratorParts {
                yielding: Type::Any,
                receiving: Type::None,
                returning: Type::None,
            };
        }
        if self.class_id(declared).is_some_and(|id| {
            matches!(
                self.class(id).namespace.as_str(),
                "typing.Iterable" | "typing.Iterator"
            )
        }) {
            return GeneratorParts {
                yielding: self
                    .class_arguments(declared)
                    .first()
                    .copied()
                    .unwrap_or(Type::Any),
                receiving: Type::None,
                returning: Type::None,
            };
        }
        match self.node(declared).as_deref() {
            Some(Node::Container(Container::Generator, arguments)) => GeneratorParts {
                yielding: arguments[0],
                receiving: arguments[1],
                returning: arguments[2],
            },
            Some(Node::Container(Container::AsyncGenerator, arguments)) => GeneratorParts {
                yielding: arguments[0],
                receiving: arguments[1],
                returning: Type::Any,
            },
            Some(Node::Container(
                kind @ (Container::Iterator
                | Container::Iterable
                | Container::AsyncIterator
                | Container::AsyncIterable),
                arguments,
            )) => GeneratorParts {
                yielding: arguments[0],
                receiving: Type::None,
                returning: if matches!(kind, Container::AsyncIterator | Container::AsyncIterable) {
                    Type::Any
                } else {
                    Type::None
                },
            },
            Some(Node::Variable(variable)) => self.generator_parts(variable.bound),
            Some(Node::Union(items)) => {
                let parts: Vec<_> = items
                    .iter()
                    .map(|&item| self.generator_parts(item))
                    .collect();
                GeneratorParts {
                    yielding: self.union(parts.iter().map(|part| part.yielding).collect()),
                    receiving: self.union(parts.iter().map(|part| part.receiving).collect()),
                    returning: self.union(parts.iter().map(|part| part.returning).collect()),
                }
            }
            _ => GeneratorParts::any(),
        }
    }

    pub(in crate::infrastructure::mypy) fn delegated_generator(
        &self,
        actual: Type,
    ) -> Option<GeneratorParts> {
        if actual.is_any() {
            return Some(GeneratorParts::any());
        }
        let expanded = self.expanded_alias_instance(actual);
        if expanded != actual {
            return self.delegated_generator(expanded);
        }
        match self.node(actual).as_deref() {
            Some(Node::Container(Container::Generator, _)) => Some(self.generator_parts(actual)),
            Some(Node::Variable(variable)) => self.delegated_generator(variable.bound),
            Some(Node::Union(items)) => {
                let parts: Vec<_> = items
                    .iter()
                    .map(|&item| self.delegated_generator(item))
                    .collect::<Option<_>>()?;
                Some(GeneratorParts {
                    yielding: self.union(parts.iter().map(|part| part.yielding).collect()),
                    receiving: self.union(parts.iter().map(|part| part.receiving).collect()),
                    returning: self.union(parts.iter().map(|part| part.returning).collect()),
                })
            }
            _ => self.iterable_item(actual).map(|yielding| GeneratorParts {
                yielding,
                receiving: Type::None,
                returning: Type::None,
            }),
        }
    }
}
