//! Pylint resolves project attributes without mypy's strict reexport policy.
mod class_stores;
mod embedded_bindings;
mod function_names;
use super::super::{
    embedded,
    project::{
        symbols::{DefinitionKind, Symbols},
        ProjectIndex,
    },
};
use crate::domain::source::PythonVersion;
use mamba_language::application::quality_syntax;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

pub(super) struct Modules {
    extensions: super::native_extensions::Extensions,
    class_stores: RefCell<super::build_attributes::Stores>,
    import_failures: BTreeMap<(usize, String), String>,
    members: BTreeMap<(usize, String), BTreeSet<String>>,
    namespaces: BTreeSet<(usize, String)>,
    builtins: BTreeSet<String>,
    builtin_modules: BTreeSet<String>,
    functions: BTreeSet<String>,
    embedded_bindings: embedded_bindings::EmbeddedBindings,
    builtin_classes: BTreeMap<String, std::rc::Rc<super::classes::Class>>,
    stdlib: RefCell<BTreeMap<String, BTreeSet<String>>>,
    target: quality_syntax::PythonTarget,
    callables: BTreeMap<(usize, String), BTreeMap<String, super::callables::Target>>,
    logging_targets: RefCell<Option<BTreeMap<String, super::callables::Target>>>,
    abstract_targets: RefCell<BTreeMap<String, BTreeMap<String, super::callables::Target>>>,
    frozen_targets: RefCell<BTreeMap<String, BTreeMap<String, super::callables::Target>>>,
    sources: BTreeMap<(usize, String), String>,
    source_indices: BTreeMap<(usize, String), usize>,
    expressions: BTreeMap<
        (usize, String),
        BTreeMap<String, mamba_language::application::quality_syntax::python_ast::Expr>,
    >,
    unpacking: BTreeMap<(usize, String), super::unpacking_bindings::Bindings>,
}

impl Modules {
    pub(super) fn new(
        index: &ProjectIndex<'_>,
        workspace: &crate::domain::source::QualityWorkspace,
        project: usize,
    ) -> Self {
        let version = workspace.projects[project].python_version;
        let selected = || {
            index.modules.iter().enumerate().filter(|(id, module)| {
                index.names.get(&(module.project, module.name.clone())) == Some(id)
            })
        };
        let members = selected()
            .filter_map(|(_, module)| {
                let document = module.document.as_ref()?;
                let symbols = Symbols::collect(
                    &document.parsed.syntax().body,
                    &module.name,
                    module.is_package,
                );
                Some((
                    (module.project, module.name.clone()),
                    symbols.bindings.into_keys().collect(),
                ))
            })
            .collect();
        let callables = selected()
            .filter_map(|(_, module)| {
                Some((
                    (module.project, module.name.clone()),
                    super::callables::collect(
                        &module.document.as_ref()?.parsed.syntax().body,
                        &module.name,
                        module.is_package,
                    ),
                ))
            })
            .collect();
        let names: Vec<BTreeSet<String>> = serde_json::from_str(super::builtins::NAMES)
            .expect("locked CPython built-in name data");
        let builtin_modules: Vec<BTreeSet<String>> =
            serde_json::from_str(include_str!("../../../assets/pylint-builtin-modules.json"))
                .expect("locked CPython built-in module data");
        let target = match version {
            PythonVersion::Py312 => quality_syntax::PythonTarget::Py312,
            PythonVersion::Py313 => quality_syntax::PythonTarget::Py313,
            PythonVersion::Py314 => quality_syntax::PythonTarget::Py314,
        };
        let builtin_document = embedded::module("builtins")
            .and_then(|source| quality_syntax::parse(source, target, true).ok());
        let functions = builtin_document
            .as_ref()
            .map(|document| {
                Symbols::collect(&document.parsed.syntax().body, "builtins", false)
                    .bindings
                    .into_iter()
                    .filter_map(|(name, definition)| {
                        matches!(definition.kind, DefinitionKind::Function).then_some(name)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let builtin_classes = builtin_document
            .as_ref()
            .map(|document| {
                super::callables::collect(&document.parsed.syntax().body, "builtins", false)
                    .into_iter()
                    .filter_map(|(name, target)| match target {
                        super::callables::Target::Class(class) => Some((name, class)),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            class_stores: RefCell::default(),
            embedded_bindings: embedded_bindings::EmbeddedBindings::default(),
            import_failures: class_stores::import_failures(index, workspace, target),
            extensions: super::native_extensions::Extensions::new(index, workspace, project),
            unpacking: selected()
                .filter_map(|(_, module)| {
                    Some((
                        (module.project, module.name.clone()),
                        super::unpacking_bindings::collect(
                            &module.document.as_ref()?.parsed.syntax().body,
                        ),
                    ))
                })
                .collect(),
            expressions: selected()
                .filter_map(|(_, module)| {
                    Some((
                        (module.project, module.name.clone()),
                        super::source_bindings::collect(
                            &module.document.as_ref()?.parsed.syntax().body,
                        ),
                    ))
                })
                .collect(),
            source_indices: selected()
                .map(|(id, module)| ((module.project, module.name.clone()), id))
                .collect(),
            sources: selected()
                .filter_map(|(_, module)| {
                    Some((
                        (module.project, module.name.clone()),
                        module.document.as_ref()?.source.to_string(),
                    ))
                })
                .collect(),
            members,
            namespaces: index.namespaces.clone(),
            builtins: names
                .into_iter()
                .nth(usize::from(version.minor() - 12))
                .unwrap(),
            builtin_modules: builtin_modules
                .into_iter()
                .nth(usize::from(version.minor() - 12))
                .unwrap(),
            functions,
            builtin_classes,
            target,
            stdlib: RefCell::new(BTreeMap::new()),
            callables,
            logging_targets: RefCell::new(None),
            abstract_targets: RefCell::new(BTreeMap::new()),
            frozen_targets: RefCell::new(BTreeMap::new()),
        }
    }

    pub(super) fn unpacking_expression(
        &self,
        project: usize,
        module: &str,
        name: &str,
    ) -> Option<mamba_language::application::quality_syntax::python_ast::Expr> {
        self.unpacking
            .get(&(project, module.into()))?
            .expression(name)
    }
    pub(super) fn source(&self, project: usize, module: &str) -> Option<&str> {
        self.sources
            .get(&(project, module.into()))
            .map(String::as_str)
            .or_else(|| embedded::module(module))
    }
    pub(super) fn source_index(&self, project: usize, module: &str) -> Option<usize> {
        if self.native(module) {
            return None;
        }
        self.source_indices.get(&(project, module.into())).copied()
    }
    pub(super) fn expression(
        &self,
        project: usize,
        module: &str,
        name: &str,
    ) -> Option<mamba_language::application::quality_syntax::python_ast::Expr> {
        self.expressions
            .get(&(project, module.into()))?
            .get(name)
            .cloned()
    }
    pub(super) fn builtin_class(&self, name: &str) -> Option<std::rc::Rc<super::classes::Class>> {
        self.builtin_classes.get(name).cloned()
    }

    pub(super) fn native(&self, name: &str) -> bool {
        self.extensions.contains(name)
    }

    pub(super) fn project_module(&self, project: usize, name: &str) -> bool {
        self.namespaces.contains(&(project, name.into()))
    }
    pub(super) fn project_native(&self, name: &str) -> bool {
        self.extensions.first_party(name)
    }
    pub(super) fn exists(&self, project: usize, name: &str) -> bool {
        if self.import_failure(project, name).is_some() {
            return false;
        }
        self.namespaces.contains(&(project, name.into()))
            || (embedded::module(name).is_some()
                && super::stdlib_versions::available(name, self.target))
            || self.extensions.exists(name)
    }

    pub(super) fn member(&self, project: usize, module: &str, name: &str) -> bool {
        if self.native(module) {
            return super::native_extensions::scope_attribute(name);
        }
        self.members(project, module)
            .is_some_and(|members| members.contains(name))
            || self.exists(project, &format!("{module}.{name}"))
    }

    pub(super) fn members(&self, project: usize, module: &str) -> Option<BTreeSet<String>> {
        if self.native(module) {
            return Some(BTreeSet::new());
        }
        if !self.builtin_modules.contains(module) {
            if let Some(members) = self.members.get(&(project, module.into())) {
                return Some(members.clone());
            }
        }
        if let Some(module) = super::stdlib_facts::module(module, self.target) {
            return Some(module.members.keys().cloned().collect());
        }
        if !self.stdlib.borrow().contains_key(module) {
            embedded::module(module)?;
            let mut members = BTreeSet::new();
            let mut visited = BTreeSet::new();
            let mut pending = vec![module.to_string()];
            while let Some(name) = pending.pop() {
                if !visited.insert(name.clone()) {
                    continue;
                }
                let Some(source) = embedded::module(&name) else {
                    continue;
                };
                let Ok(document) = quality_syntax::parse(source, self.target, true) else {
                    continue;
                };
                let symbols = Symbols::collect(&document.parsed.syntax().body, &name, false);
                for (name, definition) in symbols.bindings {
                    if let DefinitionKind::Import(import) = &definition.kind {
                        if import.member.as_deref() == Some("*") {
                            pending.push(import.module.clone());
                            continue;
                        }
                    }
                    members.insert(name);
                }
            }
            self.stdlib.borrow_mut().insert(module.into(), members);
        }
        self.stdlib.borrow().get(module).cloned()
    }

    pub(super) fn child(&self, project: usize, module: &str, member: &str) -> Option<String> {
        let child = format!("{module}.{member}");
        self.exists(project, &child).then_some(child)
    }

    pub(super) fn call_target(
        &self,
        project: usize,
        module: &str,
        name: &str,
    ) -> Option<super::callables::Target> {
        if self.native(module) {
            return None;
        }
        if module == "builtins" {
            return self
                .builtin_class(name)
                .map(super::callables::Target::Class);
        }
        if module == "logging" {
            if self.logging_targets.borrow().is_none() {
                *self.logging_targets.borrow_mut() =
                    Some(super::logging::stdlib::targets(self.target));
            }
            return self.logging_targets.borrow().as_ref()?.get(name).cloned();
        }
        if self.builtin_modules.contains(module) && module != "itertools" {
            return self.frozen_target(module, name);
        }
        if let Some(targets) = self.callables.get(&(project, module.into())) {
            return targets
                .get(name)
                .cloned()
                .map(|target| self.stored_class(target));
        }
        if super::stdlib_facts::module(module, self.target)
            .is_some_and(|module| !module.members.contains_key(name))
            && !self.exists(project, &format!("{module}.{name}"))
        {
            return None;
        }
        if module == "dataclasses" && !matches!(name, "dataclass" | "field" | "make_dataclass") {
            return None;
        }
        if !matches!(
            module,
            "abc"
                | "itertools"
                | "_py_abc"
                | "collections"
                | "contextlib"
                | "dataclasses"
                | "unittest"
                | "unittest.case"
                | "enum"
        ) && !(matches!(module, "typing" | "typing_extensions")
            && matches!(name, "final" | "overload" | "Protocol"))
        {
            return self.frozen_target(module, name);
        }
        if !self.abstract_targets.borrow().contains_key(module) {
            let document =
                quality_syntax::parse(embedded::module(module)?, self.target, true).ok()?;
            let mut targets = super::callables::collect(
                &document.parsed.syntax().body,
                module,
                module == "unittest",
            );
            super::stdlib_runtime::adjust_targets(module, &mut targets);
            if matches!(module, "typing" | "typing_extensions") {
                if let Some(super::callables::Target::Class(class)) = targets.get("_Protocol") {
                    let mut alias = (**class).clone();
                    alias.name = "Protocol".into();
                    targets.insert(
                        "Protocol".into(),
                        super::callables::Target::Class(std::rc::Rc::new(alias)),
                    );
                }
            }
            self.abstract_targets
                .borrow_mut()
                .insert(module.into(), targets);
        }
        let value = self
            .abstract_targets
            .borrow()
            .get(module)?
            .get(name)
            .cloned();
        value.or_else(|| self.frozen_target(module, name))
    }

    /// Return naming-only function identity without changing ordinary call inference.
    pub(super) fn naming_function(&self, project: usize, module: &str, name: &str) -> bool {
        function_names::is_function(self, project, module, name)
    }

    fn frozen_target(&self, module: &str, name: &str) -> Option<super::callables::Target> {
        if let Some(value) = self
            .frozen_targets
            .borrow()
            .get(module)
            .and_then(|values| values.get(name))
            .cloned()
        {
            return Some(value);
        }
        let value = super::stdlib_facts::call_target(module, name, self.target)?;
        self.frozen_targets
            .borrow_mut()
            .entry(module.into())
            .or_default()
            .insert(name.into(), value.clone());
        Some(value)
    }
}
