//! Typed dictionary declarations retain ordered fields and per-field mutability.
use super::super::types::{Node, VariableMap, Variables};
use super::*;
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall, Stmt, StmtClassDef};

#[derive(Clone)]
pub(in crate::infrastructure::mypy) struct Item {
    pub name: String,
    pub ty: Type,
    pub required: bool,
    pub readonly: bool,
}

#[derive(Clone, Default)]
pub(in crate::infrastructure::mypy) struct Definition {
    pub items: Vec<Item>,
    pub closed: bool,
}

fn dotted_name(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Name(name) => Some(name.id.to_string()),
        Expr::Attribute(attribute) => Some(format!(
            "{}.{}",
            dotted_name(&attribute.value)?,
            attribute.attr
        )),
        _ => None,
    }
}

impl Context {
    pub(in crate::infrastructure::mypy) fn typed_dict_forms(
        &self,
        body: &[Stmt],
        variables: &mut Variables,
    ) {
        for import in crate::infrastructure::project::imports(body, "", false) {
            let forms: &[&str] = if import.module == "abc" {
                &["abstractmethod"]
            } else if import.module == "dataclasses" {
                &[
                    "dataclass",
                    "field",
                    "InitVar",
                    "KW_ONLY",
                    "replace",
                    "fields",
                    "Field",
                    "asdict",
                    "astuple",
                ]
            } else if import.module == "collections.abc" {
                &[
                    "Awaitable",
                    "Coroutine",
                    "Generator",
                    "Iterator",
                    "AsyncGenerator",
                    "AsyncIterator",
                    "AsyncIterable",
                ]
            } else if matches!(import.module.as_str(), "typing" | "typing_extensions") {
                &[
                    "TypedDict",
                    "Required",
                    "NotRequired",
                    "ReadOnly",
                    "Protocol",
                    "runtime_checkable",
                    "disjoint_base",
                    "ClassVar",
                    "Annotated",
                    "Final",
                    "TypeAlias",
                    "Type",
                    "Generic",
                    "ParamSpec",
                    "Concatenate",
                    "TypeVarTuple",
                    "TypeGuard",
                    "TypeIs",
                    "Unpack",
                    "dataclass_transform",
                    "Awaitable",
                    "Coroutine",
                    "Generator",
                    "Iterator",
                    "AsyncGenerator",
                    "AsyncIterator",
                    "AsyncIterable",
                ]
            } else {
                continue;
            };
            for &form in forms {
                let local = if import.member.as_deref() == Some(form) {
                    Some(import.local.clone())
                } else if import.member.is_none() {
                    Some(format!("{}.{form}", import.local))
                } else {
                    None
                };
                if let Some(local) = local {
                    variables.entry(local).or_insert_with(|| {
                        if form == "Field" {
                            let field = self.dataclass_field_type();
                            self.class(self.class_id(field).unwrap()).object
                        } else {
                            self.intern(Node::TypingForm(form.into()))
                        }
                    });
                }
            }
        }
    }

    pub(in crate::infrastructure::mypy) fn typing_form(
        &self,
        expression: &Expr,
        variables: &VariableMap,
    ) -> Option<String> {
        let ty = *variables.get(&dotted_name(expression)?)?;
        let node = self.node(ty)?;
        if let Node::TypingForm(form) = node.as_ref() {
            Some(form.clone())
        } else {
            let id = match node.as_ref() {
                Node::Class(id) | Node::Instance(id) | Node::AppliedClass { id, .. } => *id,
                _ => return None,
            };
            let class = self.class(id);
            // Source identities include the project, target, and exact declaration.
            // Native classes must own their canonical zero-scope registry key.
            let fullname = self.embedded_source_class_name(id).or_else(|| {
                (self
                    .classes
                    .borrow()
                    .keys
                    .get(&(class.namespace.clone(), 0))
                    == Some(&id))
                .then(|| class.namespace.clone())
            })?;
            match fullname.as_str() {
                "typing.ParamSpec" | "typing_extensions.ParamSpec" => Some("ParamSpec".into()),
                "typing.TypeVarTuple" | "typing_extensions.TypeVarTuple" => {
                    Some("TypeVarTuple".into())
                }
                _ => None,
            }
        }
    }

    fn typed_dict_item(
        &self,
        name: String,
        annotation: &Expr,
        total: bool,
        variables: &VariableMap,
    ) -> Item {
        let mut required = total;
        let mut readonly = false;
        let mut annotation = annotation;
        while let Expr::Subscript(subscript) = annotation {
            match self.typing_form(&subscript.value, variables).as_deref() {
                Some("Required") => required = true,
                Some("NotRequired") => required = false,
                Some("ReadOnly") => readonly = true,
                _ => break,
            }
            annotation = &subscript.slice;
        }
        Item {
            name,
            ty: self
                .annotation_with_variables(annotation, variables)
                .unwrap_or(Type::Any),
            required,
            readonly,
        }
    }

    pub(in crate::infrastructure::mypy) fn typed_dict(
        &self,
        ty: Type,
    ) -> Option<(Arc<Class>, Definition)> {
        let class = self.class(self.class_id(ty)?);
        let definition = class.typed_dict.clone()?;
        Some((class, definition))
    }

    pub(in crate::infrastructure::mypy) fn class_typed_dict(
        &self,
        statement: &StmtClassDef,
        variables: &VariableMap,
    ) -> Option<Definition> {
        let mut definition = Definition::default();
        let mut recognized = false;
        if let Some(arguments) = &statement.arguments {
            for base in &arguments.args {
                if self.typing_form(base, variables).as_deref() == Some("TypedDict") {
                    recognized = true;
                } else if let Some((_, parent)) = self
                    .annotation_with_variables(base, variables)
                    .and_then(|ty| self.typed_dict(ty))
                {
                    recognized = true;
                    definition.closed |= parent.closed;
                    for item in parent.items {
                        if !definition
                            .items
                            .iter()
                            .any(|previous| previous.name == item.name)
                        {
                            definition.items.push(item);
                        }
                    }
                }
            }
        }
        if !recognized {
            return None;
        }
        if let Some(closed) = statement.arguments.as_ref().and_then(|arguments| {
            arguments.keywords.iter().find_map(|keyword| {
                if keyword
                    .arg
                    .as_ref()
                    .is_some_and(|name| name.as_str() == "closed")
                {
                    if let Expr::BooleanLiteral(value) = &keyword.value {
                        return Some(value.value);
                    }
                }
                None
            })
        }) {
            definition.closed = closed;
        }
        let total = statement.arguments.as_ref().is_none_or(|arguments| {
            !arguments.keywords.iter().any(|keyword| {
                keyword
                    .arg
                    .as_ref()
                    .is_some_and(|name| name.as_str() == "total")
                    && matches!(&keyword.value, Expr::BooleanLiteral(value) if !value.value)
            })
        });
        let mut local_names = std::collections::BTreeSet::new();
        for child in &statement.body {
            let Stmt::AnnAssign(assignment) = child else {
                continue;
            };
            let Expr::Name(name) = assignment.target.as_ref() else {
                continue;
            };
            if !local_names.insert(name.id.to_string()) {
                continue;
            }
            let item = self.typed_dict_item(
                name.id.to_string(),
                &assignment.annotation,
                total,
                variables,
            );
            if let Some(previous) = definition
                .items
                .iter_mut()
                .find(|previous| previous.name == item.name)
            {
                *previous = item;
            } else {
                definition.items.push(item);
            }
        }
        Some(definition)
    }

    pub(in crate::infrastructure::mypy) fn functional_typed_dict(
        &self,
        call: &ExprCall,
        variables: &VariableMap,
    ) -> Option<Definition> {
        if self.typing_form(&call.func, variables).as_deref() != Some("TypedDict") {
            return None;
        }
        let [Expr::StringLiteral(_), Expr::Dict(dictionary)] = call.arguments.args.as_slice()
        else {
            return None;
        };
        let total = !call.arguments.keywords.iter().any(|keyword| {
            keyword
                .arg
                .as_ref()
                .is_some_and(|name| name.as_str() == "total")
                && matches!(&keyword.value, Expr::BooleanLiteral(value) if !value.value)
        });
        let mut items = Vec::new();
        for item in &dictionary.items {
            let Some(Expr::StringLiteral(name)) = item.key.as_ref() else {
                return None;
            };
            items.push(self.typed_dict_item(
                name.value.to_str().into(),
                &item.value,
                total,
                variables,
            ));
        }
        let closed = call.arguments.keywords.iter().any(|keyword| {
            keyword
                .arg
                .as_ref()
                .is_some_and(|name| name.as_str() == "closed")
                && matches!(&keyword.value, Expr::BooleanLiteral(value) if value.value)
        });
        Some(Definition { items, closed })
    }

    pub(in crate::infrastructure::mypy) fn typed_dict_accepts(
        &self,
        expected: &Definition,
        actual: &Definition,
    ) -> bool {
        expected.items.iter().all(|parent| {
            let Some(child) = actual.items.iter().find(|child| child.name == parent.name) else {
                return false;
            };
            if parent.readonly {
                (!parent.required || child.required) && self.accepts(parent.ty, child.ty)
            } else {
                !child.readonly
                    && parent.required == child.required
                    && self.accepts(parent.ty, child.ty)
                    && self.accepts(child.ty, parent.ty)
            }
        })
    }

    pub(in crate::infrastructure::mypy) fn typed_dict_keys(
        &self,
        expression: &Expr,
        environment: &VariableMap,
    ) -> Option<Vec<String>> {
        if let Expr::StringLiteral(string) = expression {
            return Some(vec![string.value.to_str().into()]);
        }
        fn collect(context: &Context, ty: Type, keys: &mut Vec<String>) -> Option<()> {
            match context.node(ty)?.as_ref() {
                Node::Literal(super::super::types::Literal::Str(value)) => keys.push(value.clone()),
                Node::Union(items) => {
                    for &item in items {
                        collect(context, item, keys)?;
                    }
                }
                _ => return None,
            }
            Some(())
        }
        let ty = self.infer_with_calls(expression, environment, &|_| None)?;
        let mut keys = Vec::new();
        collect(self, ty, &mut keys)?;
        Some(keys)
    }
}
