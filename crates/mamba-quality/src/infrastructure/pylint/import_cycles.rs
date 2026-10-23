//! Project import cycles retain edge exclusions and final-file diagnostics.
use super::directives;
use crate::{
    domain::{
        model::{QualityDiagnostic, QualityTool},
        source::QualityWorkspace,
    },
    infrastructure::project::{imports, ProjectIndex},
};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::{BTreeMap, BTreeSet};
struct Edges<'a> {
    module: &'a crate::infrastructure::project::Module<'a>,
    directives: Vec<directives::Directive>,
    values: Vec<String>,
    typing: bool,
}
impl<'a> Visitor<'a> for Edges<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                let line = self
                    .module
                    .document
                    .as_ref()
                    .unwrap()
                    .line_column(statement.start().to_u32())
                    .0;
                if self.typing
                    || directives::suppressed(&self.directives, "R0401", "cyclic-import", line)
                {
                    return;
                }
                for import in imports(
                    std::slice::from_ref(statement),
                    &self.module.name,
                    self.module.is_package,
                ) {
                    self.values.push(import.module.clone());
                    if let Some(member) = import.member {
                        if member != "*" {
                            self.values.push(format!("{}.{member}", import.module));
                        }
                    }
                }
            }
            Stmt::If(value) => {
                let previous = self.typing;
                self.typing |= matches!(value.test.as_ref(), Expr::Name(name) if name.id.as_str() == "TYPE_CHECKING")
                    || matches!(value.test.as_ref(), Expr::Attribute(name) if name.attr.as_str() == "TYPE_CHECKING");
                self.visit_body(&value.body);
                self.typing = previous;
                for clause in &value.elif_else_clauses {
                    self.visit_body(&clause.body);
                }
            }
            _ => walk_stmt(self, statement),
        }
    }
}
fn cycles(graph: &BTreeMap<String, BTreeSet<String>>, order: &[String]) -> Vec<Vec<String>> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for root in order {
        let mut visited = BTreeSet::new();
        let mut path = Vec::<String>::new();
        let mut stack = vec![(root.clone(), false)];
        while let Some((node, leave)) = stack.pop() {
            if leave {
                path.pop();
                visited.insert(node);
                continue;
            }
            if let Some(start) = path.iter().position(|value| value == &node) {
                let mut cycle = path[start..].to_vec();
                let minimum = cycle
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, name)| *name)
                    .unwrap()
                    .0;
                cycle.rotate_left(minimum);
                if seen.insert(cycle.clone()) {
                    result.push(cycle);
                }
                continue;
            }
            if visited.contains(&node) {
                continue;
            }
            path.push(node.clone());
            stack.push((node.clone(), true));
            if let Some(children) = graph.get(&node) {
                stack.extend(children.iter().rev().map(|name| (name.clone(), false)));
            }
        }
    }
    result
}
pub(super) fn check(
    workspace: &QualityWorkspace,
    index: &ProjectIndex<'_>,
) -> Vec<QualityDiagnostic> {
    let mut graph = BTreeMap::<(usize, String), BTreeSet<String>>::new();
    let mut order = BTreeMap::<usize, Vec<String>>::new();
    for module in &index.modules {
        let Some(document) = &module.document else {
            continue;
        };
        let mut edges = Edges {
            module,
            directives: directives::instructions(document),
            values: Vec::new(),
            typing: false,
        };
        if edges.directives.iter().any(|entry| entry.skip) {
            continue;
        }
        edges.visit_body(&document.parsed.syntax().body);
        for target in edges.values {
            if target == module.name {
                continue;
            }
            if index.names.contains_key(&(module.project, target.clone())) {
                if !graph.contains_key(&(module.project, module.name.clone())) {
                    order
                        .entry(module.project)
                        .or_default()
                        .push(module.name.clone());
                }
                graph
                    .entry((module.project, module.name.clone()))
                    .or_default()
                    .insert(target);
            }
        }
    }
    let mut results = Vec::new();
    for (project, order) in order {
        let graph: BTreeMap<_, _> = graph
            .iter()
            .filter(|((owner, _), _)| *owner == project)
            .map(|((_, name), values)| (name.clone(), values.clone()))
            .collect();
        let Some((id, _)) = index
            .modules
            .iter()
            .enumerate()
            .rev()
            .find(|(id, module)| module.project == project && !workspace.sources[*id].is_stub)
        else {
            continue;
        };
        let source = &workspace.sources[id];
        for cycle in cycles(&graph, &order) {
            results.push(super::super::diagnostics::at(
                source,
                QualityTool::Pylint,
                "R0401",
                format!("Cyclic import ({})", cycle.join(" -> ")),
                super::super::diagnostics::point(0),
            ));
        }
    }
    results
}
