//! Plain literal format fields map only to already checked direct arguments.
use super::*;
use quality_syntax::python_ast::ExprCall;

#[derive(Clone, Copy)]
enum Root<'a> {
    Automatic,
    Position(usize),
    Named(&'a str),
}

struct Field<'a> {
    root: Root<'a>,
    converted: bool,
}

struct Replacement<'a> {
    expression: &'a Expr,
    converted: bool,
}

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn visit_literal_format<'a>(
        &mut self,
        expression: &'a Expr,
    ) -> bool
    where
        Self: Visitor<'a>,
    {
        let Expr::Call(call) = expression else {
            return false;
        };
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return false;
        };
        if attribute.attr.as_str() != "format" {
            return false;
        }
        let Expr::StringLiteral(literal) = attribute.value.as_ref() else {
            return false;
        };
        let value = literal.value.to_str();
        let plan =
            if let Some(direct) = parse_fields(value).and_then(|fields| map_fields(call, fields)) {
                FormatPlan::Replacements(
                    direct
                        .into_iter()
                        .map(|replacement| CheckedReplacement {
                            expression: replacement.expression,
                            projection: Projection::Direct,
                            attributes: Vec::new(),
                            converted: replacement.converted,
                        })
                        .collect(),
                )
            } else {
                let Some(plan) =
                    parse_checked_fields(value).and_then(|fields| map_checked_fields(call, fields))
                else {
                    return false;
                };
                plan
            };
        if matches!(&plan, FormatPlan::Replacements(replacements) if replacements.iter().all(|replacement| replacement.converted))
        {
            // Empty and explicitly converted fields need no bytes warning stage.
            return false;
        }
        // Capture each checked argument before a later child can rebind its names.
        // The owned scope guard restores a surrounding FString/format observation map.
        let _guard = self.fstring_observed.scope(Some(Default::default()));
        walk_expr(self, expression);
        let replacements = match plan {
            FormatPlan::MixedNumbering => {
                // Mypy stops before replacement/type checks for mixed numbering.
                self.emit(
                    "str-format",
                    "Cannot combine automatic field numbering and manual field specification"
                        .into(),
                    call,
                );
                return true;
            }
            FormatPlan::Replacements(replacements) => replacements,
        };
        for replacement in replacements {
            if replacement.converted {
                continue;
            }
            let Some(actual) = checked_replacement_type(&self.context, &replacement, &|original| {
                self.observed_fstring_type(original)
            }) else {
                continue;
            };
            if self.fstring_bytes_warning(actual, &mut BTreeSet::new()) {
                self.emit(
                    "str-bytes-safe",
                    "If x = b'abc' then f\"{x}\" or \"{}\".format(x) produces \"b'abc'\", not \"abc\". If this is desired behavior, use f\"{x!r}\" or \"{!r}\".format(x). Otherwise, decode the bytes".into(),
                    call,
                );
            }
        }
        true
    }
}

// None means unsupported, not a clean formatting verdict. Parse the complete value
// before activating observations or emitting any warning.
fn parse_fields(value: &str) -> Option<Vec<Field<'_>>> {
    parse_fields_with(value, parse_field)
}

fn parse_fields_with<'s, F>(
    value: &'s str,
    parse: impl Fn(&'s str) -> Option<F>,
) -> Option<Vec<F>> {
    let bytes = value.as_bytes();
    let mut fields = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'{' if bytes.get(index + 1) == Some(&b'{') => index += 2,
            b'}' if bytes.get(index + 1) == Some(&b'}') => index += 2,
            b'{' => {
                let start = index + 1;
                let end = start + bytes[start..].iter().position(|&byte| byte == b'}')?;
                fields.push(parse(&value[start..end])?);
                index = end + 1;
            }
            b'}' => return None,
            _ => index += 1,
        }
    }
    Some(fields)
}

fn parse_field(value: &str) -> Option<Field<'_>> {
    let split = value.find(['!', ':']).unwrap_or(value.len());
    let key = &value[..split];
    let mut suffix = &value[split..];
    let converted = if let Some(conversion) = suffix.strip_prefix('!') {
        if !matches!(conversion.as_bytes().first(), Some(b'r' | b's' | b'a')) {
            return None;
        }
        suffix = &conversion[1..];
        true
    } else {
        false
    };
    // Only flat, flag-free string specs share this bytes-warning check.
    if let Some(spec) = suffix.strip_prefix(':') {
        if !flat_string_spec(spec) {
            return None;
        }
    } else if !suffix.is_empty() {
        return None;
    }
    let root = if key.is_empty() {
        Root::Automatic
    } else if key.bytes().all(|byte| byte.is_ascii_digit()) {
        Root::Position(key.parse().ok()?)
    } else if key
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        Root::Named(key)
    } else {
        return None;
    };
    Some(Field { root, converted })
}

// Admit [[fill]align][width][.precision][s] only. Numeric flags/types,
// nested/dynamic fields and custom syntax must keep the ordinary path.
fn flat_string_spec(value: &str) -> bool {
    let chars: Vec<_> = value.chars().collect();
    if chars
        .iter()
        .any(|character| matches!(*character, '{' | '}') || character.is_control())
    {
        return false;
    }
    let align = |character: &char| matches!(*character, '<' | '>' | '^');
    let mut index = if chars.get(1).is_some_and(align) {
        2
    } else if chars.first().is_some_and(align) {
        1
    } else {
        0
    };
    // A leading 0 here is a numeric flag, unlike an explicit fill in "0>8".
    if chars.get(index) == Some(&'0') {
        return false;
    }
    while chars.get(index).is_some_and(char::is_ascii_digit) {
        index += 1;
    }
    if chars.get(index) == Some(&'.') {
        index += 1;
        let start = index;
        while chars.get(index).is_some_and(char::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    if chars.get(index) == Some(&'s') {
        index += 1;
    }
    index == chars.len()
}

fn map_fields<'a>(call: &'a ExprCall, fields: Vec<Field<'_>>) -> Option<Vec<Replacement<'a>>> {
    if call
        .arguments
        .args
        .iter()
        .any(|argument| matches!(argument, Expr::Starred(_)))
    {
        return None;
    }
    let mut names = BTreeSet::new();
    for keyword in &call.arguments.keywords {
        // A missing name is **kwargs; duplicates do not identify one replacement.
        if !names.insert(keyword.arg.as_ref()?.as_str()) {
            return None;
        }
    }
    let automatic = fields
        .iter()
        .any(|field| matches!(field.root, Root::Automatic));
    let numbered = fields
        .iter()
        .any(|field| matches!(field.root, Root::Position(_)));
    if automatic && numbered {
        return None;
    }
    let positional = call.arguments.args.len();
    let mut used = vec![false; positional + call.arguments.keywords.len()];
    let mut next = 0;
    let mut replacements = Vec::new();
    for field in fields {
        let (index, expression) = match field.root {
            Root::Automatic => {
                let index = next;
                next += 1;
                (index, call.arguments.args.get(index)?)
            }
            Root::Position(index) => (index, call.arguments.args.get(index)?),
            Root::Named(name) => {
                let index = call.arguments.keywords.iter().position(|keyword| {
                    keyword.arg.as_ref().is_some_and(|arg| arg.as_str() == name)
                })?;
                (positional + index, &call.arguments.keywords[index].value)
            }
        };
        used[index] = true;
        replacements.push(Replacement {
            expression,
            converted: field.converted,
        });
    }
    // Extra arguments would require the upstream string-formatting diagnostic too.
    used.into_iter().all(|used| used).then_some(replacements)
}

#[derive(Clone, Copy)]
enum Projection {
    Direct,
    Sequence,
    Mapping,
}

struct CheckedField<'s> {
    field: Field<'s>,
    attributes: Vec<&'s str>,
}

struct CheckedReplacement<'a, 's> {
    expression: &'a Expr,
    projection: Projection,
    attributes: Vec<&'s str>,
    converted: bool,
}

enum FormatPlan<'a, 's> {
    MixedNumbering,
    Replacements(Vec<CheckedReplacement<'a, 's>>),
}

fn parse_checked_fields(value: &str) -> Option<Vec<CheckedField<'_>>> {
    parse_fields_with(value, |value| {
        let split = value.find(['!', ':']).unwrap_or(value.len());
        let mut components = value[..split].split('.');
        let root = parse_field(components.next()?)?.root;
        let converted = parse_field(&value[split..])?.converted;
        let attributes = components.collect::<Vec<_>>();
        if !attributes.iter().all(|name| {
            name.bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        }) {
            return None;
        }
        Some(CheckedField {
            field: Field { root, converted },
            attributes,
        })
    })
}

// Select explicit arguments first, then the first star/dstar checked type.
// Record only original AST addresses and field metadata, never AST copies.
fn map_checked_fields<'a, 's>(
    call: &'a ExprCall,
    fields: Vec<CheckedField<'s>>,
) -> Option<FormatPlan<'a, 's>> {
    let automatic = fields
        .iter()
        .any(|field| matches!(field.field.root, Root::Automatic));
    let numbered = fields
        .iter()
        .any(|field| matches!(field.field.root, Root::Position(_)));
    if automatic && numbered {
        return Some(FormatPlan::MixedNumbering);
    }
    let mut names = BTreeSet::new();
    for keyword in &call.arguments.keywords {
        if let Some(name) = &keyword.arg {
            if !names.insert(name.as_str()) {
                return None;
            }
        }
    }
    let positional = call.arguments.args.len();
    let mut used = vec![false; positional + call.arguments.keywords.len()];
    let mut next = 0;
    let mut replacements = Vec::new();
    for field in fields {
        let root = match field.field.root {
            Root::Automatic => {
                let index = next;
                next += 1;
                Root::Position(index)
            }
            root => root,
        };
        let (expression, projection) = match root {
            Root::Position(index) => {
                if let Some((slot, expression)) = call
                    .arguments
                    .args
                    .iter()
                    .enumerate()
                    .filter(|(_, expression)| !matches!(expression, Expr::Starred(_)))
                    .nth(index)
                {
                    used[slot] = true;
                    (expression, Projection::Direct)
                } else {
                    let star =
                        call.arguments
                            .args
                            .iter()
                            .find_map(|expression| match expression {
                                Expr::Starred(star) => Some(star),
                                _ => None,
                            })?;
                    (star.value.as_ref(), Projection::Sequence)
                }
            }
            Root::Named(name) => {
                if let Some((slot, keyword)) =
                    call.arguments
                        .keywords
                        .iter()
                        .enumerate()
                        .find(|(_, keyword)| {
                            keyword.arg.as_ref().is_some_and(|arg| arg.as_str() == name)
                        })
                {
                    used[positional + slot] = true;
                    (&keyword.value, Projection::Direct)
                } else {
                    let keyword = call
                        .arguments
                        .keywords
                        .iter()
                        .find(|keyword| keyword.arg.is_none())?;
                    (&keyword.value, Projection::Mapping)
                }
            }
            Root::Automatic => unreachable!("automatic root was assigned above"),
        };
        replacements.push(CheckedReplacement {
            expression,
            projection,
            attributes: field.attributes,
            converted: field.field.converted,
        });
    }
    // Preserve the old unsupported-extra-argument boundary. Star/dstar temporary
    // replacements do not count as explicit arguments in the upstream check.
    if call
        .arguments
        .args
        .iter()
        .enumerate()
        .any(|(slot, expression)| !matches!(expression, Expr::Starred(_)) && !used[slot])
        || call
            .arguments
            .keywords
            .iter()
            .enumerate()
            .any(|(slot, keyword)| keyword.arg.is_some() && !used[positional + slot])
    {
        return None;
    }
    Some(FormatPlan::Replacements(replacements))
}

fn checked_replacement_type(
    context: &types::Context,
    replacement: &CheckedReplacement<'_, '_>,
    checked: &impl Fn(&Expr) -> Option<Option<Type>>,
) -> Option<Type> {
    // Both an observed None and a genuine miss stop here. Never re-infer a name.
    let mut actual = checked(replacement.expression)??;
    actual = match replacement.projection {
        Projection::Direct => actual,
        Projection::Mapping => context
            .mapping_types(actual)
            .map(|(_, value)| value)
            .unwrap_or(Type::Any),
        Projection::Sequence => match context.node(actual).as_deref() {
            // Only fixed TupleType is not an upstream Sequence Instance here.
            Some(Node::Tuple(_)) => Type::Any,
            _ => context.sequence_item(actual).unwrap_or(Type::Any),
        },
    };
    for name in &replacement.attributes {
        if actual.is_any() {
            actual = actual.derived_any();
        } else {
            actual = context.class_field(actual, name)?.ty?;
        }
    }
    Some(actual)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod flat_spec_tests;

#[cfg(test)]
mod mapping_boundary_tests;
