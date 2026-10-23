//! Mypy's finite string-export events are finalized after module declarations.
use super::super::modules::{Export, Exports, Lookup};
use super::{Kind, Scopes};
use crate::infrastructure::mypy::{
    classes::{Abstractness, MethodKind},
    signatures::Signature,
    types::{Node, Type},
};
use mamba_language::application::quality_syntax::{python_ast::Stmt, PythonTarget};
use ruff_text_size::TextRange;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

mod collect;

#[derive(Clone, Default, PartialEq, Eq)]
pub(in crate::infrastructure::mypy) struct ExportState {
    pub has_all: bool,
    pub names: BTreeSet<String>,
    pub globals: BTreeSet<String>,
    pub gaps: Vec<(TextRange, String)>,
    // Immutable values from the final class registry, never an intermediate
    // raw-to-decorated change flag from one refresh pass.
    members: BTreeMap<u32, PublishedMembers>,
}

#[derive(Clone, PartialEq, Eq)]
struct PublishedMembers {
    methods: BTreeMap<String, PublishedMethod>,
    fields: BTreeMap<String, PublishedField>,
}

#[derive(Clone, PartialEq, Eq)]
struct PublishedField {
    ty: Option<Type>,
    needs_annotation: bool,
    class_access: bool,
    readonly: bool,
    final_attribute: bool,
    class_var: bool,
    abstract_field: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct PublishedMethod {
    signature: Arc<Signature>,
    kind: MethodKind,
    setter: Option<Arc<Signature>>,
    final_method: bool,
    abstractness: Abstractness,
}

impl ExportState {
    pub(in crate::infrastructure::mypy) fn members_changed(&self, previous: &Self) -> bool {
        self.members != previous.members
    }

    pub(super) fn finalize(&self, name: &str, export: &mut Export) {
        // Incomplete state is not a private-symbol assertion. Public consumers
        // reject it and report the source gap through ToolReport.remaining.
        if !self.gaps.is_empty() || !self.has_all {
            return;
        }
        export.explicit = self.names.contains(name);
        if export.explicit {
            export.module_hidden = Some(false);
        }
    }
}

#[derive(Clone)]
pub(super) struct ModuleExports {
    plan: Arc<Plan>,
    pub state: Arc<ExportState>,
}

struct Plan {
    globals: BTreeSet<String>,
    events: Vec<Event>,
    gaps: Vec<(TextRange, String)>,
}

enum Event {
    Add(Vec<String>),
    Remove(String),
    CopyAll(Stmt),
    Star(Stmt),
}

impl ModuleExports {
    pub(super) fn new(body: &[Stmt], target: PythonTarget) -> Self {
        let plan = Arc::new(collect::plan(body, target));
        let state = Arc::new(plan.evaluate(None));
        Self { plan, state }
    }

    pub(super) fn refresh(&mut self, lookup: Option<&Lookup>) {
        let state = self.plan.evaluate(lookup);
        if self.state.as_ref() != &state {
            self.state = Arc::new(state);
        }
    }
}

impl Plan {
    fn evaluate(&self, lookup: Option<&Lookup>) -> ExportState {
        let mut state = ExportState {
            has_all: self.globals.contains("__all__"),
            globals: self.globals.clone(),
            gaps: self.gaps.clone(),
            ..ExportState::default()
        };
        for event in &self.events {
            let Event::Star(statement) = event else {
                continue;
            };
            // This seed is provisional. Modules::build reevaluates it with a
            // graph lookup before exposing a source's final export state.
            let Some(lookup) = lookup else { continue };
            let import = lookup.imports(std::slice::from_ref(statement)).remove(0);
            match lookup.source_public_names(&import.module) {
                Ok(Some((has_all, names))) => {
                    for name in names {
                        if !name.starts_with('_') || has_all {
                            state.globals.insert(name);
                        }
                    }
                    state.has_all |= state.globals.contains("__all__");
                }
                Ok(None) => {}
                Err(reason) => state.gaps.push((import.range, reason)),
            }
        }
        for event in &self.events {
            match event {
                Event::Add(names) if state.has_all => state.names.extend(names.iter().cloned()),
                Event::Remove(name) if state.has_all => {
                    state.names.remove(name);
                }
                Event::CopyAll(statement) => {
                    let Some(lookup) = lookup else { continue };
                    let import = lookup
                        .imports(std::slice::from_ref(statement))
                        .into_iter()
                        .find(|import| {
                            import.member.as_deref() == Some("__all__")
                                && import.alias.as_deref() == Some("__all__")
                        })
                        .expect("captured __all__ alias import");
                    match lookup.source_public_names(&import.module) {
                        Ok(Some((_, names))) => state.names = names,
                        Ok(None) => {}
                        Err(reason) => state.gaps.push((import.range, reason)),
                    }
                }
                _ => {}
            }
        }
        state.gaps.sort_by(|left, right| {
            (left.0.start(), left.0.end(), &left.1).cmp(&(right.0.start(), right.0.end(), &right.1))
        });
        state.gaps.dedup();
        state
    }
}

impl Scopes {
    pub(in crate::infrastructure::mypy) fn set_module_export_plan(&mut self, body: &[Stmt]) {
        if self.current().kind == Kind::Module {
            let target = self.context.module_target(&self.namespace);
            self.stack.last_mut().unwrap().module_exports = Some(ModuleExports::new(body, target));
            self.install_original_builtin_roots(body);
        }
    }

    pub(super) fn refresh_module_export_state(&mut self) {
        let lookup = self.modules.clone();
        if let Some(exports) = &mut self.stack.last_mut().unwrap().module_exports {
            exports.refresh(lookup.as_ref());
        }
    }

    pub(in crate::infrastructure::mypy) fn module_export_state(&self) -> Arc<ExportState> {
        let mut state = self
            .current()
            .module_exports
            .as_ref()
            .map(|exports| exports.state.as_ref().clone())
            .unwrap_or_default();
        if self.current().kind == Kind::Module && self.modules.is_some() {
            // Top-level source classes have real roots in these live maps.
            // Imported classes are excluded by their owning namespace.
            let ids: BTreeSet<_> = self
                .current()
                .variables
                .values()
                .copied()
                .chain(
                    self.current()
                        .bindings
                        .values()
                        .filter_map(|binding| binding.ty),
                )
                .filter_map(|ty| match self.context.node(ty).as_deref() {
                    Some(Node::Class(id) | Node::Instance(id) | Node::AppliedClass { id, .. }) => {
                        Some(*id)
                    }
                    _ => None,
                })
                .collect();
            for id in ids {
                let class = self.context.class(id);
                if class.namespace != self.namespace {
                    continue;
                }
                let methods = class
                    .methods
                    .iter()
                    .map(|(name, method)| {
                        (
                            name.clone(),
                            PublishedMethod {
                                signature: method.signature.clone(),
                                kind: method.kind,
                                setter: method.setter.clone(),
                                final_method: method.final_method,
                                abstractness: method.abstractness,
                            },
                        )
                    })
                    .collect();
                let fields = class
                    .fields
                    .iter()
                    .map(|(name, field)| {
                        (
                            name.clone(),
                            PublishedField {
                                ty: field.ty,
                                needs_annotation: field.needs_annotation,
                                class_access: field.class_access,
                                readonly: field.readonly,
                                final_attribute: field.final_attribute,
                                class_var: field.class_var,
                                abstract_field: field.abstract_field,
                            },
                        )
                    })
                    .collect();
                state
                    .members
                    .insert(id, PublishedMembers { methods, fields });
            }
        }
        Arc::new(state)
    }

    pub(in crate::infrastructure::mypy) fn export_state_gaps(&self) -> Vec<String> {
        self.modules
            .as_ref()
            .map(Lookup::export_state_gaps)
            .unwrap_or_default()
    }

    pub(super) fn finalize_module_exports(&self, exports: &mut Exports) {
        if let Some(module) = &self.current().module_exports {
            for (name, export) in exports {
                module.state.finalize(name, export);
            }
        }
    }
}
