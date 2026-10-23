//! Literal and constructor collections retain original expansion and spelling.
use super::{
    resolution::Context,
    unpacking_values::{Active, Unpacked},
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    Expr, ExprCall, ExprStringLiteral, StringLiteral, StringLiteralFlags, StringLiteralValue,
};
use ruff_text_size::{Ranged, TextRange};

fn string(value: &str, range: TextRange) -> Expr {
    Expr::StringLiteral(ExprStringLiteral {
        node_index: Default::default(),
        range,
        value: StringLiteralValue::single(StringLiteral {
            node_index: Default::default(),
            range,
            value: value.into(),
            flags: StringLiteralFlags::empty(),
        }),
    })
}
fn unique(values: Vec<Expr>, source: &str) -> Vec<Expr> {
    let mut seen = std::collections::BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(super::display::expression(value, source)))
        .collect()
}
pub(super) fn display(kind: &str, elements: &[Expr], source: &str) -> String {
    let elements: Vec<_> = elements
        .iter()
        .map(|value| super::display::expression(value, source))
        .collect();
    let text = elements.join(", ");
    match kind {
        "list" => format!("[{text}]"),
        "tuple" => format!("({text}{})", if elements.len() == 1 { ", " } else { "" }),
        _ => format!("{{{text}}}"),
    }
}
impl Checker<'_> {
    pub(super) fn unpacking_elements(
        &self,
        elements: &[Expr],
        origin: Context,
        active: &mut Active,
    ) -> Option<Vec<Expr>> {
        let mut values = Vec::new();
        for value in elements {
            if let Expr::Starred(value) = value {
                values.extend(
                    self.unpacked_inner(&value.value, origin.clone(), active)?
                        .elements?,
                );
            } else {
                values.push(value.clone());
            }
        }
        Some(values)
    }
    pub(super) fn unpacking_dictionary(
        &self,
        items: &[mamba_language::application::quality_syntax::python_ast::DictItem],
        origin: Context,
        active: &mut Active,
    ) -> Option<Vec<(Expr, Expr)>> {
        let mut entries = Vec::new();
        for item in items {
            if let Some(key) = &item.key {
                entries.push((key.clone(), item.value.clone()));
            } else {
                entries.extend(
                    self.unpacked_inner(&item.value, origin.clone(), active)?
                        .entries?,
                );
            }
        }
        Some(entries)
    }
    pub(super) fn unpacking_constructor(
        &self,
        call: &ExprCall,
        kind: &'static str,
        result: &mut Unpacked,
        origin: Context,
        active: &mut Active,
        source: &str,
    ) -> Option<()> {
        if kind == "dict" {
            if call.arguments.args.len() > 1 {
                return None;
            }
            let mut entries = if let Some(argument) = call.arguments.args.first() {
                let value = self.unpacked_inner(argument, origin.clone(), active)?;
                if let Some(entries) = value.entries {
                    entries
                } else {
                    let mut entries = Vec::new();
                    for pair in value.elements? {
                        let pair = self
                            .unpacked_inner(&pair, origin.clone(), active)?
                            .elements?;
                        if pair.len() != 2 {
                            return None;
                        }
                        entries.push((pair[0].clone(), pair[1].clone()));
                    }
                    entries
                }
            } else {
                Vec::new()
            };
            for keyword in &call.arguments.keywords {
                if let Some(name) = &keyword.arg {
                    entries.push((
                        string(name.as_str(), keyword.range()),
                        keyword.value.clone(),
                    ));
                } else {
                    entries.extend(
                        self.unpacked_inner(&keyword.value, origin.clone(), active)?
                            .entries?,
                    );
                }
            }
            result.elements = Some(entries.iter().map(|(key, _)| key.clone()).collect());
            result.entries = Some(entries);
            result.dictionary = true;
            result.iterable = true;
            result.kind = kind;
            return Some(());
        }
        if !call.arguments.keywords.is_empty() || call.arguments.args.len() > 1 {
            return None;
        }
        let mut elements = if let Some(argument) = call.arguments.args.first() {
            if let Expr::StringLiteral(value) = argument {
                value
                    .value
                    .to_str()
                    .chars()
                    .map(|value| string(&value.to_string(), argument.range()))
                    .collect()
            } else {
                self.unpacked_inner(argument, origin, active)?.elements?
            }
        } else {
            Vec::new()
        };
        if kind == "set" {
            elements = unique(elements, source);
        }
        result.display = display(kind, &elements, source);
        result.elements = Some(elements);
        result.iterable = true;
        result.kind = kind;
        Some(())
    }
}
