//! Finite iterator state follows the original assignment and callback scopes.
use super::{callables::Target, repeated_iterator_sites::Sites, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    Expr, ExprCall, ExprName, Stmt, StmtAssign, StmtFor,
};
use ruff_text_size::Ranged;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
enum Binding {
    Safe,
    Iterator(Option<u32>),
}
pub(super) struct State {
    sites: Sites,
    frames: Vec<BTreeMap<String, Binding>>,
}
impl State {
    pub(super) fn new(body: &[Stmt]) -> Self {
        Self {
            sites: Sites::new(body),
            frames: vec![BTreeMap::new()],
        }
    }
    pub(super) fn enter(&mut self) {
        self.frames.push(BTreeMap::new());
    }
    pub(super) fn leave(&mut self) {
        self.frames.pop().expect("iterator callback scope");
    }
    fn assign(&mut self, name: String, binding: Binding) {
        self.frames
            .last_mut()
            .expect("iterator callback scope")
            .insert(name, binding);
    }
    fn reused(&self, name: &ExprName) -> bool {
        let binding = self
            .frames
            .iter()
            .rev()
            .find_map(|frame| frame.get(name.id.as_str()));
        let Some(Binding::Iterator(definition)) = binding else {
            return false;
        };
        let Some(loops) = self.sites.ancestors.get(&name.start().to_u32()) else {
            return false;
        };
        if loops.len() < 2 {
            return false;
        }
        let boundary = definition
            .and_then(|definition| loops.iter().position(|value| *value == definition))
            .unwrap_or(loops.len());
        if boundary <= 1 {
            return false;
        }
        let Some(inner) = self.sites.loops.get(&loops[0]) else {
            return false;
        };
        if inner.early {
            return false;
        }
        let mut nested = loops[0];
        for offset in &loops[1..boundary] {
            let Some(loop_) = self.sites.loops.get(offset) else {
                return false;
            };
            let Some(tail) = loop_.children.get(&nested) else {
                return false;
            };
            if tail.exits || loop_.iterable_name.as_deref() == Some(name.id.as_str()) {
                return false;
            }
            if !tail.breaks {
                return true;
            }
            nested = *offset;
        }
        false
    }
}
fn known(name: &str) -> bool {
    matches!(
        name,
        "builtins.map"
            | "builtins.filter"
            | "builtins.zip"
            | "builtins.iter"
            | "builtins.reversed"
            | "builtins.enumerate"
            | "itertools.chain"
            | "itertools.islice"
            | "itertools.groupby"
            | "itertools.starmap"
            | "itertools.accumulate"
            | "itertools.product"
            | "itertools.permutations"
            | "itertools.combinations"
            | "itertools.combinations_with_replacement"
            | "itertools.compress"
            | "itertools.dropwhile"
            | "itertools.takewhile"
            | "itertools.filterfalse"
            | "itertools.zip_longest"
            | "itertools.pairwise"
            | "itertools.batched"
    )
}
impl Checker<'_> {
    pub(super) fn track_repeated_iterator(&mut self, statement: &StmtAssign) {
        let iterator = match statement.value.as_ref() {
            Expr::Generator(_) => true,
            Expr::Call(call) => self.expression_target(&call.func).is_some_and(|resolved| matches!(resolved.target, Target::Function(function) if function.category == "function" && function.generates)) || self.stdlib_name(&call.func).is_some_and(|name| known(&name)),
            _ => false,
        };
        let binding = if iterator {
            Binding::Iterator(
                self.repeated_iterators
                    .sites
                    .ancestors
                    .get(&statement.value.start().to_u32())
                    .and_then(|values| values.first())
                    .copied(),
            )
        } else {
            Binding::Safe
        };
        for target in &statement.targets {
            if let Expr::Name(value) = target {
                self.repeated_iterators
                    .assign(value.id.to_string(), binding);
            }
        }
    }
    pub(super) fn enter_repeated_iterator_loop(&mut self, loop_: &StmtFor) {
        if loop_.is_async {
            return;
        }
        for name in super::local_rules::target_names(&loop_.target) {
            self.repeated_iterators.assign(name, Binding::Safe);
        }
        self.repeated_iterators.enter();
        if let Expr::Name(value) = loop_.iter.as_ref() {
            self.check_repeated_iterator_name(value);
        }
    }
    pub(super) fn check_repeated_iterator_call(&mut self, call: &ExprCall) {
        if matches!(call.func.as_ref(), Expr::Name(value) if value.id.as_str() == "next") {
            return;
        }
        for argument in &call.arguments.args {
            if let Expr::Name(value) = argument {
                self.check_repeated_iterator_name(value);
            }
        }
    }
    fn check_repeated_iterator_name(&mut self, name: &ExprName) {
        if self.repeated_iterators.reused(name) {
            self.emit(
                "W4801",
                "looping-through-iterator",
                &format!(
                    "Iterator '{}' from an outer scope is reused or consumed in a nested loop.",
                    name.id
                ),
                name.range(),
            );
        }
    }
}
