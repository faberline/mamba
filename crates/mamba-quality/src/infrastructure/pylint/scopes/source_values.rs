//! Source expressions and variadic parameters retain their declaring scope.
use super::Scopes;
use mamba_language::application::quality_syntax::python_ast::Expr;

pub(super) fn defaults(
    parameters: Option<&mamba_language::application::quality_syntax::python_ast::Parameters>,
) -> std::collections::BTreeMap<String, Expr> {
    parameters
        .into_iter()
        .flat_map(|value| value.iter())
        .filter_map(|value| Some((value.name().to_string(), value.default()?.clone())))
        .collect()
}

impl Scopes {
    pub(in crate::infrastructure::pylint) fn resolve_delete(
        &mut self,
        name: &str,
        range: ruff_text_size::TextRange,
    ) -> super::Resolution {
        let resolution = self.resolve(name, range.start());
        if self.index(name).is_some_and(|index| {
            self.stack[index]
                .symbols
                .bindings
                .get(name)
                .is_some_and(|value| {
                    value.range == range && matches!(value.kind, super::DefinitionKind::Value)
                })
        }) {
            super::Resolution::Missing
        } else {
            resolution
        }
    }
    pub(in crate::infrastructure::pylint) fn values_before(
        &self,
        name: &str,
        current: usize,
        position: ruff_text_size::TextSize,
    ) -> Option<(usize, Vec<Expr>)> {
        let owner = self.index_from(name, current)?;
        if owner != current {
            return None;
        }
        let scope = &self.stack[owner];
        if let Some(values) = scope.history.before(name, position) {
            return Some((owner, values));
        }
        None
    }
    pub(in crate::infrastructure::pylint) fn parameter_before_assignment(
        &self,
        name: &str,
        current: usize,
        location: ruff_text_size::TextSize,
    ) -> bool {
        use ruff_text_size::Ranged;
        let Some(owner) = self.index_from(name, current) else {
            return false;
        };
        let scope = &self.stack[owner];
        scope.symbols.bindings.get(name).is_some_and(|definition| {
            matches!(definition.kind, super::DefinitionKind::Parameter { .. })
        }) && scope
            .first_expressions
            .get(name)
            .is_some_and(|expression| expression.start() > location)
    }
    pub(in crate::infrastructure::pylint) fn function_local(&self, name: &str) -> bool {
        let scope = self.stack.last().unwrap();
        scope.kind == super::Kind::Function && !scope.symbols.globals.contains(name)
    }
    pub(in crate::infrastructure::pylint) fn use_nonlocals(&mut self) {
        let names: Vec<_> = self
            .stack
            .last()
            .unwrap()
            .symbols
            .nonlocals
            .iter()
            .cloned()
            .collect();
        for name in names {
            self.use_name(&name);
        }
    }
    pub(in crate::infrastructure::pylint) fn first_assignment(
        &self,
        name: &str,
        range: ruff_text_size::TextRange,
    ) -> bool {
        self.index(name)
            .and_then(|index| self.stack[index].symbols.bindings.get(name))
            .is_some_and(|definition| definition.range == range)
    }
    pub(in crate::infrastructure::pylint) fn first_expression(
        &self,
        name: &str,
        current: usize,
    ) -> Option<&Expr> {
        self.stack[self.index_from(name, current)?]
            .first_expressions
            .get(name)
    }
    pub(in crate::infrastructure::pylint) fn parameter_default(
        &self,
        name: &str,
        current: usize,
    ) -> Option<(Expr, usize)> {
        let owner = self.index_from(name, current)?;
        let scope = &self.stack[owner];
        if scope
            .operand_bindings
            .get(name)
            .is_some_and(|value| !value.is_empty())
        {
            return None;
        }
        Some((
            scope.parameter_defaults.get(name)?.clone(),
            owner.saturating_sub(1),
        ))
    }
    pub(in crate::infrastructure::pylint) fn operand_expressions(
        &self,
        name: &str,
        current: usize,
    ) -> Option<(usize, Vec<Expr>)> {
        let index = self.index_from(name, current)?;
        Some((
            index,
            self.stack[index]
                .operand_bindings
                .get(name)
                .cloned()
                .unwrap_or_default(),
        ))
    }

    pub(in crate::infrastructure::pylint) fn declared_in_frame_or_module(
        &self,
        name: &str,
    ) -> bool {
        self.stack
            .last()
            .is_some_and(|scope| scope.symbols.bindings.contains_key(name))
            || self
                .stack
                .first()
                .is_some_and(|scope| scope.symbols.bindings.contains_key(name))
    }
    pub(in crate::infrastructure::pylint) fn resolve_unconsumed(
        &self,
        name: &str,
        location: ruff_text_size::TextSize,
    ) -> super::Resolution {
        let Some(index) = self.index(name) else {
            return super::Resolution::Missing;
        };
        let Some(definition) = self.stack[index].symbols.bindings.get(name) else {
            return super::Resolution::Missing;
        };
        if index == self.current_index() && location < definition.range.start() {
            super::Resolution::Before
        } else {
            super::Resolution::Defined
        }
    }
    pub(in crate::infrastructure::pylint) fn unpacking_expression(
        &self,
        name: &str,
        current: usize,
    ) -> Option<(usize, Option<Expr>)> {
        let index = self.index_from(name, current)?;
        Some((index, self.stack[index].unpacking.expression(name)))
    }
    pub(in crate::infrastructure::pylint) fn set_receiver(
        &mut self,
        name: Option<&str>,
        kind: Option<super::super::classes::MethodKind>,
    ) {
        self.stack.last_mut().unwrap().receiver =
            name.zip(kind).map(|(name, kind)| (name.to_string(), kind));
    }
    pub(in crate::infrastructure::pylint) fn assignment_receiver(
        &self,
    ) -> Option<&(String, super::super::classes::MethodKind)> {
        let index = self.current_index();
        let index = if self.stack[index].receiver_parent {
            index.checked_sub(1)?
        } else {
            index
        };
        self.stack[index].receiver.as_ref()
    }
    pub(in crate::infrastructure::pylint) fn expression_in(
        &self,
        name: &str,
        current: usize,
    ) -> Option<(usize, Option<Expr>)> {
        let index = self.index_from(name, current)?;
        Some((index, self.stack[index].expressions.get(name).cloned()))
    }
    pub(in crate::infrastructure::pylint) fn variadic_parameter(
        &self,
        name: &str,
        current: usize,
    ) -> Option<bool> {
        let scope = &self.stack[self.index_from(name, current)?];
        if scope.vararg.as_deref() == Some(name) {
            Some(true)
        } else if scope.kwarg.as_deref() == Some(name) {
            Some(false)
        } else {
            None
        }
    }
}
