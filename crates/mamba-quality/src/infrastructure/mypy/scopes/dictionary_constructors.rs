//! Keyword-only dictionary calls follow the original dictionary expression rewrite.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{
    AtomicNodeIndex, DictItem, ExprCall, ExprDict, ExprStringLiteral, StringLiteral,
    StringLiteralFlags, StringLiteralValue,
};
use types::{Container, Node};

impl Scopes {
    pub(in crate::infrastructure::mypy) fn constructor_argument(
        &self,
        signature: &Signature,
        expression: &Expr,
        actual: Type,
        environment: &BTreeMap<String, Type>,
    ) -> Type {
        let actual = self
            .context
            .iteration_argument(signature, expression, actual);
        self.context
            .dictionary_argument(signature, expression, actual, &|expr| {
                self.context.infer_with_calls(expr, environment, &|expr| {
                    self.expression_type(expr, environment)
                })
            })
    }
    pub(in crate::infrastructure::mypy) fn dictionary_expression(
        &self,
        call: &ExprCall,
        signature: &Signature,
    ) -> Option<Expr> {
        if signature.definition_module.as_deref() != Some("builtins")
            || signature.name.as_deref() != Some("dict")
            || !call.arguments.args.is_empty()
            || matches!(call.func.as_ref(), Expr::Subscript(_))
            || self.constructor_alias_marker(&call.func).is_some()
        {
            return None;
        }
        let items = call
            .arguments
            .keywords
            .iter()
            .map(|keyword| DictItem {
                key: keyword.arg.as_ref().map(|name| {
                    Expr::StringLiteral(ExprStringLiteral {
                        node_index: AtomicNodeIndex::NONE,
                        range: keyword.value.range(),
                        value: StringLiteralValue::single(StringLiteral {
                            node_index: AtomicNodeIndex::NONE,
                            range: keyword.value.range(),
                            value: name.as_str().into(),
                            flags: StringLiteralFlags::empty(),
                        }),
                    })
                }),
                value: keyword.value.clone(),
            })
            .collect();
        Some(Expr::Dict(ExprDict {
            node_index: AtomicNodeIndex::NONE,
            range: call.range(),
            items,
        }))
    }

    pub(super) fn dictionary_expression_return(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
        expected: Option<Type>,
    ) -> Option<Type> {
        let Expr::Dict(dictionary) = expression else {
            return None;
        };
        if let Some((key, value)) = expected.and_then(|ty| self.context.mapping_types(ty)) {
            return Some(
                self.context
                    .intern(Node::Container(Container::Dict, vec![key, value])),
            );
        }
        let mut key = Type::Never;
        let mut value = Type::Never;
        for item in &dictionary.items {
            let (next_key, next_value) = if item.key.is_some() {
                (
                    Type::Str,
                    self.context
                        .infer_with_calls(&item.value, environment, &|expr| {
                            self.expression_type(expr, environment)
                        })?,
                )
            } else {
                let ty = self
                    .context
                    .infer_with_calls(&item.value, environment, &|expr| {
                        self.expression_type(expr, environment)
                    })?;
                self.context.mapping_types(ty)?
            };
            key = self.context.join(key, next_key);
            value = self.context.join(value, next_value);
        }
        if let Some(Node::Union(items)) = self.context.node(value).as_deref() {
            if items.contains(&Type::None) {
                let retained = items
                    .iter()
                    .map(|&ty| {
                        let mut literals = dictionary.items.iter().filter_map(|item| {
                            let literal = self.context.literal_expression(&item.value)?;
                            match self.context.node(literal).as_deref() {
                                Some(Node::Literal(literal)) if literal.base() == ty => {
                                    Some(literal.clone())
                                }
                                _ => None,
                            }
                        });
                        match (literals.next(), literals.next()) {
                            (Some(literal), None) => {
                                self.context.intern(Node::LiteralValue(literal))
                            }
                            _ => ty,
                        }
                    })
                    .collect();
                value = self.context.union(retained);
            }
        }
        Some(
            self.context
                .intern(Node::Container(Container::Dict, vec![key, value])),
        )
    }
}
