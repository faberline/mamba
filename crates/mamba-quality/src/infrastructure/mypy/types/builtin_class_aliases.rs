//! Imported builtin class names retain their canonical annotation forms.
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;

impl Context {
    pub(in crate::infrastructure::mypy) fn builtin_constructor_alias(
        &self,
        name: &str,
        target: PythonTarget,
    ) -> Option<Type> {
        if matches!(
            name,
            "list" | "dict" | "set" | "frozenset" | "tuple" | "type"
        ) {
            return Some(self.intern(Node::TypingForm(format!("builtin:{name}"))));
        }
        if matches!(name, "reversed" | "enumerate" | "range" | "zip") {
            self.collection_constructor(name, target)
                .or_else(|| self.aggregation_function(name, target))?;
            let id = self.declare_class(&format!("builtins.{name}"), name, 0);
            return Some(self.class(id).object);
        }
        None
    }

    pub(super) fn builtin_alias_annotation(
        &self,
        expression: &Expr,
        variables: &VariableMap,
    ) -> Option<Type> {
        let (base, arguments) = match expression {
            Expr::Subscript(value) => {
                let arguments = match value.slice.as_ref() {
                    Expr::Tuple(tuple) => tuple.elts.iter().collect::<Vec<_>>(),
                    argument => vec![argument],
                };
                (value.value.as_ref(), Some(arguments))
            }
            value => (value, None),
        };
        let marker = self.annotation_binding(base, variables)?;
        let node = self.node(marker)?;
        let Node::TypingForm(form) = node.as_ref() else {
            return None;
        };
        let name = form.strip_prefix("builtin:")?;
        if name == "type" {
            let item = match arguments {
                Some(args) if args.len() == 1 => {
                    self.annotation_with_variables(args[0], variables)?
                }
                _ => Type::Any,
            };
            return Some(self.intern(Node::Container(Container::TypeObject, vec![item])));
        }
        if name == "tuple" {
            return match arguments {
                None => Some(self.intern(Node::VariadicTuple(Type::Any))),
                Some(args) if args.len() == 2 && args[1].is_ellipsis_literal_expr() => {
                    Some(self.intern(Node::VariadicTuple(
                        self.annotation_with_variables(args[0], variables)?,
                    )))
                }
                Some(args) => Some(
                    self.normalize_tuple(
                        args.into_iter()
                            .map(|arg| self.annotation_with_variables(arg, variables))
                            .collect::<Option<Vec<_>>>()?,
                    ),
                ),
            };
        }
        let kind = match name {
            "list" => Container::List,
            "set" => Container::Set,
            "frozenset" => Container::FrozenSet,
            "dict" => Container::Dict,
            _ => return None,
        };
        let count = if kind == Container::Dict { 2 } else { 1 };
        let args = match arguments {
            None => vec![Type::Any; count],
            Some(args) if args.len() == count => args
                .into_iter()
                .map(|arg| self.annotation_with_variables(arg, variables))
                .collect::<Option<Vec<_>>>()?,
            _ => vec![Type::Any; count],
        };
        Some(self.intern(Node::Container(kind, args)))
    }
}
