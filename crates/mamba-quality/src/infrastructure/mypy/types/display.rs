//! Diagnostic names retain the original type and union spelling.
use super::*;

impl Context {
    pub(crate) fn name(&self, ty: Type) -> String {
        if let Some(node) = self.node(ty) {
            return match node.as_ref() {
                Node::Deleted(_) => "<deleted>".into(),
                Node::Container(kind, args) => format!("{}[{}]", kind.name(), self.names(args)),
                Node::Tuple(args) if args.is_empty() => "tuple[()]".into(),
                Node::Tuple(args) => format!("tuple[{}]", self.names(args)),
                Node::VariadicTuple(item) => format!("tuple[{}, ...]", self.name(*item)),
                Node::Unpack(item) => format!("*{}", self.name(*item)),
                Node::Union(items) => self.union_name(items),
                Node::Variable(variable) => variable.name.clone(),
                Node::ParamSpecArgs(variable) => format!("{}.args", self.name(*variable)),
                Node::ParamSpecKwargs(variable) => format!("{}.kwargs", self.name(*variable)),
                Node::Parameters(parameters, ellipsis) => {
                    if *ellipsis {
                        "...".into()
                    } else {
                        format!(
                            "[{}]",
                            parameters
                                .iter()
                                .map(|parameter| self.name(parameter.ty.unwrap_or(Type::Any)))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                }
                Node::Instance(id) => self.class(*id).name.clone(),
                Node::Class(id) => format!("type[{}]", self.class(*id).name),
                Node::AppliedClass {
                    id,
                    arguments,
                    value,
                } => {
                    let instance = format!(
                        "{}[{}]",
                        self.class(*id).name,
                        self.display_generic_arguments(&self.class(*id).generic, arguments, false)
                    );
                    if *value {
                        format!("type[{instance}]")
                    } else {
                        instance
                    }
                }
                Node::Callable(signature) => self.callable_name(signature),
                Node::Literal(value) => format!("Literal[{}]", value.display()),
                Node::LiteralForm => "Literal".into(),
                Node::TypingForm(name) => name.clone(),
                Node::Alias(id, _) => self.alias(*id).name.clone(),
                Node::AliasInstance(id, arguments) => self.alias_instance_name(*id, arguments),
                Node::InferredAlias { target, .. } => self.name(*target),
                Node::LiteralValue(value) => self.name(value.base()),
            };
        }
        match ty {
            Type::Int => "int",
            Type::Float => "float",
            Type::Complex => "complex",
            Type::Str => "str",
            Type::Bytes => "bytes",
            Type::Bool => "bool",
            Type::None => "None",
            Type::Any | Type::AnySpecial => "Any",
            Type::Object => "object",
            Type::Never => "Never",
            Type::Node(_) => unreachable!("type IDs belong to their workspace context"),
        }
        .into()
    }

    fn names(&self, items: &[Type]) -> String {
        items
            .iter()
            .map(|&item| self.name(item))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub(super) fn display_union_items(&self, items: &[Type]) -> Vec<Type> {
        let mut displayed = Vec::new();
        for &item in items {
            let item = match self.node(item).as_deref() {
                Some(Node::LiteralValue(value)) => value.base(),
                _ => item,
            };
            // Ignore last-known values for display, then deduplicate type identities.
            // Distinct nominal types and explicit Literal types keep their identities.
            if !displayed.contains(&item) {
                displayed.push(item);
            }
        }
        displayed
    }

    fn union_name(&self, items: &[Type]) -> String {
        let displayed = self.display_union_items(items);
        let items = displayed.as_slice();
        let values: Option<Vec<_>> = items
            .iter()
            .copied()
            .filter(|&ty| ty != Type::None)
            .map(|ty| {
                self.node(ty).and_then(|node| {
                    if let Node::Literal(value) = node.as_ref() {
                        Some(value.display())
                    } else {
                        None
                    }
                })
            })
            .collect();
        if let Some(values) = values.filter(|values| !values.is_empty()) {
            return format!(
                "Literal[{}]{}",
                values.join(", "),
                if items.contains(&Type::None) {
                    " | None"
                } else {
                    ""
                }
            );
        }
        let literals: Vec<_> = items
            .iter()
            .filter_map(|&item| {
                self.node(item).and_then(|node| match node.as_ref() {
                    Node::Literal(value) => Some(value.display()),
                    _ => None,
                })
            })
            .collect();
        let mut names: Vec<_> = items
            .iter()
            .copied()
            .filter(|&item| {
                item != Type::None
                    && !(literals.len() > 1
                        && self
                            .node(item)
                            .is_some_and(|node| matches!(node.as_ref(), Node::Literal(_))))
            })
            .map(|item| self.name(item))
            .collect();
        if literals.len() > 1 {
            names.insert(0, format!("Literal[{}]", literals.join(", ")));
        }
        if names.len() > 10 {
            let more = names.len() - 5;
            names.truncate(5);
            names.push(format!("<{more} more items>"));
        }
        if items.contains(&Type::None) {
            names.push("None".into());
        }
        names.join(" | ")
    }
}
