//! Import facts for output ordering; this does not change loading or checking.
use crate::domain::source::QualityWorkspace;
use crate::infrastructure::project::ProjectIndex;
use mamba_language::application::quality_syntax::{
    python_ast::{BoolOp, Expr, UnaryOp},
    PythonTarget,
};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

pub(super) struct Import {
    pub module: String,
    pub members: Option<Vec<String>>,
    pub in_function: bool,
    pub mypy_only: Option<bool>,
}

impl Import {
    fn priority(&self, top_level: u8) -> Option<u8> {
        if self.in_function {
            Some(20)
        } else {
            self.mypy_only.map(|only| if only { 25 } else { top_level })
        }
    }
}

pub(in super::super) struct Edge {
    pub name: String,
    pub target: Option<usize>,
    pub is_module: Option<bool>,
    pub priority: Option<u8>,
}

pub(in super::super) struct Module {
    pub source: usize,
    pub project: usize,
    pub name: String,
    // Native first-admission order, not a claim of reference admission parity.
    pub encounter: usize,
    // None means that this source could not supply a valid import walk.
    pub dependencies: Option<Vec<Edge>>,
}

#[derive(Default)]
pub(super) struct Metadata {
    imports: BTreeMap<usize, Vec<Import>>,
    found: BTreeMap<(usize, String), bool>,
    absent_children: BTreeMap<(usize, String), AbsentChild>,
}

// Owned by this Loaded request, never shared by fullname across requests.
struct AbsentChild {
    parent_source: usize,
    parent_sha256: String,
    target: PythonTarget,
}

impl Metadata {
    pub(super) fn complete_child_absences(
        &mut self,
        index: &ProjectIndex<'_>,
        workspace: &QualityWorkspace,
        roots: &[Vec<PathBuf>],
        catalog: &mut super::local::Catalog,
        errors: &mut BTreeSet<String>,
    ) {
        let lookup = std::cell::OnceCell::new();
        let mut absent = BTreeMap::new();
        for (&owner, imports) in &self.imports {
            let module = &index.modules[owner];
            let Some(document) = module
                .document
                .as_ref()
                .filter(|document| document.is_valid())
            else {
                continue;
            };
            for import in imports {
                let Some(members) = &import.members else {
                    continue;
                };
                let Some(&parent) = index.names.get(&(module.project, import.module.clone()))
                else {
                    continue;
                };
                let parent_module = &index.modules[parent];
                let parent_source = &workspace.sources[parent];
                let Some(parent_document) = parent_module.document.as_ref() else {
                    continue;
                };
                if !parent_module.is_package
                    || !parent_document.is_valid()
                    || parent_document.target != document.target
                    || !super::embedded::selects_declarations(&import.module, parent_source)
                    || super::embedded::path(&import.module, document.target).as_ref()
                        != Some(&parent_source.path)
                    || crate::infrastructure::embedded::module(&import.module)
                        != Some(parent_source.text.as_str())
                {
                    continue;
                }
                for member in members.iter().filter(|member| member.as_str() != "*") {
                    let name = format!("{}.{member}", import.module);
                    if self.is_module(index, module.project, &name).is_some() {
                        continue;
                    }
                    let Some(lookup) = lookup.get_or_init(super::search::ChildAbsence::new) else {
                        continue;
                    };
                    if lookup.absent(
                        &name,
                        document.target,
                        &roots[module.project],
                        &workspace.projects[module.project].dependencies,
                        catalog,
                        errors,
                    ) {
                        absent.insert(
                            (owner, name),
                            AbsentChild {
                                parent_source: parent,
                                parent_sha256: parent_source.sha256.clone(),
                                target: document.target,
                            },
                        );
                    }
                }
            }
        }
        if errors.is_empty() {
            self.absent_children = absent;
        }
    }

    fn absent_child(
        &self,
        index: &ProjectIndex<'_>,
        workspace: &QualityWorkspace,
        owner: usize,
        parent: &str,
        name: &str,
    ) -> bool {
        let Some(receipt) = self.absent_children.get(&(owner, name.to_owned())) else {
            return false;
        };
        let module = &index.modules[owner];
        index.names.get(&(module.project, parent.to_owned())) == Some(&receipt.parent_source)
            && workspace.sources[receipt.parent_source].sha256 == receipt.parent_sha256
            && module
                .document
                .as_ref()
                .is_some_and(|document| document.target == receipt.target)
            && index.modules[receipt.parent_source]
                .document
                .as_ref()
                .is_some_and(|document| document.target == receipt.target)
    }

    pub(super) fn record(&mut self, source: usize, imports: Vec<Import>) {
        self.imports.insert(source, imports);
    }

    pub(super) fn module_found(&mut self, project: usize, name: &str) {
        self.found.insert((project, name.into()), true);
    }

    pub(super) fn observe(&mut self, project: usize, name: &str, sources: &[(PathBuf, String)]) {
        if sources.iter().any(|(_, module)| module == name) {
            self.module_found(project, name);
        } else if sources.iter().any(|(path, module)| {
            name.strip_prefix(module.as_str())
                .is_some_and(|tail| tail.starts_with('.'))
                && path
                    .file_name()
                    .is_some_and(|file| file != "__init__.py" && file != "__init__.pyi")
        }) {
            // The existing search stops at a non-package parent file.
            self.found.insert((project, name.into()), false);
        }
        // Parent packages alone do not rule out an unrepresented namespace child.
    }

    fn is_module(&self, index: &ProjectIndex<'_>, project: usize, name: &str) -> Option<bool> {
        let key = (project, name.to_owned());
        if index.names.contains_key(&key) {
            Some(true)
        } else if let Some(found) = self.found.get(&key) {
            Some(*found)
        } else {
            index.namespaces.contains(&key).then_some(true)
        }
    }

    pub(super) fn modules(
        &self,
        index: &ProjectIndex<'_>,
        workspace: &QualityWorkspace,
    ) -> Vec<Module> {
        index
            .modules
            .iter()
            .enumerate()
            .map(|(source, module)| Module {
                source,
                project: module.project,
                name: module.name.clone(),
                encounter: source,
                dependencies: self.imports.get(&source).map(|imports| {
                    self.edges(
                        index,
                        workspace,
                        source,
                        module.project,
                        &module.name,
                        imports,
                    )
                }),
            })
            .collect()
    }

    fn edges(
        &self,
        index: &ProjectIndex<'_>,
        workspace: &QualityWorkspace,
        source: usize,
        project: usize,
        owner: &str,
        imports: &[Import],
    ) -> Vec<Edge> {
        let mut entries = Vec::new();
        for import in imports {
            if let Some(members) = &import.members {
                let mut all_submodules = Some(true);
                for member in members {
                    if member == "*" {
                        all_submodules = Some(false);
                        continue;
                    }
                    let name = format!("{}.{member}", import.module);
                    match self.is_module(index, project, &name) {
                        Some(true) => entries.push((name, import.priority(10))),
                        Some(false) => all_submodules = Some(false),
                        None if self.absent_child(
                            index,
                            workspace,
                            source,
                            &import.module,
                            &name,
                        ) =>
                        {
                            all_submodules = Some(false);
                        }
                        None => {
                            if all_submodules != Some(false) {
                                all_submodules = None;
                            }
                            entries.push((name, None));
                        }
                    }
                }
                let priority = match all_submodules {
                    Some(true) => import.priority(20),
                    Some(false) => import.priority(5),
                    None if import.in_function => Some(20),
                    None if import.mypy_only == Some(true) => Some(25),
                    None => None,
                };
                entries.push((import.module.clone(), priority));
            } else {
                entries.push((import.module.clone(), import.priority(10)));
                for (offset, _) in import.module.match_indices('.') {
                    entries.push((import.module[..offset].into(), import.priority(20)));
                }
            }
        }
        // Stable depth order matches all_imported_modules_in_file; it is not a file sort.
        entries.sort_by_key(|(name, _)| Reverse(name.matches('.').count()));
        let mut positions = BTreeMap::new();
        let mut result: Vec<Edge> = Vec::new();
        for (name, priority) in entries {
            if name == owner {
                continue;
            }
            if let Some(&position) = positions.get(&name) {
                let edge: &mut Edge = &mut result[position];
                edge.priority = edge
                    .priority
                    .zip(priority)
                    .map(|(left, right)| left.min(right));
            } else {
                positions.insert(name.clone(), result.len());
                result.push(Edge {
                    target: index.names.get(&(project, name.clone())).copied(),
                    is_module: self.is_module(index, project, &name),
                    name,
                    priority,
                });
            }
        }
        if owner != "builtins" && !positions.contains_key("builtins") {
            result.push(Edge {
                name: "builtins".into(),
                target: index.names.get(&(project, "builtins".into())).copied(),
                is_module: self.is_module(index, project, "builtins"),
                priority: Some(5),
            });
        }
        result
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Truth {
    Always(bool),
    Mypy(bool),
    Unknown,
}

// Only this bit affects priority. Unsupported truth mixed with a Mypy marker stays unknown.
pub(super) fn mypy_only(expression: &Expr, target: PythonTarget) -> Option<bool> {
    if !has_marker(expression) {
        return Some(false);
    }
    condition(expression, target).map(|truth| truth == Truth::Mypy(true))
}

fn has_marker(expression: &Expr) -> bool {
    match expression {
        Expr::Name(name) => matches!(name.id.as_str(), "MYPY" | "TYPE_CHECKING"),
        Expr::Attribute(attribute) => matches!(attribute.attr.as_str(), "MYPY" | "TYPE_CHECKING"),
        Expr::UnaryOp(unary) if unary.op == UnaryOp::Not => has_marker(&unary.operand),
        Expr::BoolOp(boolean) => boolean.values.iter().any(has_marker),
        _ => false,
    }
}

fn condition(expression: &Expr, target: PythonTarget) -> Option<Truth> {
    match expression {
        Expr::Name(name) => Some(named_truth(name.id.as_str())),
        Expr::Attribute(attribute) => Some(named_truth(attribute.attr.as_str())),
        Expr::UnaryOp(unary) if unary.op == UnaryOp::Not => {
            condition(&unary.operand, target).map(|truth| match truth {
                Truth::Always(value) => Truth::Always(!value),
                Truth::Mypy(value) => Truth::Mypy(!value),
                Truth::Unknown => Truth::Unknown,
            })
        }
        Expr::BoolOp(boolean) => {
            let mut values = boolean.values.iter();
            let mut result = condition(values.next()?, target)?;
            for value in values {
                result = combine(result, condition(value, target)?, boolean.op);
            }
            Some(result)
        }
        _ => super::super::classes::statements::truth(expression, target).map(Truth::Always),
    }
}

fn named_truth(name: &str) -> Truth {
    match name {
        "PY2" => Truth::Always(false),
        "PY3" => Truth::Always(true),
        "MYPY" | "TYPE_CHECKING" => Truth::Mypy(true),
        _ => Truth::Unknown,
    }
}

fn combine(left: Truth, right: Truth, op: BoolOp) -> Truth {
    let values = [left, right];
    if op == BoolOp::Or {
        if values.contains(&Truth::Always(true)) {
            Truth::Always(true)
        } else if values.contains(&Truth::Mypy(true)) {
            Truth::Mypy(true)
        } else if left == Truth::Mypy(false) && right == left {
            left
        } else if values
            .iter()
            .all(|value| matches!(value, Truth::Always(false) | Truth::Mypy(false)))
        {
            Truth::Always(false)
        } else {
            Truth::Unknown
        }
    } else if values.contains(&Truth::Always(false)) {
        Truth::Always(false)
    } else if values.contains(&Truth::Mypy(false)) {
        Truth::Mypy(false)
    } else if left == Truth::Always(true) && right == left {
        left
    } else if values
        .iter()
        .all(|value| matches!(value, Truth::Always(true) | Truth::Mypy(true)))
    {
        Truth::Mypy(true)
    } else {
        Truth::Unknown
    }
}
