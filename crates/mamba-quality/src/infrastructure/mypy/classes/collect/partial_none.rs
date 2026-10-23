//! Real dynamic constructor exits settle reached, owned None declarations.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{visitor::walk_stmt, ExprAttribute};

#[derive(Clone, Copy)]
pub(super) struct Method<'a> {
    pub(super) id: u32,
    pub(super) definition: &'a StmtClassDef,
    pub(super) function: &'a StmtFunctionDef,
    pub(super) signature: &'a Signature,
    pub(super) kind: MethodKind,
}

#[derive(Clone, PartialEq, Eq)]
pub(in crate::infrastructure::mypy) struct Source {
    pub(in crate::infrastructure::mypy) namespace: String,
    pub(in crate::infrastructure::mypy) sha256: String,
    pub(in crate::infrastructure::mypy) target: String,
}

#[derive(Clone)]
struct Completed {
    source: Source,
    member: String,
    declaration: TextSize,
    ty: Type,
}

#[derive(Clone)]
pub(in crate::infrastructure::mypy) struct State {
    ast: String,
    metadata: CollectionMetadata,
    source: Option<Source>,
    allowed: bool,
    generation: u64,
    active: bool,
    blocked: bool,
    completed: Option<Completed>,
}

#[derive(Clone)]
pub(in crate::infrastructure::mypy) struct Plan {
    pub(in crate::infrastructure::mypy) id: u32,
    pub(in crate::infrastructure::mypy) receiver: String,
    pub(in crate::infrastructure::mypy) instance: Type,
    pub(in crate::infrastructure::mypy) function: usize,
    source: Source,
    class_offset: u32,
    constructor: TextSize,
    generation: u64,
    member: String,
    declaration: TextSize,
    target: usize,
    value: usize,
}

pub(in crate::infrastructure::mypy) struct Frame {
    pub(in crate::infrastructure::mypy) plan: Plan,
    pub(in crate::infrastructure::mypy) reached: bool,
}

fn source(context: &Context, definition: &StmtClassDef) -> Option<Source> {
    let (namespace, sha256, target) = context.class_source_identity(definition)?;
    Some(Source {
        namespace,
        sha256,
        target,
    })
}

pub(super) fn gap(context: &Context, id: u32, reason: &str) {
    let class = context.class(id);
    context
        .classes
        .borrow_mut()
        .constructor_none_gaps
        .insert(format!(
            "constructor None lifecycle is unsupported for {} at {}: {reason}",
            class.namespace, class.offset,
        ));
}

// Collection publishes facts only. An identical completed generation may restore
// its checked final value; it never creates a new body-exit receipt.
pub(super) fn collected(
    context: &Context,
    id: u32,
    definition: &StmtClassDef,
    metadata: &CollectionMetadata,
    class: &mut Class,
    allowed: bool,
) {
    let previous = context.classes.borrow().constructor_none.get(&id).cloned();
    let allowed = allowed
        && !context.module_is_stub(&class.namespace)
        && class.generic.is_empty()
        && class.methods.get("__init__").is_some_and(|method| {
            method.kind == MethodKind::Instance
                && method.signature.implicit
                && !method.signature.ignored
        })
        && class
            .fields
            .values()
            .any(|field| field.ty == Some(Type::None) && eligible(field));
    // Most declarations cannot own this partial. Do not retain their full ASTs.
    // Existing states remain as generation tombstones when admission changes.
    if previous.is_none() && !allowed {
        return;
    }
    let ast = format!("{definition:?}");
    let current_source = source(context, definition);
    let mut state = State {
        ast,
        metadata: metadata.clone(),
        source: current_source.clone(),
        allowed,
        generation: previous.as_ref().map_or(0, |state| {
            state
                .generation
                .checked_add(1)
                .expect("constructor declaration generation fits u64")
        }),
        active: false,
        blocked: false,
        completed: None,
    };
    if let Some(previous) = previous {
        let identical = previous.ast == state.ast
            && previous.metadata == state.metadata
            && previous.allowed == allowed
            && current_source.as_ref().is_none_or(|current| {
                previous
                    .source
                    .as_ref()
                    .is_none_or(|prior| prior == current)
            });
        if identical && previous.completed.is_none() {
            state.generation = previous.generation;
            state.active = previous.active;
            state.blocked = previous.blocked;
            state.source = current_source.or(previous.source);
        } else if identical {
            let completed = previous.completed.as_ref().unwrap();
            let field = class.fields.get_mut(&completed.member);
            if current_source.as_ref() == Some(&completed.source)
                && metadata.first_field_declarations.get(&completed.member)
                    == Some(&completed.declaration)
                && !metadata.annotated_fields.contains(&completed.member)
                && field
                    .as_ref()
                    .is_some_and(|field| eligible(field) && matches!(field.ty, Some(Type::None)))
            {
                field.unwrap().ty = Some(completed.ty);
                state = previous;
            } else {
                state.blocked = true;
                gap(
                    context,
                    id,
                    "completed generation cannot be revalidated during refresh",
                );
            }
        } else if previous.active || previous.completed.is_some() {
            state.blocked = true;
            gap(
                context,
                id,
                "collection changed an active or completed declaration generation",
            );
        }
    }
    context
        .classes
        .borrow_mut()
        .constructor_none
        .insert(id, state);
}

pub(super) fn admit(
    context: &Context,
    id: u32,
    definition: &StmtClassDef,
    sha256: &str,
) -> Option<Plan> {
    let class = context.class(id);
    let state = context.classes.borrow().constructor_none.get(&id)?.clone();
    let source = Source {
        namespace: class.namespace.clone(),
        sha256: sha256.to_owned(),
        target: format!("{:?}", context.module_target(&class.namespace)),
    };
    if !state.allowed || state.blocked || state.ast != format!("{definition:?}") {
        return None;
    }
    if state.completed.is_some() {
        gap(
            context,
            id,
            "a repeated class check has no constructor retry policy",
        );
        return None;
    }
    if state.source.as_ref().is_some_and(|prior| prior != &source) {
        gap(
            context,
            id,
            "actual checker source does not match declaration facts",
        );
        return None;
    }
    let function = definition
        .body
        .iter()
        .find_map(|statement| match statement {
            Stmt::FunctionDef(function) if function.name.as_str() == "__init__" => Some(function),
            _ => None,
        })?;
    if state.metadata.constructor_offset != Some(function.name.start())
        || Signature::is_generator(function)
    {
        return None;
    }
    let method = class.methods.get("__init__")?;
    let receiver = receiver(
        context,
        &class,
        Method {
            id,
            definition,
            function,
            signature: &method.signature,
            kind: method.kind,
        },
    )?;
    let mut candidates = function.body.iter().filter_map(|statement| {
        let Stmt::Assign(assignment) = statement else {
            return None;
        };
        let [Expr::Attribute(attribute)] = assignment.targets.as_slice() else {
            return None;
        };
        (direct(attribute, receiver) && matches!(assignment.value.as_ref(), Expr::NoneLiteral(_)))
            .then_some((attribute, assignment.value.as_ref()))
    });
    let (attribute, value) = candidates.next()?;
    if candidates.next().is_some() || !plain_class(definition, attribute.attr.as_str()) {
        return None;
    }
    let name = attribute.attr.as_str();
    if state.metadata.annotated_fields.contains(name)
        || state.metadata.first_field_declarations.get(name) != Some(&attribute.start())
        || !class
            .fields
            .get(name)
            .is_some_and(|field| field.ty == Some(Type::None) && eligible(field))
        || !owned_targets(
            function,
            receiver,
            name,
            attribute.start(),
            &class,
            &state.metadata,
        )
    {
        return None;
    }
    let plan = Plan {
        id,
        receiver: receiver.to_owned(),
        instance: class.instance,
        function: function as *const StmtFunctionDef as usize,
        source: source.clone(),
        class_offset: class.offset,
        constructor: function.name.start(),
        generation: state.generation,
        member: name.to_owned(),
        declaration: attribute.start(),
        target: attribute as *const ExprAttribute as usize,
        value: value as *const Expr as usize,
    };
    context
        .classes
        .borrow_mut()
        .constructor_none
        .get_mut(&id)?
        .source = Some(source);
    if function
        .body
        .first()
        .is_some_and(|statement| statement.start() < attribute.start())
    {
        gap(
            context,
            id,
            "pre-declaration checks are not covered by a deferred-body policy",
        );
    }
    Some(plan)
}

pub(super) fn begin(context: &Context, plan: Plan) -> Option<Frame> {
    let ready = context
        .classes
        .borrow()
        .constructor_none
        .get(&plan.id)
        .is_some_and(|state| {
            state.generation == plan.generation
                && state.source.as_ref() == Some(&plan.source)
                && !state.blocked
                && !state.active
                && state.completed.is_none()
        });
    if !ready {
        gap(
            context,
            plan.id,
            "a repeated or changed constructor attempt has no retry policy",
        );
        return None;
    }
    context
        .classes
        .borrow_mut()
        .constructor_none
        .get_mut(&plan.id)?
        .active = true;
    Some(Frame {
        plan,
        reached: false,
    })
}

pub(super) fn reached(
    context: &Context,
    frame: &mut Frame,
    assignment: &mamba_language::application::quality_syntax::python_ast::StmtAssign,
    actual: Option<Type>,
) {
    let [Expr::Attribute(attribute)] = assignment.targets.as_slice() else {
        return;
    };
    if attribute as *const ExprAttribute as usize != frame.plan.target
        || assignment.value.as_ref() as *const Expr as usize != frame.plan.value
    {
        return;
    }
    frame.reached =
        actual == Some(Type::None) && matches!(assignment.value.as_ref(), Expr::NoneLiteral(_));
    if !frame.reached {
        gap(
            context,
            frame.plan.id,
            "the reached declaration did not produce real None",
        );
    }
}

pub(super) fn finish(context: &Context, frame: Frame) {
    let plan = &frame.plan;
    let valid = context
        .classes
        .borrow()
        .constructor_none
        .get(&plan.id)
        .is_some_and(|state| {
            state.generation == plan.generation
                && state.source.as_ref() == Some(&plan.source)
                && state.active
                && !state.blocked
                && state.completed.is_none()
                && state.metadata.constructor_offset == Some(plan.constructor)
                && state.metadata.first_field_declarations.get(&plan.member)
                    == Some(&plan.declaration)
                && !state.metadata.annotated_fields.contains(&plan.member)
        });
    let mut class = (*context.class(plan.id)).clone();
    if !valid
        || !frame.reached
        || class.namespace != plan.source.namespace
        || class.offset != plan.class_offset
        || class.instance != plan.instance
        || !class
            .fields
            .get(&plan.member)
            .is_some_and(|field| field.ty == Some(Type::None) && eligible(field))
    {
        if let Some(state) = context
            .classes
            .borrow_mut()
            .constructor_none
            .get_mut(&plan.id)
        {
            state.active = false;
            state.blocked = true;
        }
        gap(
            context,
            plan.id,
            "function exit lacks an unchanged reached None declaration",
        );
        return;
    }
    let ty = context.union(vec![Type::Any, Type::None]);
    class.fields.get_mut(&plan.member).unwrap().ty = Some(ty);
    context.replace_class(plan.id, class);
    let mut registry = context.classes.borrow_mut();
    let state = registry.constructor_none.get_mut(&plan.id).unwrap();
    state.active = false;
    state.completed = Some(Completed {
        source: plan.source.clone(),
        member: plan.member.clone(),
        declaration: plan.declaration,
        ty,
    });
}

pub(super) fn before_other_method(context: &Context, plan: &Plan) {
    if context
        .classes
        .borrow()
        .constructor_none
        .get(&plan.id)
        .is_some_and(|state| state.completed.is_none())
    {
        gap(
            context,
            plan.id,
            "a method precedes the constructor exit; consumer deferral is unavailable",
        );
    }
}

fn owned_targets(
    function: &StmtFunctionDef,
    receiver: &str,
    candidate: &str,
    declaration: TextSize,
    class: &Class,
    metadata: &CollectionMetadata,
) -> bool {
    fn targets<'a>(
        expression: &'a Expr,
        receiver: &str,
        output: &mut Vec<&'a ExprAttribute>,
    ) -> bool {
        match expression {
            Expr::Attribute(attribute) if direct(attribute, receiver) => {
                output.push(attribute);
                true
            }
            Expr::Tuple(tuple) => tuple
                .elts
                .iter()
                .all(|item| targets(item, receiver, output)),
            Expr::List(list) => list.elts.iter().all(|item| targets(item, receiver, output)),
            _ => false,
        }
    }
    let mut prior = BTreeMap::new();
    for statement in &function.body {
        let Stmt::Assign(assignment) = statement else {
            return false;
        };
        let mut assigned = Vec::new();
        if !assignment
            .targets
            .iter()
            .all(|target| targets(target, receiver, &mut assigned))
            || assigned.is_empty()
        {
            return false;
        }
        for target in assigned {
            let name = target.attr.as_str();
            let first = *prior.entry(name).or_insert(target.start());
            if name.starts_with("__")
                || class.methods.contains_key(name)
                || metadata.first_field_declarations.get(name) != Some(&first)
                || metadata.annotated_fields.contains(name)
                || !class.fields.get(name).is_some_and(eligible)
                || (name == candidate && target.start() != declaration)
            {
                return false;
            }
        }
    }
    true
}

fn receiver<'a>(context: &Context, class: &Class, method: Method<'a>) -> Option<&'a str> {
    let Method {
        id,
        definition,
        function,
        signature,
        kind,
    } = method;
    if kind != MethodKind::Instance
        || function.name.as_str() != "__init__"
        || function.is_async
        || !function.decorator_list.is_empty()
        || function.type_params.is_some()
        || function.returns.is_some()
        || !signature.implicit
        || signature.ignored
        || !signature.generic.is_empty()
        || !signature.overloads.is_empty()
        || signature.owner.as_deref() != Some(class.name.as_str())
        || function.parameters.vararg.is_some()
        || function.parameters.kwarg.is_some()
        || function
            .parameters
            .iter()
            .any(|parameter| parameter.annotation().is_some())
        || function
            .parameters
            .posonlyargs
            .iter()
            .chain(&function.parameters.args)
            .chain(&function.parameters.kwonlyargs)
            .any(|parameter| parameter.default.is_some())
        || !definition.decorator_list.is_empty()
        || definition.type_params.is_some()
        || definition
            .arguments
            .as_ref()
            .is_some_and(|arguments| !arguments.args.is_empty() || !arguments.keywords.is_empty())
        || definition.name.start().to_u32() != class.offset
        || context.class_id(class.instance) != Some(id)
        || context.module_is_stub(&class.namespace)
        || !class.generic.is_empty()
        || !class.scalar_bases.is_empty()
        || !class.any_base_origins.is_empty()
        || class.plugin
        || class.protocol
        || class.typed_dict.is_some()
        || class.pydantic.is_some()
        || class.dataclass.is_some()
    {
        return None;
    }
    // Population inserts this canonical object base for a class with no bases.
    // The explicit-source-base guard above ran first, so this ID already exists.
    let object = context.builtin_class(Type::Object);
    let object_instance = context.class(object).instance;
    if class.bases.as_slice() != std::slice::from_ref(&object)
        || class.base_types.as_slice() != std::slice::from_ref(&object_instance)
    {
        return None;
    }
    let (Some(receiver), Some(first)) = (
        function.parameters.iter().next(),
        signature.parameters.first(),
    ) else {
        return None;
    };
    let receiver = receiver.name().as_str();
    if first.name != receiver
        || first.ty != Some(class.instance)
        || first.ty.and_then(|ty| context.class_id(ty)) != Some(id)
        || signature.parameters.iter().skip(1).any(|parameter| {
            parameter.ty.is_some_and(|ty| !ty.is_any()) || parameter.name == receiver
        })
    {
        return None;
    }
    Some(receiver)
}

fn direct(attribute: &ExprAttribute, receiver: &str) -> bool {
    matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == receiver)
}

fn eligible(field: &Field) -> bool {
    !field.needs_annotation
        && !field.class_access
        && !field.readonly
        && !field.final_attribute
        && !field.class_var
        && !field.abstract_field
}

// No class namespace operation may replace the constructor or change attribute
// lookup. All method signatures are checked separately by normal collection.
fn plain_class(definition: &StmtClassDef, terminal: &str) -> bool {
    let mut constructors = 0;
    for statement in &definition.body {
        match statement {
            Stmt::FunctionDef(function) => {
                let name = function.name.as_str();
                if name == "__init__" {
                    constructors += 1;
                }
                if name == terminal
                    || matches!(
                        name,
                        "__getattr__" | "__getattribute__" | "__setattr__" | "__delattr__"
                    )
                {
                    return false;
                }
            }
            Stmt::Assign(assignment) => {
                if !assignment.targets.iter().all(|target| {
                    matches!(target, Expr::Name(name)
                        if name.id.as_str() != "__init__" && !name.id.as_str().starts_with("__"))
                }) || !literal(&assignment.value)
                {
                    return false;
                }
            }
            Stmt::Expr(expression)
                if matches!(expression.value.as_ref(), Expr::StringLiteral(_)) => {}
            _ => return false,
        }
    }
    struct Annotations<'a> {
        terminal: &'a str,
        found: bool,
    }
    impl<'a> Visitor<'a> for Annotations<'_> {
        fn visit_stmt(&mut self, statement: &'a Stmt) {
            if let Stmt::AnnAssign(assignment) = statement {
                if matches!(assignment.target.as_ref(), Expr::Attribute(attribute)
                    if attribute.attr.as_str() == self.terminal)
                {
                    self.found = true;
                }
            }
            // A nested declaration could annotate a captured receiver. Its
            // ownership is outside this proof, so reject instead of skipping it.
            if matches!(statement, Stmt::ClassDef(_) | Stmt::FunctionDef(_)) {
                self.found = true;
                return;
            }
            walk_stmt(self, statement);
        }
    }
    let mut annotations = Annotations {
        terminal,
        found: false,
    };
    for statement in &definition.body {
        if let Stmt::FunctionDef(function) = statement {
            annotations.visit_body(&function.body);
        }
    }
    constructors == 1 && !annotations.found
}

fn literal(expression: &Expr) -> bool {
    match expression {
        Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NumberLiteral(_) => true,
        Expr::Tuple(tuple) => tuple.elts.iter().all(literal),
        Expr::List(list) => list.elts.iter().all(literal),
        Expr::Set(set) => set.elts.iter().all(literal),
        Expr::Dict(dictionary) => dictionary
            .items
            .iter()
            .all(|item| item.key.as_ref().is_some_and(literal) && literal(&item.value)),
        _ => false,
    }
}
