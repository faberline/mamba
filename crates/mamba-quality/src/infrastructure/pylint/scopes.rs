//! Lexical lookup tracks import usage and first declarations separately from types.
use super::super::project::{
    symbols::Definition,
    symbols::{DefinitionKind, Symbols},
    Import,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, Parameters, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::collections::{BTreeMap, BTreeSet};
mod bindings;
mod declarations;
pub(super) mod patterns;
mod reimports;
mod source_values;
mod unused_imports;
mod walrus;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Module,
    Function,
    Class,
}

struct Scope {
    kind: Kind,
    symbols: Symbols,
    redirects: BTreeMap<String, usize>,
    used: BTreeSet<String>,
    targets: BTreeMap<String, super::callables::Target>,
    expressions: BTreeMap<String, Expr>,
    first_expressions: BTreeMap<String, Expr>,
    history: super::binding_history::History,
    operand_bindings: BTreeMap<String, Vec<Expr>>,
    parameter_defaults: BTreeMap<String, Expr>,
    unpacking: super::unpacking_bindings::Bindings,
    receiver: Option<(String, super::classes::MethodKind)>,
    receiver_parent: bool,
    vararg: Option<String>,
    kwarg: Option<String>,
    imports: Vec<Import>,
    direct_imports: BTreeSet<u32>,
    assignments: BTreeMap<String, TextRange>,
    global_positions: BTreeMap<String, TextRange>,
}

pub(super) struct Scopes {
    stack: Vec<Scope>,
    current: String,
    is_package: bool,
    module_definitions: BTreeSet<String>,
}

#[derive(Clone, Copy)]
pub(super) enum Resolution {
    Missing,
    Before,
    Defined,
}

impl Scopes {
    pub(super) fn new(body: &[Stmt], current: &str, is_package: bool) -> Self {
        let mut scopes = Self {
            stack: Vec::new(),
            current: current.into(),
            is_package,
            module_definitions: BTreeSet::new(),
        };
        scopes.enter(Kind::Module, body, None);
        scopes
    }

    pub(super) fn enter(&mut self, kind: Kind, body: &[Stmt], parameters: Option<&Parameters>) {
        let mut symbols = Symbols::collect(body, &self.current, self.is_package);
        for (name, range) in walrus::collect(body) {
            symbols.bindings.entry(name).or_insert(Definition {
                range,
                kind: DefinitionKind::Value,
            });
        }
        for (name, range) in patterns::collect(body) {
            symbols.bindings.entry(name).or_insert(Definition {
                range,
                kind: DefinitionKind::Value,
            });
        }
        if let Some(parameters) = parameters {
            symbols.parameters(parameters);
        }
        if kind == Kind::Module {
            self.module_definitions
                .extend(symbols.bindings.keys().cloned());
            for (name, definition) in declarations::globals(body, &self.current, self.is_package) {
                symbols.bindings.entry(name).or_insert(definition);
            }
        }
        let mut redirects: BTreeMap<_, _> = symbols
            .globals
            .iter()
            .map(|name| {
                let index = if self
                    .stack
                    .first()
                    .is_some_and(|scope| scope.symbols.bindings.contains_key(name))
                {
                    0
                } else {
                    self.stack
                        .iter()
                        .rposition(|scope| {
                            scope.kind == Kind::Function
                                && scope.symbols.bindings.contains_key(name)
                        })
                        .unwrap_or(0)
                };
                (name.clone(), index)
            })
            .collect();
        for name in &symbols.nonlocals {
            if let Some(index) = self.stack.iter().rposition(|scope| {
                scope.kind == Kind::Function && scope.symbols.bindings.contains_key(name)
            }) {
                redirects.insert(name.clone(), index);
            }
        }
        self.stack.push(Scope {
            direct_imports: body
                .iter()
                .filter(|stmt| matches!(stmt, Stmt::Import(_) | Stmt::ImportFrom(_)))
                .map(|stmt| stmt.start().to_u32())
                .collect(),
            kind,
            symbols,
            redirects,
            used: BTreeSet::new(),
            targets: super::callables::collect(body, &self.current, self.is_package),
            expressions: super::source_bindings::collect(body),
            first_expressions: super::source_bindings::collect_first(body),
            history: super::binding_history::History::collect(body),
            operand_bindings: super::operand_bindings::collect(body),
            parameter_defaults: source_values::defaults(parameters),
            unpacking: super::unpacking_bindings::collect(body),
            receiver: None,
            receiver_parent: body.iter().any(|value| matches!(value, Stmt::Nonlocal(_))),
            vararg: parameters
                .and_then(|value| value.vararg.as_ref())
                .map(|value| value.name.to_string()),
            kwarg: parameters
                .and_then(|value| value.kwarg.as_ref())
                .map(|value| value.name.to_string()),
            imports: body
                .iter()
                .filter(|statement| matches!(statement, Stmt::Import(_) | Stmt::ImportFrom(_)))
                .flat_map(|statement| {
                    super::super::project::imports(
                        std::slice::from_ref(statement),
                        &self.current,
                        self.is_package,
                    )
                })
                .collect(),
            assignments: declarations::assignments(body),
            global_positions: declarations::global_positions(body),
        });
    }

    fn index(&self, name: &str) -> Option<usize> {
        self.index_from(name, self.current_index())
    }

    pub(super) fn current_index(&self) -> usize {
        self.stack.len() - 1
    }
    pub(super) fn root_declarations(&self) -> &BTreeMap<String, Definition> {
        &self.stack[0].symbols.bindings
    }
    pub(super) fn in_class(&self) -> bool {
        self.stack.last().unwrap().kind == Kind::Class
    }
    pub(super) fn parent_is_class(&self) -> bool {
        self.stack
            .get(self.current_index().saturating_sub(1))
            .is_some_and(|scope| scope.kind == Kind::Class)
    }
    pub(super) fn in_function(&self) -> bool {
        self.stack.last().unwrap().kind == Kind::Function
    }
    pub(super) fn receiver(&mut self, name: &str, target: super::callables::Target) {
        self.stack
            .last_mut()
            .unwrap()
            .targets
            .insert(name.into(), target);
    }

    fn index_from(&self, name: &str, current: usize) -> Option<usize> {
        for (index, scope) in self.stack.iter().enumerate().take(current + 1).rev() {
            if scope.kind == Kind::Class && index != current {
                continue;
            }
            if scope
                .symbols
                .bindings
                .get(name)
                .is_some_and(|value| matches!(value.kind, DefinitionKind::Parameter { .. }))
            {
                return Some(index);
            }
            if let Some(&index) = scope.redirects.get(name) {
                return Some(index);
            }
            if scope.symbols.bindings.contains_key(name) {
                return Some(index);
            }
        }
        None
    }

    pub(super) fn resolve(&mut self, name: &str, location: TextSize) -> Resolution {
        let Some(index) = self.index(name) else {
            return Resolution::Missing;
        };
        let current = index + 1 == self.stack.len();
        let scope = &mut self.stack[index];
        scope.used.insert(name.into());
        let Some(definition) = scope.symbols.bindings.get(name) else {
            return Resolution::Missing;
        };
        if current && location < definition.range.start() {
            Resolution::Before
        } else {
            Resolution::Defined
        }
    }

    pub(super) fn resolve_outer(&mut self, name: &str, location: TextSize) -> Resolution {
        let Some(index) = self.index_from(name, self.current_index().saturating_sub(1)) else {
            return Resolution::Missing;
        };
        self.stack[index].used.insert(name.into());
        self.stack[index]
            .symbols
            .bindings
            .get(name)
            .map_or(Resolution::Missing, |definition| {
                if index == self.current_index() && location < definition.range.start() {
                    Resolution::Before
                } else {
                    Resolution::Defined
                }
            })
    }

    pub(super) fn function(&self, name: &str) -> bool {
        self.index(name)
            .and_then(|index| self.stack[index].symbols.bindings.get(name))
            .is_some_and(|definition| matches!(definition.kind, DefinitionKind::Function))
    }
    pub(super) fn declared(&self, name: &str) -> bool {
        self.index(name).is_some()
    }

    pub(super) fn is_module(&self) -> bool {
        self.stack.len() == 1
    }

    pub(super) fn declarations(&self) -> &BTreeMap<String, Definition> {
        &self.stack.last().unwrap().symbols.bindings
    }

    pub(super) fn unused(&self) -> Vec<(String, Definition)> {
        let scope = self.stack.last().unwrap();
        let mut unused: Vec<_> = scope
            .symbols
            .bindings
            .iter()
            .filter(|(name, definition)| {
                !scope.used.contains(*name)
                    && (!scope.redirects.contains_key(*name)
                        || matches!(definition.kind, DefinitionKind::Parameter { .. }))
                    && !matches!(definition.kind, DefinitionKind::Import(_))
            })
            .map(|(name, definition)| (name.clone(), definition.clone()))
            .collect();
        unused.sort_by_key(|(_, definition)| {
            (
                matches!(
                    definition.kind,
                    DefinitionKind::Parameter { variadic: true }
                ),
                definition.range.start(),
            )
        });
        unused
    }

    pub(super) fn before_definition(&self, name: &str, location: TextSize) -> bool {
        let current = self.stack.last().unwrap();
        current
            .symbols
            .bindings
            .get(name)
            .is_some_and(|definition| location < definition.range.start())
    }

    pub(super) fn enter_expression(&mut self, parameters: Option<&Parameters>) {
        self.enter(Kind::Function, &[], parameters);
    }

    pub(super) fn pattern(&mut self, target: &Expr) {
        self.stack.last_mut().unwrap().symbols.pattern(target);
    }

    pub(super) fn use_name(&mut self, name: &str) {
        if let Some(index) = self.index(name) {
            self.stack[index].used.insert(name.into());
        }
    }

    pub(super) fn call_target_in(
        &self,
        name: &str,
        current: usize,
    ) -> Option<(usize, super::callables::Target)> {
        let index = self.index_from(name, current)?;
        let scope = &self.stack[index];
        let target = scope.targets.get(name).cloned().or_else(|| {
            match &scope.symbols.bindings.get(name)?.kind {
                DefinitionKind::Import(import) => {
                    let mut import = import.clone();
                    if import.member.as_deref() == Some("*") {
                        import.member = Some(import.local.clone());
                    }
                    Some(super::callables::Target::Imported(import))
                }
                _ => None,
            }
        })?;
        Some((index, target))
    }

    pub(super) fn expand_wildcard(&mut self, import: &Import, names: BTreeSet<String>) {
        let symbols = &mut self.stack.last_mut().unwrap().symbols;
        symbols.bindings.remove("*");
        for name in names.into_iter().filter(|name| !name.starts_with('_')) {
            let mut alias = import.clone();
            alias.local = name.clone();
            symbols.bindings.entry(name).or_insert(Definition {
                range: import.range,
                kind: DefinitionKind::Import(alias),
            });
        }
    }

    pub(super) fn exports(&mut self, names: &BTreeSet<String>) -> BTreeSet<String> {
        let scope = self.stack.last_mut().unwrap();
        scope.used.extend(names.iter().cloned());
        names
            .iter()
            .filter(|name| !scope.symbols.bindings.contains_key(*name))
            .cloned()
            .collect()
    }
}
