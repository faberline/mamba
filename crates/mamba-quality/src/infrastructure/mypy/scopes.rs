//! Lexical primitive bindings. Tool-specific type inference stays in `types`.
use super::signatures::Signature;
use super::types::{self, Type};
use mamba_language::application::quality_syntax::python_ast::{
    Expr, Stmt, StmtClassDef, StmtFunctionDef,
};
use mamba_language::application::quality_syntax::PythonTarget;
use ruff_text_size::{Ranged, TextSize};
use std::collections::BTreeMap;
use std::sync::Arc;
mod aliases;
mod annotation_forms;
mod attribute_members;
mod bindings;
mod bound_functions;
mod classes;
mod comparisons;
mod constructors;
mod dataclass_fields;
mod dataclass_replace;
mod declarations;
mod decorators;
mod dictionary_constructors;
mod field_factories;
mod generator_unpack;
mod generators;
mod generics;
mod imports;
mod inference;
mod inference_cycles;
mod lambdas;
pub(super) mod module_declarations;
pub(super) mod module_exports;
mod names;
mod overloads;
mod reversed;
mod runtime_predicates;
mod subscripts;
mod tuple_packs;
mod walrus;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Module,
    Class,
    Function,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Binding {
    ty: Option<Type>,
    first_definition: TextSize,
    infer_from_assignment: bool,
    callable: Option<Arc<Signature>>,
    pub(super) module: Option<String>,
    explicit: bool,
    // None keeps legacy visibility for an imported provider without hidden metadata.
    module_hidden: Option<bool>,
    annotation_form: Option<annotation_forms::Form>,
    annotation_module: Option<annotation_forms::ModuleProof>,
}

// Source proof for one ordinary annotation, installed once on scope entry.
// It identifies a declaration; it is never an assignment-success receipt.
#[derive(Clone)]
struct NamedExpressionDeclaration {
    owner: usize,
    annotation: &'static str,
    origin: Binding,
}

#[derive(Clone)]
struct Scope {
    kind: Kind,
    module_exports: Option<module_exports::ModuleExports>,
    bindings: BTreeMap<String, Binding>,
    direct_function_starts: std::collections::BTreeSet<TextSize>,
    raw_functions: BTreeMap<TextSize, Arc<Signature>>,
    // Immutable across writes and loop iterations in this lexical scope.
    named_expression_declarations: Arc<BTreeMap<String, NamedExpressionDeclaration>>,
    // True retains unknown-RHS admission for existing plain Name assignments.
    ordinary_assignment_starts: BTreeMap<TextSize, bool>,
    first_unbound_local_assignments: BTreeMap<String, TextSize>,
    comprehension_owner: Option<usize>,
    walrus_assigned: BTreeMap<String, bool>,
    redirects: BTreeMap<String, usize>,
    variables: types::Variables,
    redefinitions: std::collections::BTreeSet<String>,
    overload_aliases: std::collections::BTreeSet<String>,
    bound_variables: std::collections::BTreeSet<Type>,
    imported_names: std::collections::BTreeSet<String>,
    imported_functions: BTreeMap<String, imports::ImportedFunction>,
    import_admission: Option<imports::ImportAdmission>,
    generator: Option<(Type, bool)>,
    super_receiver: Option<(u32, Type)>,
}

// Immutable lexical facts installed only from the original module suite.
#[derive(Clone)]
struct OriginalAnnotationImport {
    module: String,
    member: String,
    range: ruff_text_size::TextRange,
    explicit: bool,
    constructor: &'static str,
}

#[derive(Clone)]
struct OriginalBuiltinRoots {
    namespace: String,
    target: mamba_language::application::quality_syntax::PythonTarget,
    roots: std::collections::BTreeSet<String>,
    global_hazards: std::collections::BTreeSet<String>,
    constructor_imports: BTreeMap<String, OriginalAnnotationImport>,
}

// A class-body receipt is issued only from the checker's borrowed original
// class AST. Addresses check membership while that AST is live; they are not
// durable class or method identities.
#[derive(Clone)]
struct ClassBodyReceipt {
    depth: usize,
    namespace: String,
    source_sha256: String,
    target: String,
    class_id: u32,
    class_name: String,
    class_offset: TextSize,
    direct_methods: BTreeMap<usize, (u32, u32)>,
    enclosing: std::collections::BTreeSet<Type>,
    generic: std::collections::BTreeSet<Type>,
}

#[derive(Clone)]
pub(super) struct Scopes {
    stack: Vec<Scope>,
    class_body_receipts: Vec<ClassBodyReceipt>,
    pub(super) context: std::rc::Rc<types::Context>,
    pub(super) modules: Option<super::modules::Lookup>,
    namespace: String,
    loop_replays: Vec<loop_state::ReplayWindow>,
    // Set only by the source-owning module/checker entry, before class refresh.
    terminal_none_source: bool,
    original_builtin_roots: Option<Arc<OriginalBuiltinRoots>>,
}

mod loop_state;

mod builtin_values;
mod entry;

impl Scopes {
    pub(super) fn new(
        body: &[Stmt],
        context: std::rc::Rc<types::Context>,
        namespace: String,
    ) -> Self {
        let mut scopes = Self {
            stack: Vec::new(),
            class_body_receipts: Vec::new(),
            context,
            modules: None,
            namespace,
            loop_replays: Vec::new(),
            terminal_none_source: false,
            original_builtin_roots: None,
        };
        scopes.enter(Kind::Module, body, BTreeMap::new(), types::Variables::new());
        scopes
    }

    pub(in crate::infrastructure::mypy) fn set_terminal_none_source(&mut self, source: &str) {
        // AST nodes omit type comments and inline Mypy flags. Require source.
        // This scope keeps its module namespace; clones retain the same proof.
        self.terminal_none_source = !source.contains("type:") && !source.contains("mypy:");
    }

    pub(super) fn leave(&mut self) {
        assert!(self.stack.len() > 1, "module scope must remain");
        if self.current().kind == Kind::Class
            && self
                .class_body_receipts
                .last()
                .is_some_and(|receipt| receipt.depth == self.stack.len() - 1)
        {
            self.class_body_receipts.pop();
        }
        self.stack.pop();
    }

    // The caller supplies the actual class AST that owns this body. A bare
    // Class frame cannot prove which class supplied a method declaration.
    pub(in crate::infrastructure::mypy) fn enter_proved_class(
        &mut self,
        definition: &StmtClassDef,
        source_sha256: &str,
        target: PythonTarget,
        parameters: BTreeMap<String, Option<Type>>,
        variables: types::Variables,
        explicit_bound: &[Type],
    ) {
        if let Some(receipt) = self.class_body_receipt(
            definition,
            source_sha256,
            target,
            &parameters,
            &variables,
            explicit_bound,
        ) {
            self.class_body_receipts.push(receipt);
        }
        self.enter_with_bound(
            Kind::Class,
            &definition.body,
            parameters,
            variables,
            explicit_bound,
        );
    }

    fn class_body_receipt(
        &self,
        definition: &StmtClassDef,
        expected_sha256: &str,
        expected_target: PythonTarget,
        parameters: &BTreeMap<String, Option<Type>>,
        variables: &types::Variables,
        explicit_bound: &[Type],
    ) -> Option<ClassBodyReceipt> {
        if self.stack.len() != 1 || self.current().kind != Kind::Module || !parameters.is_empty() {
            return None;
        }
        let (namespace, source_sha256, target) = self.context.class_source_identity(definition)?;
        if namespace != self.namespace
            || source_sha256 != expected_sha256
            || target != format!("{expected_target:?}")
            || target != format!("{:?}", self.context.module_target(&self.namespace))
            || self.modules.as_ref()?.namespace() != self.namespace
        {
            return None;
        }
        let binding = self.current().bindings.get(definition.name.as_str())?;
        let id = self.context.class_id(binding.ty?)?;
        let class = self.context.class(id);
        if binding.first_definition != definition.name.start()
            || binding.infer_from_assignment
            || binding.callable.is_some()
            || binding.module.is_some()
            || binding.ty != Some(class.object)
            || class.namespace != self.namespace
            || class.name != definition.name.as_str()
            || class.offset != definition.name.start().to_u32()
            || self.context.class_id(class.instance) != Some(id)
            || class.generic.as_slice() != explicit_bound
            || *variables != self.context.class_scope_variables(id, self.variables())
        {
            return None;
        }
        let enclosing = self.source_method_enclosing_bounds()?;
        let generic = explicit_bound.iter().copied().collect();
        let annotation_proof = self.class_method_annotation_proof(definition);
        let direct_methods = definition
            .body
            .iter()
            .filter_map(|statement| match statement {
                Stmt::FunctionDef(function) if annotation_proof(function, variables).is_ok() => {
                    Some((
                        function as *const StmtFunctionDef as usize,
                        (function.start().to_u32(), function.end().to_u32()),
                    ))
                }
                _ => None,
            })
            .collect();
        Some(ClassBodyReceipt {
            depth: self.stack.len(),
            namespace,
            source_sha256,
            target,
            class_id: id,
            class_name: class.name.clone(),
            class_offset: definition.name.start(),
            direct_methods,
            enclosing,
            generic,
        })
    }

    fn proved_class_enclosing(
        &self,
        function: &StmtFunctionDef,
    ) -> Option<std::collections::BTreeSet<Type>> {
        let receipt = self.class_body_receipts.last()?;
        if self.stack.len() != receipt.depth + 1
            || self.current().kind != Kind::Class
            || self.stack.first()?.kind != Kind::Module
            || self.namespace != receipt.namespace
            || self.modules.as_ref()?.namespace() != receipt.namespace
            || format!("{:?}", self.context.module_target(&self.namespace)) != receipt.target
            || receipt
                .direct_methods
                .get(&(function as *const StmtFunctionDef as usize))
                != Some(&(function.start().to_u32(), function.end().to_u32()))
            || !function
                .returns
                .as_deref()
                .and_then(|value| self.context.source_expression_identity(value))
                .is_some_and(|(namespace, source_sha256, target)| {
                    namespace == receipt.namespace
                        && source_sha256 == receipt.source_sha256
                        && target == receipt.target
                })
            || self.stack[0].bound_variables != receipt.enclosing
            || self.current().bound_variables != receipt.generic
        {
            return None;
        }
        let binding = self.stack[0].bindings.get(&receipt.class_name)?;
        let class = self.context.class(receipt.class_id);
        if binding.first_definition != receipt.class_offset
            || binding.infer_from_assignment
            || binding.callable.is_some()
            || binding.module.is_some()
            || binding.ty != Some(class.object)
            || class.namespace != receipt.namespace
            || class.name != receipt.class_name
            || class.offset != receipt.class_offset.to_u32()
            || class
                .generic
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                != receipt.generic
            || class
                .methods
                .get(function.name.as_str())
                .is_none_or(|method| {
                    method.signature.owner.as_deref() != Some(receipt.class_name.as_str())
                })
        {
            return None;
        }
        let mut enclosing = receipt.enclosing.clone();
        enclosing.extend(receipt.generic.iter().copied());
        Some(enclosing)
    }

    pub(super) fn annotation(&self, expression: &Expr) -> Option<Type> {
        self.context
            .annotation_with_variables(expression, &self.annotation_variables())
    }

    pub(in crate::infrastructure::mypy) fn annotation_variables(&self) -> types::Variables {
        let mut variables = self.annotation_variables_in(&self.current().variables);
        if !variables.contains_key("tuple") && !self.has_binding("tuple") {
            let target = self.context.module_target(&self.namespace);
            if let Some(marker) = self.context.builtin_constructor_alias("tuple", target) {
                variables.insert("tuple".into(), marker);
            }
        }
        variables
    }

    pub(in crate::infrastructure::mypy) fn annotation_variables_in(
        &self,
        namespace: &types::Variables,
    ) -> types::Variables {
        let mut variables = self.live_annotation_variables(namespace);
        if !variables.contains_key("dict") && !self.has_binding("dict") {
            let target = self.context.module_target(&self.namespace);
            if let Some(marker) = self.context.builtin_constructor_alias("dict", target) {
                variables.insert("dict".into(), marker);
            }
        }
        if !variables.contains_key("list") && !self.has_binding("list") {
            let target = self.context.module_target(&self.namespace);
            if let Some(marker) = self.context.builtin_constructor_alias("list", target) {
                variables.insert("list".into(), marker);
            }
        }
        variables
    }

    pub(in crate::infrastructure::mypy) fn function_binder_input(
        &self,
        function: &StmtFunctionDef,
        additional: &[Type],
    ) -> types::ReturnedCallableInput {
        let mut enclosing: std::collections::BTreeSet<_> = self
            .stack
            .iter()
            .flat_map(|scope| scope.bound_variables.iter().copied())
            .collect();
        enclosing.extend(additional.iter().copied());
        let class_enclosing = self.proved_class_enclosing(function);
        let enclosing_known = if let Some(bound) = class_enclosing {
            enclosing == bound
        } else {
            !self.stack.iter().any(|scope| scope.kind == Kind::Class)
        };
        self.context.returned_callable_input(
            function,
            &self.namespace,
            enclosing,
            enclosing_known,
            "scope",
            &|expression| self.canonical_callable_form(expression).is_some(),
        )
    }

    pub(in crate::infrastructure::mypy) fn returned_callable_gaps(&self) -> Vec<String> {
        self.context.returned_callable_gaps(&self.namespace)
    }

    // Taken once per source class population, before any method input is built.
    // Bound IDs are receipts from this live module frame, not all Variables.
    pub(super) fn source_method_enclosing_bounds(
        &self,
    ) -> Option<std::collections::BTreeSet<Type>> {
        if self.stack.len() != 1
            || self.current().kind != Kind::Module
            || self.modules.as_ref()?.namespace() != self.namespace
        {
            return None;
        }
        let mut enclosing = std::collections::BTreeSet::new();
        for &ty in &self.current().bound_variables {
            if !matches!(
                self.context.node(ty).as_deref(),
                Some(types::Node::Variable(_))
            ) {
                return None;
            }
            enclosing.insert(ty);
        }
        Some(enclosing)
    }

    // Prove only the class producer's actual Self replacement. This does not
    // allocate a variable or infer a class from a variable's name.
    fn source_method_self_type(
        &self,
        definition: &mamba_language::application::quality_syntax::python_ast::StmtClassDef,
        variables: &types::Variables,
        name: &str,
    ) -> Option<Type> {
        let scope = self.current();
        if self.stack.len() != 1
            || scope.kind != Kind::Module
            || self.modules.as_ref()?.namespace() != self.namespace
            || self.live_module_self_form(name) != Some(*scope.variables.get(name)?)
        {
            return None;
        }
        let binding = scope.bindings.get(definition.name.as_str())?;
        let id = self.context.class_id(binding.ty?)?;
        let class = self.context.class(id);
        if binding.first_definition != definition.name.start()
            || binding.infer_from_assignment
            || binding.callable.is_some()
            || binding.module.is_some()
            || binding.ty != Some(class.object)
            || class.namespace != self.namespace
            || class.offset != definition.name.start().to_u32()
            || self.context.class_id(class.instance) != Some(id)
        {
            return None;
        }
        let ty = *variables.get(name)?;
        if !self.context.is_source_self_variable(ty) {
            return None;
        }
        let node = self.context.node(ty)?;
        let types::Node::Variable(variable) = node.as_ref() else {
            return None;
        };
        if variable.parameter_spec || variable.type_var_tuple {
            return None;
        }
        self.context.node(variable.bound).and_then(|bound| {
            matches!(bound.as_ref(), types::Node::AppliedClass { id: owner, arguments, value: false }
                if *owner == id && arguments.as_slice() == class.generic.as_slice())
                .then_some(ty)
        })
    }

    pub(super) fn method_binder_input(
        &self,
        function: &StmtFunctionDef,
        owner: (
            u32,
            &mamba_language::application::quality_syntax::python_ast::StmtClassDef,
            &[Type],
        ),
        variables: &types::Variables,
        enclosing: Option<&std::collections::BTreeSet<Type>>,
        proof: &impl Fn(&StmtFunctionDef, &types::Variables) -> Result<(), (u32, &'static str)>,
    ) -> types::ReturnedCallableInput {
        let (id, definition, generic) = owner;
        let class = self.context.class(id);
        let binding = self.current().bindings.get(definition.name.as_str());
        let receipt = (|| {
            let reject = |reason| Err((function.start().to_u32(), reason));
            if self.stack.len() != 1 {
                return reject("method input is not in the sole module frame");
            }
            if self.current().kind != Kind::Module {
                return reject("method input owner is not a module");
            }
            if enclosing.is_none() {
                return reject("enclosing-bound producer returned no receipt");
            }
            if class.namespace != self.namespace {
                return reject("class namespace differs from the live module");
            }
            if class.offset != definition.name.start().to_u32() {
                return reject("class definition offset differs from its owner");
            }
            if self.context.class_id(class.instance) != Some(id) {
                return reject("class instance has a different declaration identity");
            }
            let Some(binding) = binding else {
                return reject("live class name binding is absent");
            };
            if binding.first_definition != definition.name.start() {
                return reject("live class binding has a different first definition");
            }
            if binding.ty != Some(class.object) {
                return reject("live class binding does not hold the class object");
            }
            if class.generic.as_slice() != generic {
                return reject("class generic IDs differ from the method input");
            }
            if !generic.iter().all(|&ty| {
                self.context.node(ty).is_some_and(|node| {
                    matches!(node.as_ref(), types::Node::Variable(variable)
                        if variables.get(&variable.name).copied() == Some(ty))
                })
            }) {
                return reject("class generic variable lacks its exact input-map identity");
            }
            proof(function, variables)
        })();
        if let Err(reason) = receipt {
            // Preserve the Context-only class input, including its real class
            // generic set. An absent receipt is never a proved empty scope.
            return self
                .context
                .returned_callable_input(
                    function,
                    &class.namespace,
                    generic.iter().copied().collect(),
                    false,
                    "class",
                    &|_| false,
                )
                .with_method_refusals(Some(reason), BTreeMap::new());
        }
        let mut bound = enclosing.unwrap().clone();
        bound.extend(generic.iter().copied());
        bound.extend(
            variables
                .keys()
                .filter_map(|name| self.source_method_self_type(definition, variables, name)),
        );
        let refusals = std::cell::RefCell::new(BTreeMap::new());
        let input = self.context.returned_callable_input(
            function,
            &class.namespace,
            bound,
            true,
            "class",
            &|expression| {
                let result = (|| {
                    let name = types::Context::annotation_form_name(expression)
                        .ok_or("Callable base has no ordinary annotation path")?;
                    let marker = variables
                        .get(&name)
                        .copied()
                        .ok_or("Callable marker is absent from the actual input map")?;
                    if !self.context.is_callable_form(marker) {
                        return Err("Callable input marker has a different role");
                    }
                    let root = name.split('.').next().unwrap();
                    if variables.get(root).is_some_and(|&ty| {
                        matches!(
                            self.context.node(ty).as_deref(),
                            Some(types::Node::Variable(_))
                        )
                    }) {
                        return Err("Callable root is a local type variable");
                    }
                    // The actual input must retain the exact live marker.
                    if self.canonical_callable_form(expression) != Some(marker) {
                        return Err("Callable input differs from its live canonical form");
                    }
                    Ok(())
                })();
                if let Err(reason) = result {
                    refusals
                        .borrow_mut()
                        .entry(expression as *const Expr as usize)
                        .or_insert((expression.start().to_u32(), reason));
                }
                result.is_ok()
            },
        );
        input.with_method_refusals(None, refusals.into_inner())
    }

    pub(super) fn refresh_function_annotations(
        &self,
        function: &StmtFunctionDef,
        signature: &Signature,
    ) -> Signature {
        let variables = self.annotation_variables();
        let fallback = (!self.current().variables.contains_key("dict"))
            .then(|| variables.get("dict").copied())
            .flatten();
        let tuple_fallback = (!self.current().variables.contains_key("tuple"))
            .then(|| variables.get("tuple").copied())
            .flatten();
        let binders = self.function_binder_input(function, &[]);
        let mut fresh =
            signature.refresh_annotations(function, &self.context, &variables, &binders);
        // The fallback is only an annotation input, never a runtime or inherited binding.
        // A function type parameter named dict replaces the fallback and must remain.
        if fallback.is_some() && fresh.variables.get("dict").copied() == fallback {
            fresh.variables.remove("dict");
        }
        // The tuple marker has the same temporary annotation-only lifetime.
        if tuple_fallback.is_some() && fresh.variables.get("tuple").copied() == tuple_fallback {
            fresh.variables.remove("tuple");
        }
        // Equal contents can share the input snapshot without changing bindings.
        if fresh.variables == signature.variables {
            fresh.variables = signature.variables.clone();
        }
        fresh
    }

    pub(super) fn variables(&self) -> types::Variables {
        self.live_annotation_variables(&self.current().variables)
    }

    pub(in crate::infrastructure::mypy) fn bind_type_variables(&mut self, variables: &[Type]) {
        self.stack
            .last_mut()
            .unwrap()
            .bound_variables
            .extend(variables.iter().copied());
    }

    pub(in crate::infrastructure::mypy) fn type_variable_bound(&self, ty: Type) -> bool {
        self.stack
            .iter()
            .any(|scope| scope.bound_variables.contains(&ty))
    }

    pub(in crate::infrastructure::mypy) fn has_binding(&self, name: &str) -> bool {
        self.resolved_binding(name).is_some()
    }

    pub(in crate::infrastructure::mypy) fn imported_dynamic_type(&self, name: &str) -> bool {
        self.resolved_binding(name)
            .is_some_and(|binding| binding.ty.is_some_and(|ty| ty.is_any()))
            && self
                .stack
                .iter()
                .any(|scope| scope.imported_names.contains(name))
    }

    pub(in crate::infrastructure::mypy) fn annotation_scope(
        &mut self,
        variables: types::Variables,
        bound: std::collections::BTreeSet<Type>,
    ) -> (types::Variables, std::collections::BTreeSet<Type>) {
        let scope = self.stack.last_mut().unwrap();
        (
            std::mem::replace(&mut scope.variables, variables),
            std::mem::replace(&mut scope.bound_variables, bound),
        )
    }

    pub(super) fn environment(&self) -> BTreeMap<String, Type> {
        let mut environment = BTreeMap::new();
        for (index, scope) in self.stack.iter().enumerate() {
            // A class namespace is visible in its own suite, not in nested scopes.
            if scope.kind == Kind::Class && index + 1 != self.stack.len() {
                continue;
            }
            for (name, binding) in &scope.bindings {
                if let Some(ty) = binding.ty {
                    environment.insert(name.clone(), ty);
                } else {
                    environment.remove(name);
                }
            }
            for (name, &target) in &scope.redirects {
                if let Some(ty) = self.stack[target]
                    .bindings
                    .get(name)
                    .and_then(|binding| binding.ty)
                {
                    environment.insert(name.clone(), ty);
                } else {
                    environment.remove(name);
                }
            }
        }
        environment
    }

    pub(super) fn bind(&mut self, target: &Expr, ty: Option<Type>) {
        self.add_binding(target, ty, true);
    }

    pub(super) fn assignment_bindings<'a>(
        &self,
        target: &'a Expr,
        ty: Type,
    ) -> Option<Vec<(&'a Expr, Type)>> {
        if matches!(target, Expr::Name(_)) {
            return Some(vec![(target, ty)]);
        }
        let targets: &[Expr] = match target {
            Expr::Tuple(tuple) => tuple.elts.as_ref(),
            Expr::List(list) => list.elts.as_ref(),
            _ => return None,
        };
        if let Some(types::Node::Union(items)) = self.context.node(ty).as_deref() {
            let mut arms = items
                .iter()
                .map(|&item| self.assignment_bindings(target, item));
            let mut bindings = arms.next()??;
            for arm in arms {
                let arm = arm?;
                if bindings.len() != arm.len() {
                    return None;
                }
                for ((leaf, combined), (other, item)) in bindings.iter_mut().zip(arm) {
                    if !std::ptr::eq(*leaf, other) {
                        return None;
                    }
                    *combined = self.context.union(vec![*combined, item]);
                }
            }
            return Some(bindings);
        }
        if matches!(target, Expr::Tuple(_))
            && matches!(
                self.context.node(ty).as_deref(),
                Some(types::Node::Container(types::Container::Generator, _))
            )
        {
            return self.generator_assignment_bindings(targets, ty);
        }
        let items = match self.context.node(ty).as_deref() {
            Some(types::Node::Tuple(items)) => {
                if items.len() != targets.len() || self.context.tuple_pack_position(items).is_some()
                {
                    return None;
                }
                items.clone()
            }
            Some(types::Node::VariadicTuple(item)) => vec![*item; targets.len()],
            Some(types::Node::Container(types::Container::List, items)) => {
                vec![*items.first()?; targets.len()]
            }
            _ if ty.is_any() => vec![Type::Any; targets.len()],
            _ => return None,
        };
        let mut bindings = Vec::new();
        for (target, ty) in targets.iter().zip(items) {
            bindings.extend(self.assignment_bindings(target, ty)?);
        }
        Some(bindings)
    }

    pub(super) fn declare(&mut self, target: &Expr, ty: Option<Type>) {
        self.add_binding(target, ty, false);
    }

    pub(super) fn assigned_type(&self, target: &Expr) -> Option<Type> {
        let Expr::Name(name) = target else {
            return None;
        };
        let binding = self.binding(name.id.as_str())?;
        if binding.infer_from_assignment
            && (binding.first_definition == name.start() || self.earlier_local_assignment(name))
            && !self.replayed_loop_definition(name.id.as_str(), name.start())
        {
            None
        } else {
            binding.ty
        }
    }

    pub(super) fn before_definition(&self, name: &str, location: TextSize) -> bool {
        if let Some(&assigned) = self.current().walrus_assigned.get(name) {
            return !assigned;
        }
        self.current()
            .bindings
            .get(name)
            .is_some_and(|binding| location < binding.first_definition)
    }

    pub(super) fn callable(&self, name: &str) -> Option<&Arc<Signature>> {
        self.resolved_binding(name)?.callable.as_ref()
    }

    pub(super) fn declared_callable(&self, expression: &Expr) -> bool {
        let Expr::Name(name) = expression else {
            return false;
        };
        self.resolved_binding(name.id.as_str())
            .is_some_and(|binding| {
                !binding.infer_from_assignment
                    && binding
                        .ty
                        .and_then(|ty| self.context.node(ty))
                        .is_some_and(|node| matches!(node.as_ref(), types::Node::Callable(_)))
            })
    }

    pub(super) fn remember_callable(&mut self, target: &Expr, value: &Expr) {
        if !matches!(target, Expr::Name(_)) {
            return;
        }
        let callable = self.assignment_callable(value);
        self.remember_assigned_callable(target, callable);
    }

    pub(super) fn assignment_callable(&self, value: &Expr) -> Option<Arc<Signature>> {
        self.callable_expression(value)
            .map(|signature| Arc::new(signature.anonymous()))
    }

    pub(super) fn remember_assigned_callable(
        &mut self,
        target: &Expr,
        callable: Option<Arc<Signature>>,
    ) {
        let Expr::Name(name) = target else {
            return;
        };
        if let Some(binding) = self.binding_mut(name.id.as_str()) {
            if binding.infer_from_assignment {
                binding.callable = callable;
                binding.annotation_form = None;
                binding.annotation_module = None;
            }
        }
    }

    fn current(&self) -> &Scope {
        self.stack.last().expect("module scope")
    }

    // Valid only while this live lexical scope is entered. No persistent ID.
    pub(in crate::infrastructure::mypy) fn try_input_scope(&self) -> (usize, Kind, Option<usize>) {
        let scope = self.current();
        (self.stack.len() - 1, scope.kind, scope.comprehension_owner)
    }

    fn binding_scope(&self, name: &str) -> usize {
        self.current()
            .redirects
            .get(name)
            .copied()
            .unwrap_or(self.stack.len() - 1)
    }

    pub(in crate::infrastructure::mypy) fn first_definition(&self, name: &str) -> Option<TextSize> {
        self.binding(name).map(|binding| binding.first_definition)
    }

    fn binding(&self, name: &str) -> Option<&Binding> {
        self.stack[self.binding_scope(name)].bindings.get(name)
    }

    fn binding_mut(&mut self, name: &str) -> Option<&mut Binding> {
        let index = self.binding_scope(name);
        self.stack[index].bindings.get_mut(name)
    }
}
