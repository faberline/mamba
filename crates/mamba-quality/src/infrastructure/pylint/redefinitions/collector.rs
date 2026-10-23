//! Lexical declarations preserve imports and overload-free first definitions.
use super::{
    branches::{self, Branch, Path},
    decorator_redefines,
};
use mamba_language::application::quality_syntax::{
    python_ast::{
        visitor::{walk_expr, walk_stmt, Visitor},
        ExceptHandler, Expr, Parameters, Stmt, StmtFunctionDef,
    },
    PythonTarget,
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::collections::BTreeMap;

pub(super) struct Binding {
    pub(super) range: TextRange,
    pub(super) branches: Path,
    overload: bool,
    pub(super) declaration: bool,
    pub(super) annotation_only: bool,
    annotation: bool,
    exempt: bool,
    forward: bool,
    pub(super) handler_name: bool,
}
pub(super) struct Reference {
    pub(super) name: String,
    pub(super) range: TextRange,
    pub(super) branches: Path,
}
struct Frame<'a> {
    body: &'a [Stmt],
    parameters: Option<&'a Parameters>,
    class: bool,
}
struct Collector<'a> {
    bindings: BTreeMap<String, Vec<Binding>>,
    aliases: BTreeMap<String, String>,
    branches: Path,
    guard: Option<&'a Expr>,
    frames: Vec<Frame<'a>>,
    target: PythonTarget,
    class: bool,
    forward: bool,
    references: Vec<Reference>,
    branch_counts: BTreeMap<u32, usize>,
}
impl Collector<'_> {
    fn add(
        &mut self,
        name: &str,
        range: TextRange,
        declaration: bool,
        overload: bool,
        exempt: bool,
    ) {
        self.bindings.entry(name.into()).or_default().push(Binding {
            range,
            branches: self.branches.clone(),
            declaration,
            annotation_only: false,
            annotation: false,
            overload,
            exempt,
            forward: self.forward,
            handler_name: false,
        });
    }
    fn qualified(&self, value: &Expr) -> Option<String> {
        let value = if let Expr::Call(value) = value {
            value.func.as_ref()
        } else {
            value
        };
        let parts = crate::infrastructure::pylint::values::path(value)?;
        let (first, rest) = parts.split_first()?;
        let base = self.aliases.get(first).unwrap_or(first);
        Some(
            std::iter::once(base.as_str())
                .chain(rest.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join("."),
        )
    }
    fn overload(&self, function: &StmtFunctionDef) -> bool {
        function.decorator_list.iter().any(|value| {
            self.qualified(&value.expression)
                .is_some_and(|name| name == "typing.overload" || name == "overload")
        })
    }
    fn forward_ref(&self, value: &Expr) -> bool {
        matches!(value, Expr::Call(_))
            && self.qualified(value).is_some_and(|name| {
                name == "typing.ForwardRef"
                    || (name == "annotationlib.ForwardRef" && self.target == PythonTarget::Py314)
            })
    }
}
impl<'a> Collector<'a> {
    fn branch(&mut self, id: u32, kind: Branch, body: &'a [Stmt], guard: Option<&'a Expr>) {
        self.branches.push((id, kind));
        let previous = self.guard.take();
        self.guard = guard;
        self.visit_body(body);
        self.guard = previous;
        self.branches.pop();
    }
}
impl<'a> Visitor<'a> for Collector<'a> {
    fn visit_stmt(&mut self, value: &'a Stmt) {
        match value {
            Stmt::FunctionDef(value) => {
                let exempt = decorator_redefines(value)
                    || (self.class && value.name.as_str() == "__module__")
                    || self.guard.is_some_and(|guard| {
                        branches::permits_redefinition(guard, value.name.as_str())
                    });
                self.add(
                    value.name.as_str(),
                    value.name.range(),
                    true,
                    self.overload(value),
                    exempt,
                );
                self.frames.push(Frame {
                    body: &value.body,
                    parameters: Some(&value.parameters),
                    class: false,
                });
            }
            Stmt::ClassDef(value) => {
                let exempt = self.guard.is_some_and(|guard| {
                    branches::permits_redefinition(guard, value.name.as_str())
                });
                self.add(value.name.as_str(), value.name.range(), true, false, exempt);
                self.frames.push(Frame {
                    body: &value.body,
                    parameters: None,
                    class: true,
                });
            }
            Stmt::AnnAssign(value) if matches!(value.target.as_ref(), Expr::Name(_)) => {
                if let Some(expression) = &value.value {
                    self.visit_expr(&value.target);
                    self.visit_expr(expression);
                } else if let Expr::Name(name) = value.target.as_ref() {
                    self.add(name.id.as_str(), name.range(), false, false, false);
                    self.bindings
                        .get_mut(name.id.as_str())
                        .unwrap()
                        .last_mut()
                        .unwrap()
                        .annotation_only = true;
                }
                if let Expr::Name(name) = value.target.as_ref() {
                    self.bindings
                        .get_mut(name.id.as_str())
                        .unwrap()
                        .last_mut()
                        .unwrap()
                        .annotation = true;
                }
            }
            Stmt::Assign(value) => {
                self.forward = self.forward_ref(&value.value);
                for value in &value.targets {
                    self.visit_expr(value);
                }
                self.forward = false;
                self.visit_expr(&value.value);
            }
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                for import in
                    crate::infrastructure::project::imports(std::slice::from_ref(value), "", false)
                {
                    self.add(&import.local, import.range, false, false, false);
                }
            }
            Stmt::If(value) => {
                let complete = value
                    .elif_else_clauses
                    .last()
                    .is_some_and(|clause| clause.test.is_none());
                self.branch_counts.insert(
                    value.start().to_u32(),
                    value.elif_else_clauses.len() + 1 + usize::from(!complete),
                );
                self.branch(
                    value.start().to_u32(),
                    Branch::If(0),
                    &value.body,
                    Some(&value.test),
                );
                for (index, clause) in value.elif_else_clauses.iter().enumerate() {
                    self.branch(
                        value.start().to_u32(),
                        Branch::If(index + 1),
                        &clause.body,
                        clause.test.as_ref(),
                    );
                }
            }
            Stmt::Try(value) => {
                let id = value.start().to_u32();
                self.branch(id, Branch::TryBody, &value.body, None);
                for (index, handler) in value.handlers.iter().enumerate() {
                    let ExceptHandler::ExceptHandler(handler) = handler;
                    if let Some(name) = &handler.name {
                        self.branches.push((id, Branch::TryHandler(index)));
                        self.add(name.as_str(), name.range(), false, false, false);
                        self.bindings
                            .get_mut(name.as_str())
                            .unwrap()
                            .last_mut()
                            .unwrap()
                            .handler_name = true;
                        self.branches.pop();
                    }
                    self.branch(id, Branch::TryHandler(index), &handler.body, None);
                }
                self.branch(id, Branch::TryElse, &value.orelse, None);
                self.branch(id, Branch::TryFinally, &value.finalbody, None);
            }
            _ => {
                let previous = self.guard.take();
                walk_stmt(self, value);
                self.guard = previous;
            }
        }
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        match value {
            Expr::Name(value) if value.ctx.is_store() => {
                self.add(value.id.as_str(), value.range(), false, false, false)
            }
            Expr::Name(value) if value.ctx.is_load() => self.references.push(Reference {
                name: value.id.to_string(),
                range: value.range(),
                branches: self.branches.clone(),
            }),
            Expr::Lambda(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::Generator(_) => {}
            _ => walk_expr(self, value),
        }
    }
}

pub(super) fn scope(
    body: &[Stmt],
    parameters: Option<&Parameters>,
    class: bool,
    inherited: &BTreeMap<String, String>,
    target: PythonTarget,
    issues: &mut BTreeMap<u32, TextSize>,
    uses: &mut BTreeMap<u32, bool>,
    expired: &mut std::collections::BTreeSet<u32>,
) {
    let symbols = crate::infrastructure::project::symbols::Symbols::collect(body, "", false);
    let mut aliases = inherited.clone();
    for (name, binding) in &symbols.bindings {
        if let crate::infrastructure::project::symbols::DefinitionKind::Import(import) =
            &binding.kind
        {
            aliases.insert(
                name.clone(),
                import.member.as_ref().map_or_else(
                    || import.module.clone(),
                    |member| format!("{}.{member}", import.module),
                ),
            );
        } else {
            aliases.remove(name);
        }
    }
    let mut data = Collector {
        bindings: BTreeMap::new(),
        aliases,
        branches: Vec::new(),
        guard: None,
        frames: Vec::new(),
        target,
        class,
        forward: false,
        references: Vec::new(),
        branch_counts: BTreeMap::new(),
    };
    if let Some(parameters) = parameters {
        for parameter in parameters.iter() {
            data.add(
                parameter.name().as_str(),
                parameter.name().range(),
                false,
                false,
                false,
            );
        }
    }
    data.visit_body(body);
    if parameters.is_some() || !class {
        let mut flow_symbols =
            crate::infrastructure::project::symbols::Symbols::collect(body, "", false);
        if parameters.is_none() {
            flow_symbols.bindings.retain(|_, value| {
                matches!(
                    value.kind,
                    crate::infrastructure::project::symbols::DefinitionKind::Import(_)
                )
            });
        }
        super::flow::collect(
            &flow_symbols,
            &data.bindings,
            &data.references,
            &data.branch_counts,
            uses,
            expired,
        );
    }
    for (name, bindings) in &data.bindings {
        let Some(first) = bindings
            .iter()
            .find(|value| !value.overload && !value.annotation)
        else {
            continue;
        };
        if crate::infrastructure::pylint::defaults::fixed().ignored(name, false) {
            continue;
        }
        for (index, value) in bindings.iter().enumerate() {
            if value.declaration
                && !value.exempt
                && !value.overload
                && value.range != first.range
                && !branches::exclusive(&value.branches, &first.branches)
                && !bindings[..index].iter().any(|value| value.forward)
            {
                issues.insert(value.range.start().to_u32(), first.range.start());
            }
        }
    }
    for frame in data.frames {
        scope(
            frame.body,
            frame.parameters,
            frame.class,
            &data.aliases,
            target,
            issues,
            uses,
            expired,
        );
    }
}
