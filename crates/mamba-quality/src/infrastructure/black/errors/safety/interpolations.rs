//! CPython interpolated strings coalesce literal chunks and retain field spelling.
use super::{
    ast::{Builder, Result},
    tree::{self, Value},
};
use ruff_python_ast::{
    ConversionFlag, FStringPartRef, FStringValue, InterpolatedElement, InterpolatedStringElement,
    InterpolatedStringElements, TStringValue,
};
use ruff_text_size::Ranged;

// CPython's tokenizer keeps field source until !, :, or }, removes comments
// outside quoted strings, and normalizes physical newlines before parsing.
fn interpolation_metadata(source: &str) -> std::result::Result<String, String> {
    let source = source.replace("\r\n", "\n").replace('\r', "\n");
    let bytes = source.as_bytes();
    let mut quote = None;
    let mut index = 0;
    let mut has_comment = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\\' {
            index += 2;
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            if quote.is_none() {
                quote = Some(byte);
            } else if quote == Some(byte) {
                quote = None;
            }
        } else if byte == b'#' && quote.is_none() {
            has_comment = true;
            break;
        }
        index += 1;
    }
    if !has_comment {
        return Ok(source);
    }
    // Match set_ftstring_expr's second pass, including its quote handling.
    let mut metadata = Vec::with_capacity(bytes.len());
    quote = None;
    index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if matches!(byte, b'\'' | b'"') {
            if quote.is_none() {
                quote = Some(byte);
            } else if quote == Some(byte) {
                quote = None;
            }
            metadata.push(byte);
        } else if byte == b'#' && quote.is_none() {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            if index < bytes.len() {
                metadata.push(b'\n');
            }
        } else {
            metadata.push(byte);
        }
        index += 1;
    }
    String::from_utf8(metadata).map_err(|_| "Invalid template interpolation metadata".into())
}

fn interpolation_spelling(metadata: &str) -> &str {
    // _strip_interpolation_expr removes only trailing Python whitespace and =.
    // Python additionally treats the four ASCII information separators as space.
    metadata.trim_end_matches(|character: char| {
        character == '=' || character.is_whitespace() || matches!(character, '\u{1c}'..='\u{1f}')
    })
}

fn flush_literal(literal: &mut String, values: &mut Vec<Value>) {
    if !literal.is_empty() {
        values.push(tree::constant(tree::string(std::mem::take(literal))));
    }
}
impl Builder<'_> {
    fn field_metadata(&self, value: &InterpolatedElement) -> std::result::Result<String, String> {
        let start = value
            .start()
            .to_u32()
            .checked_add(1)
            .ok_or("Template interpolation range overflows")?;
        let end = value.end().to_u32();
        let expression_end = value.expression.end().to_u32();
        let expected = if value.conversion != ConversionFlag::None {
            "Exclamation"
        } else if value.format_spec.is_some() {
            "Colon"
        } else {
            "Rbrace"
        };
        // Expr ranges can omit grouping parentheses and leading/trailing trivia.
        // Start at the field's { and find its suffix in the parsed token stream.
        // Nested delimiters and !/: inside the expression precede expression_end.
        let first = self
            .document
            .tokens
            .partition_point(|token| token.start < expression_end);
        let boundary = self.document.tokens[first..]
            .iter()
            .take_while(|token| token.start < end)
            .find(|token| matches!(token.kind.as_str(), "Exclamation" | "Colon" | "Rbrace"))
            .filter(|token| token.kind == expected && token.end <= end)
            .ok_or("Template interpolation suffix is absent from parsed tokens")?;
        let source = self.document.source;
        if source.get(value.start().to_u32() as usize..start as usize) != Some("{") {
            return Err("Template interpolation does not start with a brace".into());
        }
        let text = source
            .get(start as usize..boundary.start as usize)
            .ok_or("Template interpolation source range is invalid")?;
        interpolation_metadata(text)
    }

    pub fn tstring(&self, value: &TStringValue) -> Result {
        let mut values = Vec::new();
        let mut literal = String::new();
        for part in value.iter() {
            for element in &part.elements {
                match element {
                    InterpolatedStringElement::Literal(v) => literal.push_str(&v.value),
                    InterpolatedStringElement::Interpolation(v) => {
                        let metadata = self.field_metadata(v)?;
                        if v.debug_text.is_some() {
                            literal.push_str(&metadata);
                        }
                        flush_literal(&mut literal, &mut values);
                        let conversion = if v.debug_text.is_some()
                            && v.conversion == ConversionFlag::None
                            && v.format_spec.is_none()
                        {
                            ConversionFlag::Repr
                        } else {
                            v.conversion
                        };
                        let format_spec = if let Some(spec) = &v.format_spec {
                            let mut parts = Vec::new();
                            let mut text = String::new();
                            self.formatted_elements(&spec.elements, &mut text, &mut parts)?;
                            flush_literal(&mut text, &mut parts);
                            tree::node("JoinedStr", [("values", Value::List(parts))])
                        } else {
                            tree::none()
                        };
                        values.push(tree::node(
                            "Interpolation",
                            [
                                ("value", self.expr(&v.expression)?),
                                ("str", tree::string(interpolation_spelling(&metadata))),
                                ("conversion", tree::integer(conversion as i8)),
                                ("format_spec", format_spec),
                            ],
                        ));
                    }
                }
            }
        }
        flush_literal(&mut literal, &mut values);
        Ok(tree::node("TemplateStr", [("values", Value::List(values))]))
    }

    fn formatted_elements(
        &self,
        elements: &InterpolatedStringElements,
        literal: &mut String,
        values: &mut Vec<Value>,
    ) -> std::result::Result<(), String> {
        for element in elements {
            match element {
                InterpolatedStringElement::Literal(v) => literal.push_str(&v.value),
                InterpolatedStringElement::Interpolation(v) => {
                    if v.debug_text.is_some() {
                        literal.push_str(&self.field_metadata(v)?);
                    }
                    flush_literal(literal, values);
                    let conversion = if v.debug_text.is_some()
                        && v.conversion == ConversionFlag::None
                        && v.format_spec.is_none()
                    {
                        ConversionFlag::Repr
                    } else {
                        v.conversion
                    };
                    let format_spec = if let Some(spec) = &v.format_spec {
                        let mut parts = Vec::new();
                        let mut text = String::new();
                        self.formatted_elements(&spec.elements, &mut text, &mut parts)?;
                        flush_literal(&mut text, &mut parts);
                        tree::node("JoinedStr", [("values", Value::List(parts))])
                    } else {
                        tree::none()
                    };
                    values.push(tree::node(
                        "FormattedValue",
                        [
                            ("value", self.expr(&v.expression)?),
                            ("conversion", tree::integer(conversion as i8)),
                            ("format_spec", format_spec),
                        ],
                    ));
                }
            }
        }
        Ok(())
    }
    pub fn fstring(&self, value: &FStringValue) -> Result {
        let mut values = Vec::new();
        let mut literal = String::new();
        for part in value.iter() {
            match part {
                FStringPartRef::Literal(v) => literal.push_str(&v.value),
                FStringPartRef::FString(v) => {
                    self.formatted_elements(&v.elements, &mut literal, &mut values)?
                }
            }
        }
        flush_literal(&mut literal, &mut values);
        Ok(tree::node("JoinedStr", [("values", Value::List(values))]))
    }
}
