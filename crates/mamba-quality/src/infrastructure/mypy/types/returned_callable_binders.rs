//! Declaration-local returned Callable binders use live form and source receipts.
use super::super::signatures::{Parameter, Signature};
use super::*;
use mamba_language::application::quality_syntax::{
    python_ast::{
        visitor::{walk_expr, walk_stmt, Visitor},
        Stmt, StmtFunctionDef,
    },
    PythonTarget,
};
use ruff_text_size::Ranged;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Owner {
    namespace: String,
    sha256: String,
    target: String,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Producer {
    owner: Owner,
    declaration: (u32, u32),
    role: &'static str,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    owner: Owner,
    function: (u32, u32),
    occurrence: (u32, u32),
    path: Vec<u32>,
    variable: u32,
    role: &'static str,
}

#[derive(Clone)]
struct Site {
    range: (u32, u32),
    path: Vec<u32>,
    returns: usize,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct GapKey {
    namespace: String,
    owner: Option<Owner>,
    function: (u32, u32),
    consumer: &'static str,
}

#[derive(Clone)]
struct Frame {
    owner: Owner,
    function: (u32, u32),
    sites: BTreeMap<usize, Site>,
    bound: BTreeSet<u32>,
    failed: Arc<AtomicBool>,
    seen: Arc<Mutex<BTreeSet<usize>>>,
    source_bound: Arc<Mutex<BTreeMap<usize, BTreeSet<Type>>>>,
    gap: GapKey,
}

#[derive(Clone)]
struct CompletedReturn {
    owner: Owner,
    function: (u32, u32),
    returned: Type,
    source_bound: BTreeMap<usize, BTreeSet<Type>>,
}

struct ActiveValidation {
    owner: Owner,
    names: HashMap<usize, Type>,
}

#[derive(Default)]
pub(super) struct State {
    // Addresses only validate live function ASTs. They never enter binder keys.
    sources: HashMap<usize, (Option<Owner>, usize)>,
    producers: HashMap<usize, (Option<Producer>, usize)>,
    ids: HashMap<Key, u32>,
    legacy: HashMap<u32, Option<Producer>>,
    original_ids: HashMap<Producer, Option<u32>>,
    gaps: BTreeMap<GapKey, BTreeSet<String>>,
    frame: Option<Frame>,
    completed: HashMap<usize, CompletedReturn>,
    validation_names: Option<ActiveValidation>,
}

pub(in crate::infrastructure::mypy) struct ReturnValidationGuard {
    context: Rc<Context>,
    previous: Option<ActiveValidation>,
}

impl Drop for ReturnValidationGuard {
    fn drop(&mut self) {
        self.context
            .returned_callable_binders
            .borrow_mut()
            .validation_names = self.previous.take();
    }
}

pub(in crate::infrastructure::mypy) struct SourceGuard {
    context: Rc<Context>,
    functions: Vec<usize>,
    producers: Vec<usize>,
}

pub(in crate::infrastructure::mypy) struct BorrowedSourceGuard<'a> {
    _source: SourceGuard,
    body: &'a [Stmt],
}

impl Drop for BorrowedSourceGuard<'_> {
    fn drop(&mut self) {
        // This borrow keeps the original parsed AST alive until lease release.
        let _ = self.body.len();
    }
}

impl Drop for SourceGuard {
    fn drop(&mut self) {
        let mut state = self.context.returned_callable_binders.borrow_mut();
        for address in &self.functions {
            let remove = state.sources.get_mut(address).is_some_and(|(_, leases)| {
                *leases -= 1;
                *leases == 0
            });
            if remove {
                state.sources.remove(address);
                state.completed.remove(address);
            }
        }
        for address in &self.producers {
            let remove = state.producers.get_mut(address).is_some_and(|(_, leases)| {
                *leases -= 1;
                *leases == 0
            });
            if remove {
                state.producers.remove(address);
            }
        }
    }
}

pub(in crate::infrastructure::mypy) struct Input {
    namespace: String,
    enclosing: BTreeSet<Type>,
    enclosing_known: bool,
    proved: BTreeSet<usize>,
    consumer: &'static str,
    method_refusal: Option<(u32, &'static str)>,
    method_form_refusals: BTreeMap<usize, (u32, &'static str)>,
}

impl Input {
    pub(in crate::infrastructure::mypy) fn with_method_refusals(
        mut self,
        outer: Option<(u32, &'static str)>,
        forms: BTreeMap<usize, (u32, &'static str)>,
    ) -> Self {
        self.method_refusal = outer;
        self.method_form_refusals = forms;
        self
    }
}

struct Guard<'a> {
    context: &'a Context,
    previous: Option<Frame>,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.context.returned_callable_binders.borrow_mut().frame = self.previous.take();
    }
}

fn range(value: &impl Ranged) -> (u32, u32) {
    (value.start().to_u32(), value.end().to_u32())
}

fn arguments(expression: &Expr) -> Option<(&Expr, &Expr, &Expr)> {
    let Expr::Subscript(value) = expression else {
        return None;
    };
    let Expr::Tuple(tuple) = value.slice.as_ref() else {
        return None;
    };
    let [parameters, returns] = tuple.elts.as_slice() else {
        return None;
    };
    Some((&value.value, parameters, returns))
}

fn ordinary(parameters: &Expr) -> bool {
    matches!(parameters, Expr::List(_) | Expr::EllipsisLiteral(_))
}

impl Context {
    pub(in crate::infrastructure::mypy) fn returned_callable_validation(
        self: &Rc<Self>,
        function: &StmtFunctionDef,
        returned: Option<Type>,
        variables: &VariableMap,
        namespace: &str,
        sha256: &str,
        target: PythonTarget,
    ) -> Option<ReturnValidationGuard> {
        let expression = function.returns.as_deref()?;
        let address = function as *const _ as usize;
        let owner = Owner {
            namespace: namespace.into(),
            sha256: sha256.into(),
            target: format!("{target:?}"),
        };
        // The declaration lease can change while the returned-binder source
        // lease still holds this same AST. Refuse a cached completion then.
        if self.source_expression_identity(expression)
            != Some((
                owner.namespace.clone(),
                owner.sha256.clone(),
                owner.target.clone(),
            ))
        {
            return None;
        }
        let completed = {
            let state = self.returned_callable_binders.borrow();
            if state
                .sources
                .get(&address)
                .and_then(|(owner, _)| owner.as_ref())
                != Some(&owner)
            {
                return None;
            }
            state.completed.get(&address).cloned()?
        };
        if completed.owner != owner
            || completed.function != range(function)
            || Some(completed.returned) != returned
        {
            return None;
        }
        struct Names<'a> {
            context: &'a Context,
            owner: &'a Owner,
            sites: &'a BTreeMap<usize, BTreeSet<Type>>,
            variables: &'a VariableMap,
            active: Option<BTreeSet<Type>>,
            names: HashMap<usize, Type>,
        }
        impl<'a> Visitor<'a> for Names<'a> {
            fn visit_expr(&mut self, value: &'a Expr) {
                let previous = self.active.clone();
                if let Some((_, parameters, _)) =
                    arguments(value).filter(|(_, parameters, _)| ordinary(parameters))
                {
                    // Each nested Callable needs its own completed source receipt.
                    self.active = self.sites.get(&(parameters as *const _ as usize)).cloned();
                }
                if let (Some(bound), Expr::Name(name)) = (self.active.as_ref(), value) {
                    if let Some(&ty) = self.variables.get(name.id.as_str()) {
                        if bound.contains(&ty)
                            && self.context.variable_identity(ty).is_some_and(|id| {
                                let state = self.context.returned_callable_binders.borrow();
                                state
                                    .legacy
                                    .get(&id)
                                    .and_then(|p| p.as_ref())
                                    .is_some_and(|p| {
                                        p.owner == *self.owner
                                            && state.original_ids.get(p) == Some(&Some(id))
                                    })
                            })
                        {
                            self.names.insert(value as *const _ as usize, ty);
                        }
                    }
                }
                walk_expr(self, value);
                self.active = previous;
            }
        }
        let mut names = Names {
            context: self,
            owner: &owner,
            sites: &completed.source_bound,
            variables,
            active: None,
            names: HashMap::new(),
        };
        names.visit_expr(expression);
        let previous = self
            .returned_callable_binders
            .borrow_mut()
            .validation_names
            .replace(ActiveValidation {
                owner: owner.clone(),
                names: names.names,
            });
        Some(ReturnValidationGuard {
            context: self.clone(),
            previous,
        })
    }

    pub(in crate::infrastructure::mypy) fn returned_callable_validation_name(
        &self,
        expression: &Expr,
    ) -> Option<Type> {
        let (owner, ty) = {
            let state = self.returned_callable_binders.borrow();
            let active = state.validation_names.as_ref()?;
            (
                active.owner.clone(),
                active
                    .names
                    .get(&(expression as *const _ as usize))
                    .copied()?,
            )
        };
        (self.source_expression_identity(expression)
            == Some((owner.namespace, owner.sha256, owner.target)))
        .then_some(ty)
    }

    pub(in crate::infrastructure::mypy) fn register_returned_callable_source(
        self: &Rc<Self>,
        namespace: &str,
        sha256: &str,
        target: PythonTarget,
        body: &[Stmt],
    ) -> SourceGuard {
        #[derive(Default)]
        struct Sites {
            functions: BTreeSet<usize>,
            producers: BTreeMap<usize, (u32, u32)>,
        }
        impl<'a> Visitor<'a> for Sites {
            fn visit_stmt(&mut self, statement: &'a Stmt) {
                if let Stmt::FunctionDef(function) = statement {
                    self.functions.insert(function as *const _ as usize);
                }
                if let Stmt::Assign(assignment) = statement {
                    if let [Expr::Name(name)] = assignment.targets.as_slice() {
                        if matches!(assignment.value.as_ref(), Expr::Call(_)) {
                            self.producers
                                .insert(name as *const _ as usize, range(name));
                        }
                    }
                }
                walk_stmt(self, statement);
            }
        }
        let owner = Owner {
            namespace: namespace.into(),
            sha256: sha256.into(),
            target: format!("{target:?}"),
        };
        let mut sites = Sites::default();
        sites.visit_body(body);
        let mut state = self.returned_callable_binders.borrow_mut();
        for &address in &sites.functions {
            state
                .sources
                .entry(address)
                .and_modify(|(old, leases)| {
                    *leases += 1;
                    if old.as_ref() != Some(&owner) {
                        *old = None;
                    }
                })
                .or_insert_with(|| (Some(owner.clone()), 1));
        }
        for (&address, &declaration) in &sites.producers {
            let producer = Producer {
                owner: owner.clone(),
                declaration,
                role: "legacy.TypeVar",
            };
            state
                .producers
                .entry(address)
                .and_modify(|(old, leases)| {
                    *leases += 1;
                    if old.as_ref() != Some(&producer) {
                        *old = None;
                    }
                })
                .or_insert((Some(producer), 1));
        }
        SourceGuard {
            context: self.clone(),
            functions: sites.functions.into_iter().collect(),
            producers: sites.producers.into_keys().collect(),
        }
    }

    pub(in crate::infrastructure::mypy) fn borrow_returned_callable_source<'a>(
        self: &Rc<Self>,
        namespace: &str,
        sha256: &str,
        target: PythonTarget,
        body: &'a [Stmt],
    ) -> BorrowedSourceGuard<'a> {
        BorrowedSourceGuard {
            _source: self.register_returned_callable_source(namespace, sha256, target, body),
            body,
        }
    }

    pub(in crate::infrastructure::mypy) fn returned_callable_input(
        &self,
        function: &StmtFunctionDef,
        namespace: &str,
        enclosing: BTreeSet<Type>,
        enclosing_known: bool,
        consumer: &'static str,
        proof: &impl Fn(&Expr) -> bool,
    ) -> Input {
        struct Forms<'a, F> {
            proof: &'a F,
            proved: BTreeSet<usize>,
        }
        impl<'a, F: Fn(&Expr) -> bool> Visitor<'a> for Forms<'_, F> {
            fn visit_expr(&mut self, value: &'a Expr) {
                if let Some((base, parameters, _)) = arguments(value) {
                    if (self.proof)(base) {
                        self.proved.insert(parameters as *const _ as usize);
                    }
                }
                walk_expr(self, value);
            }
        }
        let mut forms = Forms {
            proof,
            proved: BTreeSet::new(),
        };
        if let Some(value) = function.returns.as_deref() {
            forms.visit_expr(value);
        }
        Input {
            namespace: namespace.into(),
            enclosing,
            enclosing_known,
            proved: forms.proved,
            consumer,
            method_refusal: None,
            method_form_refusals: BTreeMap::new(),
        }
    }

    pub(in crate::infrastructure::mypy) fn record_returned_callable_variable(
        &self,
        declaration: &impl Ranged,
        id: u32,
        role: &'static str,
    ) {
        if role != "legacy.TypeVar" {
            return;
        }
        let mut state = self.returned_callable_binders.borrow_mut();
        let producer = state
            .producers
            .get(&(declaration as *const _ as usize))
            .and_then(|(producer, _)| producer.as_ref())
            .filter(|producer| producer.declaration == range(declaration) && producer.role == role)
            .cloned();
        state
            .legacy
            .entry(id)
            .and_modify(|old| {
                if old.as_ref() != producer.as_ref() {
                    *old = None;
                }
            })
            .or_insert_with(|| producer.clone());
        if let Some(producer) = producer {
            state
                .original_ids
                .entry(producer)
                .and_modify(|old| {
                    if *old != Some(id) {
                        *old = None;
                    }
                })
                .or_insert(Some(id));
        }
    }

    fn stable_returned_callable_variable(&self, id: u32, target: &str) -> bool {
        let state = self.returned_callable_binders.borrow();
        state
            .legacy
            .get(&id)
            .and_then(|p| p.as_ref())
            .is_some_and(|producer| {
                producer.owner.target == target
                    && state.original_ids.get(producer) == Some(&Some(id))
            })
    }

    pub(in crate::infrastructure::mypy) fn returned_callable_gaps(
        &self,
        namespace: &str,
    ) -> Vec<String> {
        self.returned_callable_binders
            .borrow()
            .gaps
            .iter()
            .filter(|(key, _)| key.namespace == namespace)
            .flat_map(|(key, reasons)| {
                reasons
                    .iter()
                    .map(move |reason| format!("{}: {}", key.namespace, reason))
            })
            .collect()
    }

    fn variable_identity(&self, ty: Type) -> Option<u32> {
        match self.node(ty).as_deref() {
            Some(Node::Variable(value)) => Some(value.id),
            _ => None,
        }
    }

    // Unlike the general collect_variables, a callable's own generic variables
    // are not free declaration variables. No other consumer uses this traversal.
    fn declaration_variables(&self, ty: Type, bound: &BTreeSet<u32>, result: &mut Vec<Type>) {
        let Some(node) = self.node(ty) else { return };
        match node.as_ref() {
            Node::Variable(value) => {
                if !bound.contains(&value.id) && !result.contains(&ty) {
                    result.push(ty);
                }
            }
            Node::Callable(signature) => {
                let mut bound = bound.clone();
                bound.extend(
                    signature
                        .generic
                        .iter()
                        .filter_map(|&ty| self.variable_identity(ty)),
                );
                for ty in signature
                    .parameters
                    .iter()
                    .filter_map(|p| p.ty)
                    .chain(signature.returns)
                    .chain(signature.narrowing.map(|(_, ty)| ty))
                {
                    self.declaration_variables(ty, &bound, result);
                }
            }
            Node::Container(_, items)
            | Node::Tuple(items)
            | Node::Union(items)
            | Node::AppliedClass {
                arguments: items, ..
            }
            | Node::AliasInstance(_, items)
            | Node::InferredAlias {
                arguments: items, ..
            } => {
                for &ty in items {
                    self.declaration_variables(ty, bound, result);
                }
            }
            Node::VariadicTuple(ty)
            | Node::Unpack(ty)
            | Node::ParamSpecArgs(ty)
            | Node::ParamSpecKwargs(ty) => self.declaration_variables(*ty, bound, result),
            Node::Parameters(parameters, _) => {
                for ty in parameters.iter().filter_map(|p| p.ty) {
                    self.declaration_variables(ty, bound, result);
                }
            }
            _ => {}
        }
    }

    // Reuse annotation form rules for the two forms whose non-type payload
    // must not participate in binder collection (typeanal.py:2783-2793).
    fn binder_annotation_payload<'a>(
        &self,
        expression: &'a Expr,
        variables: &VariableMap,
    ) -> Option<Vec<&'a Expr>> {
        let Expr::Subscript(value) = expression else {
            return None;
        };
        if self.is_literal_form(&value.value, variables) {
            return Some(Vec::new());
        }
        if self.typing_form(&value.value, variables).as_deref() == Some("Annotated") {
            let first = match value.slice.as_ref() {
                Expr::Tuple(tuple) => tuple.elts.first(),
                value => Some(value),
            };
            return Some(first.into_iter().collect());
        }
        None
    }

    fn return_outer_variables(
        &self,
        expression: &Expr,
        variables: &VariableMap,
        sites: Option<&BTreeMap<usize, Site>>,
        result: &mut Vec<Type>,
    ) {
        struct Outside<'a> {
            context: &'a Context,
            variables: &'a VariableMap,
            sites: Option<&'a BTreeMap<usize, Site>>,
            result: &'a mut Vec<Type>,
        }
        impl<'a> Visitor<'a> for Outside<'_> {
            fn visit_expr(&mut self, expression: &'a Expr) {
                if let Some(payload) = self
                    .context
                    .binder_annotation_payload(expression, self.variables)
                {
                    for value in payload {
                        self.visit_expr(value);
                    }
                    return;
                }
                let stop = self.sites.map_or_else(
                    || {
                        self.context
                            .annotation_with_variables(expression, self.variables)
                            .is_some_and(|ty| {
                                matches!(self.context.node(ty).as_deref(), Some(Node::Callable(_)))
                            })
                    },
                    |sites| {
                        arguments(expression)
                            .is_some_and(|(_, p, _)| sites.contains_key(&(p as *const _ as usize)))
                    },
                );
                if stop {
                    return;
                }
                if matches!(
                    expression,
                    Expr::Name(_) | Expr::Attribute(_) | Expr::StringLiteral(_)
                ) {
                    if let Some(ty) = self
                        .context
                        .annotation_with_variables(expression, self.variables)
                    {
                        self.context
                            .declaration_variables(ty, &BTreeSet::new(), self.result);
                    }
                    return;
                }
                walk_expr(self, expression);
            }
        }
        Outside {
            context: self,
            variables,
            sites,
            result,
        }
        .visit_expr(expression);
    }

    pub(in crate::infrastructure::mypy) fn returned_callable_return(
        &self,
        function: &StmtFunctionDef,
        variables: &VariableMap,
        parameters: &[Parameter],
        returns: Option<Type>,
        narrowing: Option<Type>,
        input: &Input,
    ) -> (Option<Type>, Option<Vec<Type>>) {
        let function_address = function as *const _ as usize;
        self.returned_callable_binders
            .borrow_mut()
            .completed
            .remove(&function_address);
        let owner = self
            .returned_callable_binders
            .borrow()
            .sources
            .get(&(function as *const _ as usize))
            .and_then(|(owner, _)| owner.clone());
        let key = GapKey {
            namespace: owner
                .as_ref()
                .map_or_else(|| input.namespace.clone(), |owner| owner.namespace.clone()),
            owner: owner.clone(),
            function: range(function),
            consumer: input.consumer,
        };
        // A successful refresh settles this consumer. An unknown annotation
        // cannot erase a previously observed ownership obligation.
        let Some(expression) = function.returns.as_deref() else {
            return (returns, None);
        };
        let mut outer = Vec::new();
        for ty in parameters.iter().filter_map(|p| p.ty) {
            self.declaration_variables(ty, &BTreeSet::new(), &mut outer);
        }
        for parameter in function.type_params.iter().flat_map(|p| &p.type_params) {
            let name = match parameter {
                mamba_language::application::quality_syntax::python_ast::TypeParam::TypeVar(v) => v.name.as_str(),
                mamba_language::application::quality_syntax::python_ast::TypeParam::ParamSpec(v) => v.name.as_str(),
                mamba_language::application::quality_syntax::python_ast::TypeParam::TypeVarTuple(v) => v.name.as_str(),
            };
            if let Some(&ty) = variables.get(name) {
                if !outer.contains(&ty) {
                    outer.push(ty);
                }
            }
        }
        if function.type_params.is_some() {
            let mut explicit = Vec::new();
            for parameter in function.type_params.iter().flat_map(|p| &p.type_params) {
                let name = match parameter {
                    mamba_language::application::quality_syntax::python_ast::TypeParam::TypeVar(v) => v.name.as_str(),
                    mamba_language::application::quality_syntax::python_ast::TypeParam::ParamSpec(v) => v.name.as_str(),
                    mamba_language::application::quality_syntax::python_ast::TypeParam::TypeVarTuple(v) => v.name.as_str(),
                };
                if let Some(&ty) = variables.get(name) {
                    if !explicit.contains(&ty) {
                        explicit.push(ty);
                    }
                }
            }
            for ty in outer {
                if !explicit.contains(&ty) {
                    explicit.push(ty);
                }
            }
            outer = explicit;
        }
        // Determine outside-return occurrences before testing whether any
        // Callable needs a new binder. Shape here only stops collection; it
        // never admits a binder without the later live form proof.
        self.return_outer_variables(expression, variables, None, &mut outer);
        let enclosing: BTreeSet<_> = input
            .enclosing
            .iter()
            .filter_map(|&ty| self.variable_identity(ty))
            .collect();
        let mut known = enclosing.clone();
        known.extend(outer.iter().filter_map(|&ty| self.variable_identity(ty)));
        struct Sites<'a> {
            context: &'a Context,
            variables: &'a VariableMap,
            input: &'a Input,
            known: &'a BTreeSet<u32>,
            sites: BTreeMap<usize, Site>,
            path: Vec<u32>,
            counts: Vec<u32>,
            gaps: BTreeSet<String>,
            needs: bool,
        }
        impl<'a> Visitor<'a> for Sites<'_> {
            fn visit_expr(&mut self, value: &'a Expr) {
                let index = self.counts.last_mut().unwrap();
                self.path.push(*index);
                *index += 1;
                self.counts.push(0);
                if let Some(payload) = self
                    .context
                    .binder_annotation_payload(value, self.variables)
                {
                    for value in payload {
                        self.visit_expr(value);
                    }
                    self.counts.pop();
                    self.path.pop();
                    return;
                }
                if let Some(ty) = self
                    .context
                    .annotation_with_variables(value, self.variables)
                {
                    if matches!(self.context.node(ty).as_deref(), Some(Node::Callable(_))) {
                        let mut free = Vec::new();
                        self.context
                            .declaration_variables(ty, self.known, &mut free);
                        if !free.is_empty() {
                            self.needs = true;
                            let candidate = arguments(value).filter(|(_, p, _)| ordinary(p));
                            let refusal = candidate.and_then(|(base, _, _)| {
                                self.input
                                    .method_form_refusals
                                    .get(&(base as *const Expr as usize))
                            });
                            let admitted = candidate.filter(|(base, p, _)| {
                                self.input.proved.contains(&(*p as *const _ as usize))
                                    && Context::annotation_form_name(base)
                                        .and_then(|name| self.variables.get(&name).copied())
                                        .and_then(|ty| self.context.annotation_alias_value(ty))
                                        .is_some_and(|ty| self.context.is_callable_form(ty))
                            });
                            if let Some((_, p, r)) = admitted {
                                if free.iter().any(|&ty| !matches!(self.context.node(ty).as_deref(), Some(Node::Variable(v)) if !v.parameter_spec && !v.type_var_tuple)) {
                                    self.gaps.insert(format!("Returned Callable binder has unsupported variadic variables at byte {}", value.start().to_u32()));
                                } else {
                                    self.sites.insert(p as *const _ as usize, Site { range: range(value), path: self.path.clone(), returns: r as *const _ as usize });
                                }
                            } else {
                                self.gaps.insert(format!("Returned Callable binder lacks a live ordinary Callable form receipt at byte {}", value.start().to_u32()));
                                if let Some((offset, reason)) = refusal {
                                    self.gaps.insert(format!("Returned Callable method form receipt refused at byte {offset}: {reason}"));
                                }
                            }
                        } else if let Some((_, p, r)) = arguments(value)
                            .filter(|(_, p, _)| ordinary(p))
                            .filter(|(base, p, _)| {
                                self.input.proved.contains(&(*p as *const _ as usize))
                                    && Context::annotation_form_name(base)
                                        .and_then(|name| self.variables.get(&name).copied())
                                        .and_then(|ty| self.context.annotation_alias_value(ty))
                                        .is_some_and(|ty| self.context.is_callable_form(ty))
                            })
                        {
                            self.sites.insert(
                                p as *const _ as usize,
                                Site {
                                    range: range(value),
                                    path: self.path.clone(),
                                    returns: r as *const _ as usize,
                                },
                            );
                        }
                    }
                }
                walk_expr(self, value);
                self.counts.pop();
                self.path.pop();
            }
        }
        let mut sites = Sites {
            context: self,
            variables,
            input,
            known: &known,
            sites: BTreeMap::new(),
            path: Vec::new(),
            counts: vec![0],
            gaps: BTreeSet::new(),
            needs: false,
        };
        sites.visit_expr(expression);
        if !sites.needs {
            if !sites.sites.is_empty() && sites.gaps.is_empty() {
                self.returned_callable_binders
                    .borrow_mut()
                    .gaps
                    .remove(&key);
            }
            return (returns, None);
        }
        if !input.enclosing_known {
            sites.gaps.insert(format!(
                "Returned Callable binder lacks a complete enclosing-bound receipt at byte {}",
                function.start().to_u32()
            ));
            if let Some((offset, reason)) = input.method_refusal {
                sites.gaps.insert(format!(
                    "Returned Callable method producer receipt refused at byte {offset}: {reason}"
                ));
            }
        }
        if narrowing.is_some() {
            sites.gaps.insert(format!("Returned Callable binder under an outer narrowing annotation is unsupported at byte {}", function.start().to_u32()));
        }
        if owner.as_ref().is_none_or(|owner| {
            owner.namespace != input.namespace
                || owner.target != format!("{:?}", self.module_target(&input.namespace))
        }) {
            sites.gaps.insert(format!(
                "Returned Callable binder lacks an immutable source owner at byte {}",
                function.start().to_u32()
            ));
        }
        if !sites.gaps.is_empty() {
            self.returned_callable_binders
                .borrow_mut()
                .gaps
                .insert(key, sites.gaps);
            return (returns, None);
        }
        let Sites { sites, .. } = sites;
        self.return_outer_variables(expression, variables, Some(&sites), &mut outer);
        known.extend(outer.iter().filter_map(|&ty| self.variable_identity(ty)));
        let failed = Arc::new(AtomicBool::new(false));
        let expected: BTreeSet<_> = sites.keys().copied().collect();
        let seen = Arc::new(Mutex::new(BTreeSet::new()));
        let source_bound = Arc::new(Mutex::new(BTreeMap::new()));
        let frame = Frame {
            owner: owner.unwrap(),
            function: range(function),
            sites,
            bound: known,
            failed: failed.clone(),
            seen: seen.clone(),
            source_bound: source_bound.clone(),
            gap: key.clone(),
        };
        let completed_owner = frame.owner.clone();
        let _guard = self.returned_callable_frame(frame);
        let result = self.annotation_with_variables(expression, variables);
        let all_visited = *seen.lock().expect("returned Callable occurrence receipt") == expected;
        if result.is_none() || failed.load(Ordering::Relaxed) || !all_visited {
            self.returned_callable_binders
                .borrow_mut()
                .gaps
                .entry(key)
                .or_default()
                .insert(format!(
                    "Returned Callable binder annotation rebuild is incomplete at byte {}",
                    function.start().to_u32()
                ));
            return (returns, None);
        }
        outer.retain(|&ty| {
            self.variable_identity(ty)
                .is_none_or(|id| !enclosing.contains(&id))
        });
        self.returned_callable_binders
            .borrow_mut()
            .gaps
            .remove(&key);
        if let Some(returned) = result {
            self.returned_callable_binders
                .borrow_mut()
                .completed
                .insert(
                    function_address,
                    CompletedReturn {
                        owner: completed_owner,
                        function: range(function),
                        returned,
                        source_bound: source_bound
                            .lock()
                            .expect("returned Callable source-bound receipt")
                            .clone(),
                    },
                );
        }
        (result, Some(outer))
    }

    fn returned_callable_frame(&self, frame: Frame) -> Guard<'_> {
        let previous = self
            .returned_callable_binders
            .borrow_mut()
            .frame
            .replace(frame);
        Guard {
            context: self,
            previous,
        }
    }

    pub(super) fn returned_callable_annotation(
        &self,
        arguments: &[&Expr],
        variables: &VariableMap,
    ) -> Option<Type> {
        let [parameters, returns] = arguments else {
            return None;
        };
        let frame = self.returned_callable_binders.borrow().frame.clone()?;
        let site = frame
            .sites
            .get(&(*parameters as *const _ as usize))?
            .clone();
        if site.returns != *returns as *const _ as usize {
            return None;
        }
        let result =
            self.build_returned_callable_annotation(arguments, variables, frame.clone(), site);
        if result.is_some() {
            frame
                .seen
                .lock()
                .expect("returned Callable occurrence receipt")
                .insert(*parameters as *const _ as usize);
        }
        if result.is_none() {
            frame.failed.store(true, Ordering::Relaxed);
            self.returned_callable_binders
                .borrow_mut()
                .gaps
                .entry(frame.gap)
                .or_default()
                .insert(format!(
                    "Returned Callable binder metadata or annotation is unsupported at byte {}",
                    parameters.start().to_u32()
                ));
        }
        result
    }

    fn build_returned_callable_annotation(
        &self,
        arguments: &[&Expr],
        variables: &VariableMap,
        frame: Frame,
        site: Site,
    ) -> Option<Type> {
        let [parameters, returns] = arguments else {
            return None;
        };
        let parameters_address = *parameters as *const Expr as usize;
        // Probe under the same immutable input but without binder creation.
        let previous = self.returned_callable_binders.borrow_mut().frame.take();
        let probe_guard = Guard {
            context: self,
            previous,
        };
        let probe_parameters = self.callable_parameters(parameters, variables)?;
        let _probe_returns = self.annotation_with_variables(returns, variables)?;
        let mut own = Vec::new();
        for ty in probe_parameters.0.iter().filter_map(|p| p.ty) {
            self.declaration_variables(ty, &frame.bound, &mut own);
        }
        self.return_outer_variables(returns, variables, Some(&frame.sites), &mut own);
        own.retain(|&ty| {
            self.variable_identity(ty)
                .is_some_and(|id| !frame.bound.contains(&id))
        });
        let mut ids = BTreeSet::new();
        let valid = own.iter().all(|&ty| matches!(self.node(ty).as_deref(), Some(Node::Variable(v)) if !v.parameter_spec && !v.type_var_tuple && self.stable_returned_callable_variable(v.id, &frame.owner.target) && ids.insert(v.id)));
        if !valid {
            self.returned_callable_binders
                .borrow_mut()
                .gaps
                .entry(frame.gap.clone())
                .or_default()
                .insert(format!(
                    "Returned Callable binder lacks stable legacy producer identity at byte {}",
                    parameters.start().to_u32()
                ));
            return None;
        }
        // Metadata may retain proved outer binders. A new or otherwise unbound
        // metadata dependency needs a separate recursive binding proof.
        if own.iter().any(|&ty| {
            let Some(Node::Variable(v)) = self.node(ty).as_deref().cloned() else {
                return true;
            };
            let mut dependencies = Vec::new();
            for ty in std::iter::once(v.bound)
                .chain(v.values.iter().copied())
                .chain(v.default)
            {
                self.declaration_variables(ty, &BTreeSet::new(), &mut dependencies);
            }
            dependencies.iter().any(|&ty| {
                self.variable_identity(ty)
                    .is_some_and(|id| !frame.bound.contains(&id))
            })
        }) {
            return None;
        }
        let mut solutions = BTreeMap::new();
        let mut generic = Vec::new();
        for &ty in &own {
            let node = self.node(ty)?;
            let Node::Variable(variable) = node.as_ref() else {
                return None;
            };
            let key = Key {
                owner: frame.owner.clone(),
                function: frame.function,
                occurrence: site.range,
                path: site.path.clone(),
                variable: variable.id,
                role: "returned.ordinary.Callable",
            };
            let old = self
                .returned_callable_binders
                .borrow()
                .ids
                .get(&key)
                .copied();
            let id = old.unwrap_or_else(|| {
                let id = self.variable_id();
                self.returned_callable_binders
                    .borrow_mut()
                    .ids
                    .insert(key, id);
                id
            });
            // Rebuild all current declaration metadata; cache only numeric identity.
            let new = self.intern(Node::Variable(super::variables::Variable {
                id,
                ..variable.clone()
            }));
            solutions.insert(ty, new);
            generic.push(new);
        }
        let variables: VariableMap = variables
            .iter()
            .map(|(name, &ty)| (name.clone(), self.substitute(ty, &solutions)))
            .collect();
        drop(probe_guard);
        let source_bound = frame.source_bound.clone();
        let mut nested = frame;
        nested
            .bound
            .extend(generic.iter().filter_map(|&ty| self.variable_identity(ty)));
        let _guard = self.returned_callable_frame(nested);
        let narrowing = self.predicate_annotation(arguments[1], &variables);
        let returns = self.annotation_with_variables(returns, &variables)?;
        let (parameters, ellipsis) = self.callable_parameters(parameters, &variables)?;
        let result = self.callable_type(Arc::new(Signature {
            name: None,
            parameters: parameters.into(),
            returns: Some(returns),
            narrowing,
            return_explicit: true,
            implicit: false,
            plain_source_implicit_name: false,
            ignored: false,
            variables: Variables::new(),
            generic: generic.into(),
            owner: None,
            constructor: false,
            ellipsis,
            overloads: Arc::from([]),
            bound_receiver: None,
            definition_module: None,
            native_symbol: None,
        }));
        source_bound
            .lock()
            .expect("returned Callable source-bound receipt")
            .insert(parameters_address, own.into_iter().collect());
        Some(result)
    }
}
