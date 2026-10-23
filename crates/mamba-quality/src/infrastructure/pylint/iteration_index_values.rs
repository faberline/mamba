//! Index candidates retain raw call names, inferred bound methods, and start values.
use super::{callables::Target, display, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprSubscript};

pub(super) struct Lookup {
    pub dictionary: bool,
    pub object: String,
    pub key: String,
    pub value: String,
    pub pair: bool,
}

fn nonzero(expression: &Expr) -> Option<bool> {
    if let Some(value) = super::simplifiable_numbers::number(expression) {
        return Some(value.truth());
    }
    match expression {
        Expr::StringLiteral(_) | Expr::BytesLiteral(_) => Some(true),
        _ => None,
    }
}
impl Checker<'_> {
    pub(super) fn index_lookup(&self, target: &Expr, iterator: &Expr) -> Option<Lookup> {
        let Expr::Call(call) = iterator else {
            return None;
        };
        if let Expr::Attribute(attribute) = call.func.as_ref() {
            let bound = matches!(self.callable_target(&call.func), Some(Target::Function(value)) if matches!(value.category, "method" | "classmethod"))
                || self
                    .unpacked_value(
                        &attribute.value,
                        Context::Scope(self.scopes.current_index()),
                    )
                    .is_some_and(|value| value.dictionary && value.view.is_none());
            if attribute.attr.as_str() != "items" || !bound {
                return None;
            }
            let object = display::expression(&attribute.value, &self.source.text);
            return match target {
                Expr::Tuple(tuple) if tuple.elts.len() >= 2 => {
                    let Expr::Name(key) = &tuple.elts[0] else {
                        return None;
                    };
                    Some(Lookup {
                        dictionary: true,
                        object,
                        key: key.id.to_string(),
                        value: display::expression(&tuple.elts[1], &self.source.text),
                        pair: false,
                    })
                }
                Expr::Name(name) => Some(Lookup {
                    dictionary: true,
                    object,
                    key: name.id.to_string(),
                    value: String::new(),
                    pair: true,
                }),
                _ => None,
            };
        }
        if !matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "enumerate") {
            return None;
        }
        let (Expr::Name(object), _) = self.stdlib_argument(call, 0, "iterable")? else {
            return None;
        };
        let Expr::Tuple(tuple) = target else {
            return None;
        };
        let [Expr::Name(key), Expr::Name(value), ..] = tuple.elts.as_slice() else {
            return None;
        };
        let start = call.arguments.args.get(1).or_else(|| {
            call.arguments
                .keywords
                .iter()
                .find(|value| {
                    value
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "start")
                })
                .map(|value| &value.value)
        });
        if start.is_some_and(|start| {
            let raw = if let Expr::UnaryOp(value) = start {
                value.operand.as_ref()
            } else {
                start
            };
            nonzero(raw).or_else(|| {
                self.stdlib_constant(start, Context::Scope(self.scopes.current_index()))
                    .as_ref()
                    .and_then(nonzero)
            }) == Some(true)
        }) {
            return None;
        }
        Some(Lookup {
            dictionary: false,
            object: object.id.to_string(),
            key: key.id.to_string(),
            value: value.id.to_string(),
            pair: false,
        })
    }
    pub(super) fn index_replacement(
        &self,
        lookup: &Lookup,
        subscript: &ExprSubscript,
    ) -> Option<String> {
        if display::expression(&subscript.value, &self.source.text) != lookup.object {
            return None;
        }
        if lookup.pair {
            let Expr::Subscript(key) = subscript.slice.as_ref() else {
                return None;
            };
            if !matches!(key.value.as_ref(), Expr::Name(name) if name.id.as_str() == lookup.key) {
                return None;
            }
            let zero = nonzero(&key.slice).or_else(|| {
                self.stdlib_constant(&key.slice, Context::Scope(self.scopes.current_index()))
                    .as_ref()
                    .and_then(nonzero)
            });
            if zero != Some(false) {
                return None;
            }
            let display = display::expression(&subscript.slice, &self.source.text);
            return Some(display.rsplit_once('0').map_or_else(
                || display.clone(),
                |(left, right)| format!("{left}1{right}"),
            ));
        }
        matches!(subscript.slice.as_ref(), Expr::Name(name) if name.id.as_str() == lookup.key)
            .then(|| lookup.value.clone())
    }
}
