//! Dependency snapshots supply inference facts without increasing checked files.
mod duplicate;
pub(super) mod embedded;
mod imports;
mod local;
mod output;
mod search;
mod shadow;
use crate::{
    domain::source::{QualityWorkspace, SourceSnapshot},
    infrastructure::{encoding, metadata, project::CachedDocument, project::ProjectIndex},
};
use mamba_language::application::quality_syntax::{
    self, python_ast::visitor::Visitor, SyntaxDocument,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::Arc,
};

pub(super) struct Loaded {
    pub workspace: QualityWorkspace,
    names: BTreeMap<usize, String>,
    pub untyped: BTreeSet<(usize, String)>,
    pub imported: BTreeSet<(usize, String)>,
    pub read_errors: Vec<String>,
    pub local: BTreeSet<usize>,
    pub parse_order: Vec<usize>,
    pub blocking: Option<crate::domain::model::QualityDiagnostic>,
    documents: Vec<CachedDocument>,
    output: output::Metadata,
}

pub(super) fn core_shadow(
    workspace: &QualityWorkspace,
) -> Option<crate::domain::model::QualityDiagnostic> {
    shadow::find(workspace)
}

impl Loaded {
    pub(super) fn output_modules(&self, index: &ProjectIndex<'_>) -> Vec<output::Module> {
        self.output.modules(index, &self.workspace)
    }

    fn ensure_documents(&mut self) {
        while self.documents.len() < self.workspace.sources.len() {
            let id = self.documents.len();
            let source = &self.workspace.sources[id];
            let document = if source.decoding_issue.is_none() {
                Some(
                    quality_syntax::parse_data_for_type_inference(
                        &source.text,
                        crate::infrastructure::native::target(&self.workspace, source),
                        source.is_stub,
                    )
                    .map(Arc::new),
                )
            } else {
                None
            };
            self.documents.push(document);
        }
    }

    pub(super) fn document(&self, id: usize) -> Option<SyntaxDocument<'_>> {
        let source = &self.workspace.sources[id];
        let data = self.documents.get(id)?.as_ref()?.as_ref().ok()?;
        Some(SyntaxDocument::from_data(&source.text, Arc::clone(data)))
    }

    pub(super) fn index(&self) -> ProjectIndex<'_> {
        let mut index =
            ProjectIndex::for_type_inference_with_documents(&self.workspace, &self.documents);
        if self.names.is_empty() {
            return index;
        }
        for (id, name) in &self.names {
            index.modules[*id].name = name.clone();
        }
        index.names.clear();
        index.namespaces.clear();
        for (id, module) in index.modules.iter().enumerate() {
            let key = (module.project, module.name.clone());
            if self.workspace.sources[id].is_stub || !index.names.contains_key(&key) {
                index.names.insert(key, id);
            }
            index
                .namespaces
                .insert((module.project, module.name.clone()));
            for (offset, _) in module.name.match_indices('.') {
                index
                    .namespaces
                    .insert((module.project, module.name[..offset].into()));
            }
        }
        index.dependents = vec![BTreeSet::new(); index.modules.len()];
        for (id, module) in index.modules.iter().enumerate() {
            if let Some(document) = &module.document {
                let mut collector =
                    imports::Imports::new(&module.name, module.is_package, document.target);
                collector.visit_body(&document.parsed.syntax().body);
                for name in collector.names {
                    if let Some(dependency) = index.names.get(&(module.project, name)) {
                        index.dependents[*dependency].insert(id);
                    }
                }
            }
        }
        index
    }
}

pub(super) fn load(workspace: QualityWorkspace, names: BTreeMap<usize, String>) -> Loaded {
    let source_count = workspace.sources.len();
    let roots = local::roots(&workspace);
    let mut result = Loaded {
        workspace,
        names,
        untyped: BTreeSet::new(),
        imported: BTreeSet::new(),
        read_errors: Vec::new(),
        local: BTreeSet::new(),
        parse_order: (0..source_count).collect(),
        blocking: None,
        documents: Vec::new(),
        output: output::Metadata::default(),
    };
    let mut output = output::Metadata::default();
    let mut errors = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut scanned = 0;
    let mut catalog = local::Catalog::default();
    loop {
        result.ensure_documents();
        let index = result.index();
        let mut pending = Vec::new();
        // Record owners before reading pending sources. Two names can resolve
        // to one path in the same import batch.
        let mut owners: BTreeMap<_, _> = result
            .workspace
            .sources
            .iter()
            .enumerate()
            .map(|(id, source)| {
                (
                    (source.project, source.path.clone()),
                    index.modules[id].name.clone(),
                )
            })
            .collect();
        let mut duplicate_source = None;
        let mut untyped = BTreeSet::new();
        let mut imported = BTreeSet::new();
        'scan: for (id, source) in result.workspace.sources.iter().enumerate().skip(scanned) {
            let module = &index.modules[id];
            let Some(document) = &module.document else {
                if embedded::is_source(source) {
                    errors.insert(format!(
                        "Embedded stub {} requires valid target-version syntax for inference",
                        module.name
                    ));
                }
                continue;
            };
            let mut imports =
                imports::Imports::for_output(&module.name, module.is_package, document.target);
            imports.visit_body(&document.parsed.syntax().body);
            if let Some(entries) = imports.entries {
                output.record(id, entries);
            }
            for name in imports.names {
                imported.insert((source.project, name.clone()));
                if index.namespaces.contains(&(source.project, name.clone()))
                    || !visited.insert((source.project, name.clone()))
                {
                    continue;
                }
                let local = local::sources_with_catalog(
                    &roots[source.project],
                    &name,
                    &mut catalog,
                    &mut errors,
                );
                let is_local = !local.is_empty();
                if !is_local {
                    if let Some(path) = embedded::path(&name, document.target) {
                        output.module_found(source.project, &name);
                        let key = (source.project, path.clone());
                        if let std::collections::btree_map::Entry::Vacant(entry) = owners.entry(key)
                        {
                            entry.insert(name.clone());
                            pending.push((source.project, path, name, false, document.target));
                        }
                        continue;
                    }
                }
                if !is_local
                    && (super::modules::Lookup::native_pydantic_module(&name)
                        || matches!(name.as_str(), "typing_extensions" | "pydantic.mypy")
                        || embedded::available(&name, document.target))
                {
                    continue;
                }
                let found = if is_local {
                    search::Found {
                        sources: local,
                        untyped: false,
                    }
                } else {
                    search::sources(
                        &result.workspace.projects[source.project].dependencies,
                        &name,
                        &mut catalog,
                        &mut errors,
                    )
                };
                if found.untyped {
                    untyped.insert((source.project, name.clone()));
                }
                if errors.is_empty() {
                    output.observe(source.project, &name, &found.sources);
                }
                let mut found_sources = found.sources;
                found_sources.sort_by_key(|(_, module)| module != &name);
                for (path, name) in found_sources {
                    let key = (module.project, path.clone());
                    if let Some(previous) = owners.get(&key) {
                        if previous != &name {
                            duplicate_source = Some(duplicate::diagnostic(path, previous, &name));
                            break 'scan;
                        }
                        continue;
                    }
                    owners.insert(key, name.clone());
                    pending.push((module.project, path, name, is_local, document.target));
                }
            }
        }
        scanned = result.workspace.sources.len();
        if duplicate_source.is_none() && pending.is_empty() && errors.is_empty() {
            output.complete_child_absences(
                &index,
                &result.workspace,
                &roots,
                &mut catalog,
                &mut errors,
            );
        }
        drop(index);
        if let Some(diagnostic) = duplicate_source {
            // Root syntax is checked by the caller before this blocker. Stop
            // here so unrelated pending decode errors cannot mask ownership.
            result.blocking = Some(diagnostic);
            break;
        }
        result.untyped.extend(untyped);
        result.imported.extend(imported);
        if pending.is_empty() {
            break;
        }
        for (project, path, name, is_local, target) in pending {
            if !is_local {
                if let Some(source) = embedded::source(project, &name, target) {
                    let id = result.workspace.sources.len();
                    result.names.insert(id, name);
                    // Embedded data supplies declarations only. Do not put its
                    // syntax or semantic diagnostics into the user-file stream.
                    result.workspace.sources.push(source);
                    continue;
                }
            }
            match fs::read(&path) {
                Ok(bytes) => match encoding::decode(&path, &bytes) {
                    Ok((text, encoding)) => {
                        let id = result.workspace.sources.len();
                        result.names.insert(id, name);
                        result.parse_order.push(id);
                        if is_local {
                            result.local.insert(id);
                        }
                        result.workspace.sources.push(SourceSnapshot {
                            is_stub: path.extension().is_some_and(|extension| extension == "pyi"),
                            path,
                            project,
                            sha256: metadata::sha256(&bytes),
                            bytes,
                            text,
                            encoding,
                            decoding_issue: None,
                        });
                    }
                    Err(error) => {
                        errors.insert(format!("{}: {}", path.display(), error.message));
                    }
                },
                Err(error) => {
                    errors.insert(format!("{}: {error}", path.display()));
                }
            }
        }
    }
    result.output = output;
    result.read_errors = errors.into_iter().collect();
    result
}
