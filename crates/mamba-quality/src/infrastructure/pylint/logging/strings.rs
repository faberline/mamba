//! Logging format calls retain the interacting default string diagnostics.
use super::*;

fn fields(text: &str) -> Result<Vec<(String, bool)>, ()> {
    let values: Vec<_> = text.chars().collect();
    let mut result = Vec::new();
    let mut position = 0;
    while position < values.len() {
        let value = values[position];
        if matches!(value, '{' | '}') && values.get(position + 1) == Some(&value) {
            position += 2;
            continue;
        }
        if value == '}' {
            return Err(());
        }
        if value != '{' {
            position += 1;
            continue;
        }
        position += 1;
        let start = position;
        let mut depth = 1;
        while depth != 0 {
            let value = *values.get(position).ok_or(())?;
            depth += usize::from(value == '{');
            depth -= usize::from(value == '}');
            position += 1;
        }
        let contents: String = values[start..position - 1].iter().collect();
        let name = contents
            .split(['!', ':'])
            .next()
            .unwrap_or_default()
            .to_string();
        let specification = contents
            .split_once(':')
            .is_some_and(|(_, specification)| !specification.is_empty());
        result.push((name, specification));
    }
    Ok(result)
}

pub(super) fn complex(text: &str) -> bool {
    fields(text).is_ok_and(|fields| fields.iter().any(|(_, complex)| *complex))
}

impl Checker<'_> {
    pub(in crate::infrastructure::pylint) fn check_logging_string_interactions(
        &mut self,
        expression: &Expr,
    ) {
        if let Expr::FString(string) = expression {
            if !self
                .statements
                .ignored_fstrings
                .contains(&expression.start().to_u32())
                && !string
                    .value
                    .elements()
                    .any(|element| matches!(element, InterpolatedStringElement::Interpolation(_)))
            {
                self.emit(
                    "W1309",
                    "f-string-without-interpolation",
                    "Using an f-string that does not have any interpolated variables",
                    expression.range(),
                );
            }
        }
        if let Expr::BinOp(binary) = expression {
            if binary.op == Operator::Mod {
                if let Expr::StringLiteral(string) = binary.left.as_ref() {
                    if !string.value.to_str().contains(['{', '}'])
                        && !self.source.text[binary.right.range()].contains('\\')
                        && !matches!(self.static_value(&binary.right), Value::Sequence(ref values, "list") if values.len() > 1)
                        && !matches!(self.static_value(&binary.right), Value::Mapping(ref values) if values.len() > 1)
                    {
                        self.emit(
                            "C0209",
                            "consider-using-f-string",
                            "Formatting a regular string which could be an f-string",
                            string.range(),
                        );
                        if let Expr::StringLiteral(right) = binary.right.as_ref() {
                            self.emit(
                                "C0209",
                                "consider-using-f-string",
                                "Formatting a regular string which could be an f-string",
                                right.range(),
                            );
                        }
                    }
                }
            }
        }
        let Expr::Call(call) = expression else { return };
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return;
        };
        if attribute.attr.as_str() != "format" {
            return;
        }
        let Expr::StringLiteral(string) = attribute.value.as_ref() else {
            return;
        };
        let Ok(fields) = super::super::string_format::field_names(string.value.to_str()) else {
            return;
        };
        let invalid_argument = call.arguments.args.iter().any(|argument| {
            self.source.text[argument.range()].contains('\\')
                || matches!(argument, Expr::Starred(starred) if matches!(self.static_value(&starred.value), Value::Sequence(ref values, "list") if values.len() > 1))
        });
        let repeated_keyword = call.arguments.args.is_empty()
            && call.arguments.keywords.iter().any(|keyword| {
                keyword.arg.as_ref().is_some_and(|name| {
                    fields
                        .iter()
                        .filter(|field| field.as_str() == name.as_str())
                        .count()
                        > 1
                })
                || matches!(self.static_value(&keyword.value), Value::Mapping(ref items) if items.len() > 1 && fields.len() > 1)
            });
        if !invalid_argument && !repeated_keyword {
            self.emit(
                "C0209",
                "consider-using-f-string",
                "Formatting a regular string which could be an f-string",
                string.range(),
            );
        }
    }
}
