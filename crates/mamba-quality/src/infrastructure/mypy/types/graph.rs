//! Interned structural types share nodes across one workspace analysis.
use super::Type;
mod nominal_joins;
mod subtypes;
mod union_joins;
use std::{cell::RefCell, collections::HashMap, sync::Arc};

#[derive(Clone, PartialEq, Eq, Hash)]
struct SourceDeclarationIdentity {
    namespace: String,
    source_sha256: String,
    python_target: String,
}

struct SourceDeclarationOwner {
    identity: SourceDeclarationIdentity,
    // Addresses only prove that a site belongs to this live original AST.
    // They are never part of the durable declaration key.
    sites: std::collections::BTreeSet<usize>,
    class_sites: std::collections::BTreeSet<usize>,
}

#[derive(PartialEq, Eq, Hash)]
struct SourceVariableKey {
    source: SourceDeclarationIdentity,
    offset: u32,
    role: &'static str,
    name: String,
}

pub(in crate::infrastructure::mypy) struct SourceDeclarationGuard {
    context: std::rc::Rc<Context>,
    previous: Option<SourceDeclarationOwner>,
}

impl Drop for SourceDeclarationGuard {
    fn drop(&mut self) {
        self.context
            .source_declaration_owner
            .replace(self.previous.take());
    }
}

#[derive(Default)]
struct SourceDeclarationSites(
    std::collections::BTreeSet<usize>,
    std::collections::BTreeSet<usize>,
);

impl<'a> mamba_language::application::quality_syntax::python_ast::visitor::Visitor<'a>
    for SourceDeclarationSites
{
    fn visit_stmt(
        &mut self,
        statement: &'a mamba_language::application::quality_syntax::python_ast::Stmt,
    ) {
        use mamba_language::application::quality_syntax::python_ast::{
            visitor::walk_stmt, Stmt, TypeParam,
        };
        if let Stmt::ClassDef(class) = statement {
            self.1.insert(class as *const _ as usize);
        }
        if let Stmt::FunctionDef(function) = statement {
            for parameter in function
                .type_params
                .iter()
                .flat_map(|parameters| &parameters.type_params)
            {
                let name = match parameter {
                    TypeParam::TypeVar(parameter) => &parameter.name,
                    TypeParam::ParamSpec(parameter) => &parameter.name,
                    TypeParam::TypeVarTuple(parameter) => &parameter.name,
                };
                self.0.insert(name as *const _ as usize);
            }
        }
        walk_stmt(self, statement);
    }

    fn visit_expr(
        &mut self,
        expression: &'a mamba_language::application::quality_syntax::python_ast::Expr,
    ) {
        use mamba_language::application::quality_syntax::python_ast::{visitor::walk_expr, Expr};
        match expression {
            Expr::Name(name) => {
                self.0.insert(name as *const _ as usize);
            }
            Expr::Call(call) => {
                self.0.insert(call as *const _ as usize);
            }
            Expr::Compare(compare) => {
                self.0.insert(compare as *const _ as usize);
            }
            _ => {}
        }
        self.0.insert(expression as *const _ as usize);
        walk_expr(self, expression);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Container {
    List,
    Set,
    FrozenSet,
    Dict,
    Sequence,
    Iterable,
    Mapping,
    DictKeys,
    DictValues,
    DictItems,
    TypeObject,
    Awaitable,
    Coroutine,
    Generator,
    Iterator,
    AsyncGenerator,
    AsyncIterator,
    AsyncIterable,
}

impl Container {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Set => "set",
            Self::FrozenSet => "frozenset",
            Self::Dict => "dict",
            Self::Sequence => "Sequence",
            Self::Iterable => "Iterable",
            Self::Mapping => "Mapping",
            Self::DictKeys => "dict_keys",
            Self::DictValues => "dict_values",
            Self::DictItems => "dict_items",
            Self::TypeObject => "type",
            Self::Awaitable => "Awaitable",
            Self::Coroutine => "Coroutine",
            Self::Generator => "Generator",
            Self::Iterator => "Iterator",
            Self::AsyncGenerator => "AsyncGenerator",
            Self::AsyncIterator => "AsyncIterator",
            Self::AsyncIterable => "AsyncIterable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Node {
    Deleted(String),
    Container(Container, Vec<Type>),
    Tuple(Vec<Type>),
    VariadicTuple(Type),
    Unpack(Type),
    Union(Vec<Type>),
    Variable(super::variables::Variable),
    ParamSpecArgs(Type),
    ParamSpecKwargs(Type),
    Parameters(Vec<super::super::signatures::Parameter>, bool),
    Instance(u32),
    Class(u32),
    AppliedClass {
        id: u32,
        arguments: Vec<Type>,
        value: bool,
    },
    Callable(Arc<super::super::signatures::Signature>),
    Literal(super::literals::Literal),
    LiteralForm,
    TypingForm(String),
    Alias(u32, Option<Type>),
    AliasInstance(u32, Vec<Type>),
    InferredAlias {
        id: u32,
        arguments: Vec<Type>,
        target: Type,
    },
    LiteralValue(super::literals::Literal),
}

#[derive(Default)]
struct Nodes {
    values: Vec<Arc<Node>>,
    index: HashMap<Arc<Node>, u32>,
    next_variable: u32,
    native_variables: HashMap<(&'static str, u32), u32>,
    source_variables: HashMap<SourceVariableKey, u32>,
}

#[derive(Default)]
pub(crate) struct Context {
    nodes: RefCell<Nodes>,
    pub(in crate::infrastructure::mypy) classes: RefCell<super::super::classes::Registry>,
    pub(super) aliases: RefCell<super::aliases::Registry>,
    pub(super) observed_expression_types: RefCell<Option<super::ObservedExpressionTypes>>,
    source_declaration_owner: RefCell<Option<SourceDeclarationOwner>>,
    pub(super) returned_callable_binders: RefCell<super::returned_callable_binders::State>,
    pub(super) unsupported_any_origin: std::cell::Cell<bool>,
}

impl Context {
    pub(in crate::infrastructure::mypy) fn class_source_identity(
        &self,
        declaration: &impl ruff_text_size::Ranged,
    ) -> Option<(String, String, String)> {
        self.source_declaration_owner
            .borrow()
            .as_ref()
            .filter(|owner| {
                owner
                    .class_sites
                    .contains(&(declaration as *const _ as usize))
            })
            .map(|owner| {
                (
                    owner.identity.namespace.clone(),
                    owner.identity.source_sha256.clone(),
                    owner.identity.python_target.clone(),
                )
            })
    }

    pub(in crate::infrastructure::mypy) fn source_declaration_site(
        &self,
        declaration: &impl ruff_text_size::Ranged,
    ) -> bool {
        self.source_declaration_owner
            .borrow()
            .as_ref()
            .is_some_and(|owner| owner.sites.contains(&(declaration as *const _ as usize)))
    }

    pub(in crate::infrastructure::mypy) fn source_expression_identity(
        &self,
        expression: &mamba_language::application::quality_syntax::python_ast::Expr,
    ) -> Option<(String, String, String)> {
        self.source_declaration_owner
            .borrow()
            .as_ref()
            .filter(|owner| owner.sites.contains(&(expression as *const _ as usize)))
            .map(|owner| {
                (
                    owner.identity.namespace.clone(),
                    owner.identity.source_sha256.clone(),
                    owner.identity.python_target.clone(),
                )
            })
    }

    pub(in crate::infrastructure::mypy) fn source_declarations(
        self: &std::rc::Rc<Self>,
        namespace: &str,
        source_sha256: &str,
        python_target: mamba_language::application::quality_syntax::PythonTarget,
        body: &[mamba_language::application::quality_syntax::python_ast::Stmt],
        condition: Option<&mamba_language::application::quality_syntax::python_ast::Expr>,
    ) -> SourceDeclarationGuard {
        use mamba_language::application::quality_syntax::python_ast::visitor::Visitor;
        let mut sites = SourceDeclarationSites::default();
        for statement in body {
            sites.visit_stmt(statement);
        }
        if let Some(condition) = condition {
            sites.visit_expr(condition);
        }
        let owner = SourceDeclarationOwner {
            identity: SourceDeclarationIdentity {
                namespace: namespace.into(),
                source_sha256: source_sha256.into(),
                python_target: format!("{python_target:?}"),
            },
            sites: sites.0,
            class_sites: sites.1,
        };
        SourceDeclarationGuard {
            context: self.clone(),
            previous: self.source_declaration_owner.replace(Some(owner)),
        }
    }

    pub(in crate::infrastructure::mypy) fn source_variable_id(
        &self,
        declaration: &impl ruff_text_size::Ranged,
        role: &'static str,
        name: &str,
    ) -> u32 {
        let source = self
            .source_declaration_owner
            .borrow()
            .as_ref()
            .filter(|owner| owner.sites.contains(&(declaration as *const _ as usize)))
            .map(|owner| owner.identity.clone());
        let Some(source) = source else {
            // Synthetic and foreign ASTs retain the ordinary fresh-binder path.
            return self.variable_id();
        };
        let key = SourceVariableKey {
            source,
            offset: declaration.start().to_u32(),
            role,
            name: name.into(),
        };
        if let Some(&id) = self.nodes.borrow().source_variables.get(&key) {
            return id;
        }
        let id = self.variable_id();
        self.nodes.borrow_mut().source_variables.insert(key, id);
        id
    }

    pub(super) fn variable_id(&self) -> u32 {
        let mut nodes = self.nodes.borrow_mut();
        let id = nodes.next_variable;
        nodes.next_variable = id.checked_add(1).expect("type variable ID fits in u32");
        id
    }

    pub(in crate::infrastructure::mypy) fn native_variable(
        &self,
        symbol: &'static str,
        position: u32,
        name: String,
        bound: Type,
    ) -> Type {
        let key = (symbol, position);
        let previous = self.nodes.borrow().native_variables.get(&key).copied();
        let id = previous.unwrap_or_else(|| {
            let id = self.variable_id();
            self.nodes.borrow_mut().native_variables.insert(key, id);
            id
        });
        // Reuse declaration identity, not the current bound or other node metadata.
        self.intern(Node::Variable(super::variables::Variable {
            id,
            name,
            bound,
            values: Vec::new(),
            default: None,
            variance: super::variables::ParameterVariance::Invariant,
            parameter_spec: false,
            type_var_tuple: false,
        }))
    }

    pub(in crate::infrastructure::mypy) fn native_variable_identity(
        &self,
        symbol: &'static str,
        position: u32,
    ) -> Option<u32> {
        self.nodes
            .borrow()
            .native_variables
            .get(&(symbol, position))
            .copied()
    }

    pub(crate) fn intern(&self, node: Node) -> Type {
        let mut nodes = self.nodes.borrow_mut();
        if let Some(&index) = nodes.index.get(&node) {
            return Type::Node(index);
        }
        let index = u32::try_from(nodes.values.len()).expect("type node index fits in u32");
        let node = Arc::new(node);
        nodes.values.push(node.clone());
        nodes.index.insert(node, index);
        Type::Node(index)
    }

    pub(crate) fn node(&self, ty: Type) -> Option<Arc<Node>> {
        let Type::Node(index) = ty else {
            return None;
        };
        self.nodes.borrow().values.get(index as usize).cloned()
    }

    pub(crate) fn join(&self, left: Type, right: Type) -> Type {
        if let Some(joined) = self.bounded_union_join(left, right) {
            return joined;
        }
        if left.is_any() || right.is_any() {
            return left.merge_any(right);
        }
        if self.accepts(left, right) {
            return self.joined_instance_any_origin(left, right, left);
        }
        if self.accepts(right, left) {
            return self.joined_instance_any_origin(left, right, right);
        }
        if left == Type::None || right == Type::None {
            return self.union(if left == Type::None {
                vec![right, left]
            } else {
                vec![left, right]
            });
        }
        if matches!(
            (self.node(left).as_deref(), self.node(right).as_deref()),
            (
                Some(Node::Container(Container::List, _)),
                Some(Node::Container(Container::List, _))
            )
        ) {
            return Type::Object;
        }
        if let (Some(left), Some(right)) = (self.sequence_item(left), self.sequence_item(right)) {
            let item = self.join_instance_any_argument(left, right);
            return self.intern(Node::Container(Container::Sequence, vec![item]));
        }
        if let Some(joined) = self.single_inheritance_join(left, right) {
            return self.joined_instance_any_origin(left, right, joined);
        }
        Type::Object
    }

    // InstanceJoiner visits direct generic arguments even for equal instances.
    // Fixed tuple slots instead use scalar joins. Only a Special-participating
    // computation takes this path; ordinary-only joins keep their old result.
    fn joined_instance_any_origin(&self, left: Type, right: Type, joined: Type) -> Type {
        if !self.has_special_any(joined) {
            return joined;
        }
        let left_node = self.node(left);
        let right_node = self.node(right);
        let joined_node = self.node(joined);
        match (
            left_node.as_deref(),
            right_node.as_deref(),
            joined_node.as_deref(),
        ) {
            (Some(Node::Tuple(a)), Some(Node::Tuple(b)), Some(Node::Tuple(items)))
                if a.len() == b.len()
                    && a.len() == items.len()
                    && self.tuple_pack_position(a).is_none()
                    && self.tuple_pack_position(b).is_none() =>
            {
                self.intern(Node::Tuple(
                    a.iter().zip(b).map(|(&a, &b)| self.join(a, b)).collect(),
                ))
            }
            (
                Some(Node::Container(a, left)),
                Some(Node::Container(b, right)),
                Some(Node::Container(kind, items)),
            ) if a == b
                && a == kind
                && *kind != Container::TypeObject
                && left.len() == right.len()
                && left.len() == items.len() =>
            {
                self.intern(Node::Container(
                    *kind,
                    left.iter()
                        .zip(right)
                        .map(|(&a, &b)| self.join_instance_any_argument(a, b))
                        .collect(),
                ))
            }
            (
                Some(Node::AppliedClass {
                    id: a,
                    arguments: left,
                    value: av,
                }),
                Some(Node::AppliedClass {
                    id: b,
                    arguments: right,
                    value: bv,
                }),
                Some(Node::AppliedClass {
                    id,
                    arguments,
                    value,
                }),
            ) if a == b
                && a == id
                && av == bv
                && av == value
                && left.len() == right.len()
                && left.len() == arguments.len() =>
            {
                self.applied_class(
                    *id,
                    left.iter()
                        .zip(right)
                        .map(|(&a, &b)| self.join_instance_any_argument(a, b))
                        .collect(),
                    *value,
                )
            }
            (
                Some(Node::VariadicTuple(a)),
                Some(Node::VariadicTuple(b)),
                Some(Node::VariadicTuple(_)),
            ) => self.intern(Node::VariadicTuple(self.join_instance_any_argument(*a, *b))),
            // Native-only or opaque joins have no positive erased-origin rule.
            // Keep their legacy value shape without asserting Special origin.
            _ => self.ordinary_any_origin(joined),
        }
    }

    fn join_instance_any_argument(&self, left: Type, right: Type) -> Type {
        if (self.has_special_any(left) || self.has_special_any(right))
            && (left.is_any() || right.is_any())
        {
            Type::Any
        } else {
            self.join(left, right)
        }
    }

    pub(crate) fn sequence_item(&self, ty: Type) -> Option<Type> {
        match ty {
            Type::Str => Some(Type::Str),
            Type::Bytes => Some(Type::Int),
            _ => self.node(ty).and_then(|node| match node.as_ref() {
                Node::Container(Container::List | Container::Sequence, args) => Some(args[0]),
                Node::VariadicTuple(item) => Some(*item),
                Node::Tuple(items) => items
                    .iter()
                    .copied()
                    .reduce(|left, right| self.join(left, right)),
                _ => self.nominal_sequence_item(ty),
            }),
        }
    }

    pub(crate) fn union(&self, types: Vec<Type>) -> Type {
        let mut items = Vec::new();
        for ty in types {
            let flattened = self.node(ty);
            let types = match flattened.as_deref() {
                Some(Node::Union(items)) => items.clone(),
                _ => vec![ty],
            };
            for ty in types {
                if ty != Type::Never && !items.iter().any(|&prior| self.any_origin_equal(prior, ty))
                {
                    items.push(ty);
                }
            }
        }
        if items.is_empty() {
            Type::Never
        } else if items.len() == 1 {
            items[0]
        } else {
            self.intern(Node::Union(items))
        }
    }

    pub(crate) fn variance_hint(&self, expected: Type, actual: Type) -> Option<String> {
        if let Some(expected) = self.standard_iterable_form(expected) {
            return self.variance_hint(expected, actual);
        }
        if let Some(hint) = self.iterable_hint(expected, actual) {
            return Some(hint);
        }
        if let Some(hint) = self.protocol_hint(expected, actual) {
            return Some(hint);
        }
        if self.typed_dict(actual).is_some() {
            if let Some(node) = self.node(expected) {
                if let Node::Container(Container::Iterable, args) = node.as_ref() {
                    if self.is_class_value(actual) {
                        return Some(format!("Following member(s) of \"{}\" have conflicts:\n    Expected:\n        def __iter__() -> Iterator[{}]\n    Got:\n        def __iter__({}, /) -> Iterator[str]", self.class(self.class_id(actual).unwrap()).name, self.name(args[0]), self.name(expected)));
                    }
                    return Some(format!("Following member(s) of \"{}\" have conflicts:\n    Expected:\n        def __iter__(self) -> Iterator[{}]\n    Got:\n        def __iter__(self) -> Iterator[str]", self.name(actual), self.name(args[0])));
                }
            }
        }
        let (expected, actual) = (self.node(expected)?, self.node(actual)?);
        let (Node::Container(kind, expected), Node::Container(actual_kind, actual)) =
            (expected.as_ref(), actual.as_ref())
        else {
            return None;
        };
        if kind != actual_kind
            || expected.len() != actual.len()
            || !expected
                .iter()
                .zip(actual)
                .all(|(&e, &a)| self.accepts(e, a))
        {
            return None;
        }
        let suggestion = match kind {
            Container::List => "Consider using \"Sequence\" instead, which is covariant",
            Container::Dict if expected[0] == actual[0] => {
                "Consider using \"Mapping\" instead, which is covariant in the value type"
            }
            _ => return None,
        };
        Some(format!("\"{}\" is invariant -- see https://mypy.readthedocs.io/en/stable/common_issues.html#variance\n{suggestion}", kind.name()))
    }
}
