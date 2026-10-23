//! Loop replay retains inferred declarations without re-entering their first site.
use super::*;
use ruff_text_size::TextRange;

#[derive(Clone)]
pub(super) struct ReplayWindow {
    depth: usize,
    range: TextRange,
    revisited: std::collections::BTreeSet<(usize, String, TextSize)>,
}

#[derive(PartialEq, Eq)]
pub(in crate::infrastructure::mypy) struct LoopState(Vec<ScopeState>);

#[derive(PartialEq, Eq)]
struct ScopeState {
    kind: Kind,
    module_exports: Option<Arc<module_exports::ExportState>>,
    bindings: BTreeMap<String, Binding>,
    direct_functions: std::collections::BTreeSet<TextSize>,
    raw_functions: BTreeMap<TextSize, Arc<Signature>>,
    walrus_assigned: BTreeMap<String, bool>,
    redirects: BTreeMap<String, usize>,
    variables: types::Variables,
    bound_variables: std::collections::BTreeSet<Type>,
    generator: Option<(Type, bool)>,
    super_receiver: Option<(u32, Type)>,
    ordinary_assignments: BTreeMap<TextSize, bool>,
    first_unbound_assignments: BTreeMap<String, TextSize>,
    comprehension_owner: Option<usize>,
    redefinitions: std::collections::BTreeSet<String>,
    overload_aliases: std::collections::BTreeSet<String>,
    imported_names: std::collections::BTreeSet<String>,
    imported_functions: BTreeMap<String, imports::ImportedFunction>,
    import_admission: Option<imports::ImportAdmission>,
}

impl Scopes {
    pub(in crate::infrastructure::mypy) fn loop_binding_owner(&self, name: &str) -> usize {
        self.binding_scope(name)
    }

    pub(in crate::infrastructure::mypy) fn loop_namespace(&self) -> &str {
        &self.namespace
    }

    pub(in crate::infrastructure::mypy) fn begin_loop_replay(&mut self, range: TextRange) {
        self.loop_replays.push(ReplayWindow {
            depth: self.stack.len(),
            range,
            revisited: Default::default(),
        });
    }

    pub(in crate::infrastructure::mypy) fn repeat_loop_replay(&mut self) {
        let window = self.loop_replays.last_mut().expect("active loop replay");
        window.revisited = self
            .stack
            .iter()
            .take(window.depth)
            .enumerate()
            .flat_map(|(owner, scope)| {
                scope.bindings.iter().filter_map(move |(name, binding)| {
                    binding.infer_from_assignment.then_some((
                        owner,
                        name.clone(),
                        binding.first_definition,
                    ))
                })
            })
            .collect();
    }

    pub(in crate::infrastructure::mypy) fn end_loop_replay(&mut self) {
        self.loop_replays.pop().expect("active loop replay");
    }

    pub(in crate::infrastructure::mypy) fn replayed_loop_definition(
        &self,
        name: &str,
        start: TextSize,
    ) -> bool {
        let owner = self.binding_scope(name);
        self.loop_replays.iter().any(|window| {
            owner < window.depth
                && window.range.contains(start)
                && window.revisited.contains(&(owner, name.to_owned(), start))
        })
    }

    pub(in crate::infrastructure::mypy) fn loop_state(&self) -> LoopState {
        LoopState(
            self.stack
                .iter()
                .map(|scope| ScopeState {
                    kind: scope.kind,
                    module_exports: scope
                        .module_exports
                        .as_ref()
                        .map(|exports| exports.state.clone()),
                    bindings: scope.bindings.clone(),
                    direct_functions: scope.direct_function_starts.clone(),
                    raw_functions: scope.raw_functions.clone(),
                    walrus_assigned: scope.walrus_assigned.clone(),
                    redirects: scope.redirects.clone(),
                    variables: scope.variables.clone(),
                    bound_variables: scope.bound_variables.clone(),
                    generator: scope.generator,
                    super_receiver: scope.super_receiver,
                    ordinary_assignments: scope.ordinary_assignment_starts.clone(),
                    first_unbound_assignments: scope.first_unbound_local_assignments.clone(),
                    comprehension_owner: scope.comprehension_owner,
                    redefinitions: scope.redefinitions.clone(),
                    overload_aliases: scope.overload_aliases.clone(),
                    imported_names: scope.imported_names.clone(),
                    imported_functions: scope.imported_functions.clone(),
                    import_admission: scope.import_admission.clone(),
                })
                .collect(),
        )
    }
}
