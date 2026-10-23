//! Empty dict names keep their lexical origin until a write resolves both bounds.
use super::*;
use quality_syntax::python_ast::{ExprCall, ExprName, ExprSubscript, StmtAssign};
use ruff_text_size::TextSize;
use types::{Container, Node, Type};

impl Checker<'_> {
    fn fresh_nested_empty_list_assignment(&self, assignment: &StmtAssign) -> bool {
        let [target @ Expr::Name(name)] = assignment.targets.as_slice() else {
            return false;
        };
        let Some((_, first)) = self.scopes.first_inferred_local_assignment(target) else {
            return false;
        };
        self.flow.reachable
            && name.id.as_str() != "_"
            && !self.flow.deleted.contains(name.id.as_str())
            && self.scopes.assigned_type(target).is_none()
            && !self.partial_sets.contains_key(&first)
            && self.observed_fstring_type(&assignment.value).is_none()
            && self
                .context
                .observed_expression_type(&assignment.value)
                .is_none()
    }

    // Keep the ordinary inference entry for every unproved caller. This entry
    // changes no expression visit, callback count, binding, or observation receipt.
    pub(super) fn infer_list_common_assignment(&self, assignment: &StmtAssign) -> Option<Type> {
        if !matches!(assignment.value.as_ref(), Expr::List(_))
            || !self.fresh_nested_empty_list_assignment(assignment)
        {
            return self.infer(&assignment.value);
        }
        let environment = self
            .scopes
            .inference_environment(&assignment.value, self.environment());
        self.context
            .infer_list_common_assignment(&assignment.value, &environment, &|value| {
                self.scopes.expression_type(value, &environment)
            })
            .or_else(|| self.untyped_pattern_value(&assignment.value))
    }

    // Empty syntax plus its existing inferred shape proves only this nested gap.
    pub(super) fn nested_empty_list_assignment(
        &self,
        assignment: &StmtAssign,
        actual: Option<Type>,
    ) -> bool {
        if !self.fresh_nested_empty_list_assignment(assignment) {
            return false;
        }
        let Some(actual) = actual else {
            return false;
        };
        let Some(node) = self.context.node(actual) else {
            return false;
        };
        if self.context.literal_list_common_type_incomplete(
            &assignment.value,
            actual,
            &self.environment(),
        ) {
            return true;
        }
        let Node::Container(Container::Dict, arguments) = node.as_ref() else {
            return false;
        };
        let &[key, value] = arguments.as_slice() else {
            return false;
        };
        if !matches!(
            key,
            Type::Str | Type::Bytes | Type::Int | Type::Float | Type::Complex | Type::Bool
        ) || !matches!(self.context.node(value).as_deref(),
                Some(Node::Container(Container::List, items)) if items.len() == 1 && items[0].is_any())
        {
            return false;
        }
        let empty = |value: &Expr| {
            matches!(value, Expr::List(list) if list.elts.is_empty())
                && self.observed_fstring_type(value).is_none()
                && self.context.observed_expression_type(value).is_none()
        };
        match assignment.value.as_ref() {
            Expr::Dict(dictionary) => {
                !dictionary.items.is_empty()
                    && dictionary
                        .items
                        .iter()
                        .all(|item| item.key.is_some() && empty(&item.value))
            }
            Expr::DictComp(dictionary) => {
                !dictionary.generators.is_empty()
                    && dictionary
                        .generators
                        .iter()
                        .all(|generator| !generator.is_async)
                    && empty(&dictionary.value)
            }
            _ => false,
        }
    }

    // Call only after the existing annotation diagnostic, with the source proof
    // from nested_empty_list_assignment. Other Never values retain their meaning.
    pub(super) fn recover_nested_empty_list(&self, ty: Type, incomplete: bool) -> Type {
        if incomplete {
            self.context
                .substitute(ty, &BTreeMap::from([(Type::Never, Type::Any)]))
        } else {
            ty
        }
    }

    pub(super) fn partial_dict_key(&self, name: &str) -> Option<TextSize> {
        let (owner, definition) = self.scopes.inferred_binding_identity(name)?;
        self.partial_sets
            .get(&definition)
            .filter(|partial| {
                partial.active
                    && partial.name == name
                    && partial.kind == Container::Dict
                    && partial.binding_owner == Some(owner)
            })
            .map(|_| definition)
    }

    pub(super) fn prepare_partial_dict_assignment(
        &mut self,
        assignment: &StmtAssign,
        actual: Option<Type>,
    ) {
        let [Expr::Name(name)] = assignment.targets.as_slice() else {
            return;
        };
        let empty =
            matches!(assignment.value.as_ref(), Expr::Dict(value) if value.items.is_empty());
        if let Some(key) = self.partial_dict_key(name.id.as_str()) {
            if self.partial_sets[&key].inferred.is_some() {
                return;
            }
            if empty {
                self.partial_sets.get_mut(&key).unwrap().last_assignment = name.start();
            } else if let Some(ty) = actual.and_then(|ty| self.complete_partial_dict(ty, false)) {
                if self.resolve_partial_dict(key, ty) {
                    self.partial_sets.get_mut(&key).unwrap().last_assignment = name.start();
                }
            }
        }
    }

    pub(super) fn remember_partial_dict(
        &mut self,
        assignment: &StmtAssign,
    ) -> Option<try_inputs::PartialFamily> {
        let [target @ Expr::Name(name)] = assignment.targets.as_slice() else {
            return None;
        };
        if !matches!(assignment.value.as_ref(), Expr::Dict(value) if value.items.is_empty())
            || self.partial_dict_key(name.id.as_str()).is_some()
        {
            return None;
        }
        let (owner, definition) = self.scopes.inferred_binding_identity(name.id.as_str())?;
        if definition != name.start() || self.scopes.assigned_type(target).is_some() {
            return None;
        }
        let diagnostic = self.diagnostics.len();
        // Share set diagnostic compaction so nested scopes retain valid outer indices.
        self.diagnostics
            .push(crate::infrastructure::diagnostics::at(
                self.source,
                QualityTool::Mypy,
                "var-annotated",
                format!(
                    "Need type annotation for \"{}\" (hint: \"{}: dict[<type>, <type>] = ...\")",
                    name.id, name.id
                ),
                target.range(),
            ));
        self.partial_sets.insert(
            name.start(),
            partial_sets::PartialSet {
                name: name.id.to_string(),
                target: target.clone(),
                inferred: None,
                active: true,
                last_assignment: name.start(),
                preserve_error: false,
                diagnostic,
                owner: self.partial_set_scope,
                kind: Container::Dict,
                allow_unresolved_exit: self.partial_dict_dynamic,
                binding_owner: Some(owner),
            },
        );
        Some(try_inputs::PartialFamily::Dict)
    }

    fn partial_dict_read_key(&self, name: &ExprName) -> Option<TextSize> {
        (name.ctx.is_load()
            && !self.partial_dict_lvalues.contains(&name.start())
            && !self.flow.deleted.contains(name.id.as_str()))
        .then(|| self.partial_dict_key(name.id.as_str()))
        .flatten()
    }

    pub(super) fn read_partial_dict(&mut self, name: &ExprName) {
        if let Some(key) = self.partial_dict_read_key(name) {
            self.preserve_partial_dict_read(key);
        }
    }

    pub(super) fn committed_partial_dict_read(
        &self,
        name: &ExprName,
    ) -> Option<partial_sets::PartialReadRecovery> {
        let origin = self.partial_dict_read_key(name)?;
        let partial = self.partial_sets.get(&origin)?;
        let binding_owner = partial.binding_owner?;
        let Expr::Name(declaration) = &partial.target else {
            return None;
        };
        let declaration_type =
            self.scopes
                .inferred_declaration(name.id.as_str(), binding_owner, origin)?;
        if partial.owner != self.partial_set_scope
            || partial.inferred.is_some()
            || !partial.preserve_error
            || !declaration.ctx.is_store()
            || declaration.id != name.id
            || declaration.start() != origin
            || self.environment().get(name.id.as_str()).copied() != Some(declaration_type)
            || !matches!(self.context.node(declaration_type).as_deref(),
                Some(Node::Container(Container::Dict, arguments)) if arguments.len() == 2)
        {
            return None;
        }
        let diagnostic = self.diagnostics.get(partial.diagnostic)?;
        if diagnostic.tool != QualityTool::Mypy
            || diagnostic.rule != "var-annotated"
            || diagnostic.severity != "error"
            || diagnostic.path != self.source.path
            || diagnostic.location.offset != Some(partial.target.start().to_u32())
            || diagnostic.location.end_offset != Some(partial.target.end().to_u32())
        {
            return None;
        }
        // Mypy fixup_partial_type fills the *known dict family's* unresolved
        // bounds with unannotated Any. Do not infer bounds from a raw value.
        let actual = self
            .context
            .intern(Node::Container(Container::Dict, vec![Type::Any; 2]));
        Some(partial_sets::PartialReadRecovery {
            name: name.id.to_string(),
            binding_owner,
            partial_owner: partial.owner,
            origin,
            declaration: partial.target.range(),
            declaration_type,
            read: name.range(),
            diagnostic: partial.diagnostic,
            actual,
        })
    }

    pub(super) fn partial_read_recovery_is_current(
        &self,
        receipt: &partial_sets::PartialReadRecovery,
    ) -> bool {
        self.scopes.inferred_binding_identity(&receipt.name)
            == Some((receipt.binding_owner, receipt.origin))
            && self.scopes.inferred_declaration(
                &receipt.name,
                receipt.binding_owner,
                receipt.origin,
            ) == Some(receipt.declaration_type)
            && self.environment().get(&receipt.name).copied() == Some(receipt.declaration_type)
            && !self.flow.deleted.contains(&receipt.name)
            && receipt.partial_owner == self.partial_set_scope
            && self
                .partial_sets
                .get(&receipt.origin)
                .is_some_and(|partial| {
                    partial.active
                        && partial.kind == Container::Dict
                        && partial.name == receipt.name
                        && partial.binding_owner == Some(receipt.binding_owner)
                        && partial.owner == receipt.partial_owner
                        && partial.inferred.is_none()
                        && partial.preserve_error
                        && partial.target.range() == receipt.declaration
                        && partial.diagnostic == receipt.diagnostic
                })
            && self
                .diagnostics
                .get(receipt.diagnostic)
                .is_some_and(|diagnostic| {
                    diagnostic.tool == QualityTool::Mypy
                        && diagnostic.rule == "var-annotated"
                        && diagnostic.severity == "error"
                        && diagnostic.path == self.source.path
                        && diagnostic.location.offset == Some(receipt.declaration.start().to_u32())
                        && diagnostic.location.end_offset
                            == Some(receipt.declaration.end().to_u32())
                })
    }

    fn preserve_partial_dict_read(&mut self, key: TextSize) {
        let partial = self.partial_sets.get_mut(&key).unwrap();
        partial.preserve_error |= partial.inferred.is_none();
    }

    pub(super) fn reassign_partial_dict(&mut self, name: &ExprName) {
        if let Some(key) = self.partial_dict_key(name.id.as_str()) {
            let partial = self.partial_sets.get_mut(&key).unwrap();
            if partial.last_assignment != name.start() {
                partial.preserve_error |= partial.inferred.is_none();
                partial.active = false;
            }
        }
    }

    pub(super) fn partial_dict_assignment_receivers(
        &self,
        assignment: &StmtAssign,
    ) -> Vec<TextSize> {
        assignment
            .targets
            .iter()
            .filter_map(|target| {
                let Expr::Subscript(subscript) = target else {
                    return None;
                };
                let Expr::Name(name) = subscript.value.as_ref() else {
                    return None;
                };
                self.partial_dict_key(name.id.as_str())
                    .map(|_| name.start())
            })
            .collect()
    }

    pub(super) fn resolve_partial_dict_index(&mut self, target: &ExprSubscript, value: &Expr) {
        let Expr::Name(name) = target.value.as_ref() else {
            return;
        };
        let Some(key) = self.partial_dict_key(name.id.as_str()) else {
            return;
        };
        if self.flow.deleted.contains(name.id.as_str()) {
            return;
        }
        if self.partial_sets[&key].inferred.is_some() {
            return;
        }
        let arguments = self
            .partial_dict_item(&target.slice)
            .zip(self.partial_dict_item(value));
        if let Some((key_type, value_type)) = arguments {
            let ty = self
                .context
                .intern(Node::Container(Container::Dict, vec![key_type, value_type]));
            if self.resolve_partial_dict(key, ty) {
                return;
            }
        }
        // The ordinary indexed assignment next reads an unresolved base.
        self.preserve_partial_dict_read(key);
    }

    fn partial_dict_update<'a>(
        &self,
        expression: &'a Expr,
    ) -> Option<(&'a ExprCall, &'a ExprName)> {
        let Expr::Call(call) = expression else {
            return None;
        };
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return None;
        };
        let Expr::Name(name) = attribute.value.as_ref() else {
            return None;
        };
        if attribute.attr.as_str() != "update"
            || call.arguments.args.len() != 1
            || !call.arguments.keywords.is_empty()
            || call.arguments.args[0].is_starred_expr()
            || self.flow.deleted.contains(name.id.as_str())
        {
            return None;
        }
        let key = self.partial_dict_key(name.id.as_str())?;
        self.partial_sets[&key]
            .inferred
            .is_none()
            .then_some((call, name))
    }

    pub(super) fn partial_dict_update_receiver(&self, expression: &Expr) -> Option<TextSize> {
        self.partial_dict_update(expression)
            .map(|(_, name)| name.start())
    }

    pub(super) fn finish_partial_dict_update(&mut self, expression: &Expr) {
        let Some((call, name)) = self.partial_dict_update(expression) else {
            return;
        };
        let key = self.partial_dict_key(name.id.as_str()).unwrap();
        // Argument visits have already committed any self-read before this inference.
        let inferred = self
            .partial_dict_item(&call.arguments.args[0])
            .and_then(|ty| self.complete_partial_dict(ty, true));
        if let Some(ty) = inferred {
            if self.resolve_partial_dict(key, ty) {
                return;
            }
        }
        self.preserve_partial_dict_read(key);
    }

    fn resolve_partial_dict(&mut self, key: TextSize, ty: Type) -> bool {
        let name = self.partial_sets[&key].name.clone();
        let Some(receipt) = self.scopes.refine_inferred_binding_receipt(&name, key, ty) else {
            return false;
        };
        self.partial_sets.get_mut(&key).unwrap().inferred = Some(ty);
        self.write_partial_declaration(&name, key, try_inputs::PartialFamily::Dict, receipt);
        true
    }

    fn complete_partial_dict(&self, ty: Type, bounds_are_items: bool) -> Option<Type> {
        if ty.is_any() {
            return Some(
                self.context
                    .intern(Node::Container(Container::Dict, vec![Type::Any; 2])),
            );
        }
        matches!(self.context.node(ty).as_deref(),
            Some(Node::Container(Container::Dict, arguments))
                if arguments.iter().all(|&item| self.complete_partial_dict_item(item, bounds_are_items)))
        .then_some(ty)
    }

    fn partial_dict_item(&self, expression: &Expr) -> Option<Type> {
        if matches!(expression, Expr::Dict(value) if value.items.is_empty())
            || matches!(expression, Expr::List(value) if value.elts.is_empty())
        {
            return None;
        }
        self.infer(expression)
            .filter(|&ty| self.complete_partial_dict_item(ty, true))
    }

    fn complete_partial_dict_item(&self, ty: Type, top: bool) -> bool {
        if ty == Type::Never || (top && ty == Type::None) {
            return false;
        }
        match self.context.node(ty).as_deref() {
            Some(Node::Container(_, items) | Node::Tuple(items) | Node::Union(items)) => items
                .iter()
                .all(|&item| self.complete_partial_dict_item(item, false)),
            Some(Node::VariadicTuple(item) | Node::Unpack(item)) => {
                self.complete_partial_dict_item(*item, false)
            }
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{ports::ProjectStore, source::PythonVersion};
    use crate::infrastructure::filesystem::Filesystem;
    use std::fs;

    #[test]
    fn empty_dict_reads_writes_shadowing_and_dynamic_exit_keep_partial_identity() {
        let source = r#"shadow = {}
shadow["x"] = 1
from typing import Any
def takes_int(value: int) -> int:
    return value
def parameter_shadow(shadow: int) -> None:
    takes_int(shadow)
items = [takes_int(shadow) for shadow in [1]]
function = lambda shadow: takes_int(shadow)

def typed() -> None:
    pending = {}
    def collect() -> None:
        pending["x"] = len(pending)
    resolved = {}
    resolved["x"] = 1
    resolved["y"] = "bad"
    annotated: dict[Any, Any] = {}
    annotated["x"] = 1
    outer = {}
    def shadow() -> None:
        outer: dict[str, int] = {}
        len(outer)
    outer["x"] = 1
    updated = {}
    updated.update({"x": 1})
    len(updated)
    reassigned = {}
    reassigned = {"x": 1}
    len(reassigned)
    values = set()
    values.add(1)

def dynamic():
    unused = {}
    observed = {}
    len(observed)

def outer_write() -> None:
    captured = {}
    def collect() -> None:
        captured["x"] = 1
    len(captured)

def restored() -> None:
    unresolved = {}
"#;
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("pyproject.toml"),
            "[project]\nname = \"fixture\"\nversion = \"0.0.0\"\nrequires-python = \">=3.12\"\n",
        )
        .unwrap();
        let path = root.path().join("main.py");
        fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = PythonVersion::Py312;
        let report = crate::infrastructure::mypy::check(&workspace);
        let partials: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule == "var-annotated")
            .map(|diagnostic| {
                (
                    diagnostic.location.line,
                    diagnostic.location.column,
                    diagnostic.location.end_column,
                    diagnostic.message.as_str(),
                )
            })
            .collect();
        assert_eq!(
            partials,
            [
                (12, 5, Some(11), "Need type annotation for \"pending\" (hint: \"pending: dict[<type>, <type>] = ...\")"),
                (36, 5, Some(12), "Need type annotation for \"observed\" (hint: \"observed: dict[<type>, <type>] = ...\")"),
                (46, 5, Some(14), "Need type annotation for \"unresolved\" (hint: \"unresolved: dict[<type>, <type>] = ...\")"),
            ]
        );
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule == "assignment" && diagnostic.location.line == 17));
        assert!(!report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule == "arg-type"));
        assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
    }
}
