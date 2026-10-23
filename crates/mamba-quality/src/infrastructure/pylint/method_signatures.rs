//! Override checks use Pylint's positional, keyword, and variadic policy.
use super::{
    callables::{Kind, Parameter, Signature},
    classes::MethodKind,
    defaults, Checker,
};
use mamba_language::application::quality_syntax::python_ast::StmtFunctionDef;

fn positional(signature: &Signature, kind: MethodKind) -> Vec<&Parameter> {
    signature
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == Kind::Positional)
        .skip(usize::from(kind == MethodKind::Class))
        .collect()
}

fn keyword(signature: &Signature) -> Vec<&Parameter> {
    signature
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == Kind::KeywordOnly)
        .collect()
}

fn positional_differences(original: &[&Parameter], current: &[&Parameter]) -> Vec<String> {
    let mut messages = Vec::new();
    for index in 0..original.len().max(current.len()) {
        let Some(current) = current.get(index) else {
            return vec!["Number of parameters ".into()];
        };
        let Some(original) = original.get(index) else {
            if current.required {
                return vec!["Number of parameters ".into()];
            }
            continue;
        };
        if defaults::fixed().ignored(&original.name, false)
            || defaults::fixed().ignored(&current.name, false)
        {
            continue;
        }
        if original.name != current.name {
            messages.push(format!(
                "Parameter '{}' has been renamed to '{}' in",
                original.name, current.name
            ));
        }
    }
    messages
}

fn keyword_difference(original: &[&Parameter], current: &[&Parameter]) -> bool {
    original
        .iter()
        .any(|parameter| !current.iter().any(|value| value.name == parameter.name))
        || current.iter().any(|parameter| {
            parameter.required && !original.iter().any(|value| value.name == parameter.name)
        })
}

fn count(signature: &Signature) -> usize {
    signature
        .parameters
        .iter()
        .filter(|parameter| parameter.kind != Kind::PositionalOnly)
        .count()
        + usize::from(signature.variadic)
        + usize::from(signature.keywords.is_some())
}

fn defaults_count(signature: &Signature) -> usize {
    signature
        .parameters
        .iter()
        .filter(|parameter| parameter.kind != Kind::KeywordOnly && !parameter.required)
        .count()
}

impl Checker<'_> {
    pub(super) fn check_override_signature(
        &mut self,
        function: &StmtFunctionDef,
        kind: MethodKind,
        original: &Signature,
        original_kind: MethodKind,
        parent: &str,
        class: &str,
    ) {
        let name = function.name.as_str();
        if (name.starts_with("__") && !name.ends_with("__")) || function.decorator_list.iter().any(|value| matches!(&value.expression, mamba_language::application::quality_syntax::python_ast::Expr::Attribute(value) if value.attr.as_str() == "setter")) {
            return;
        }
        let current = Signature::from_function(function);
        let mut original_positional = positional(original, original_kind);
        let current_positional = positional(&current, kind);
        let mut original_keyword = keyword(original);
        let current_keyword = keyword(&current);
        if current.variadic {
            original_positional.retain(|parameter| {
                current_positional
                    .iter()
                    .any(|value| value.name == parameter.name)
            });
        }
        if current.keywords.is_some() {
            original_keyword.retain(|parameter| {
                current_keyword
                    .iter()
                    .any(|value| value.name == parameter.name)
            });
        }
        let mut messages = positional_differences(&original_positional, &current_positional);
        if keyword_difference(&original_keyword, &current_keyword)
            && !messages
                .first()
                .is_some_and(|message| message.starts_with("Number "))
        {
            messages.push("Number of parameters ".into());
        }
        if (original.variadic && !current.variadic)
            || (original.keywords.is_some() && current.keywords.is_none())
        {
            messages.push("Variadics removed in".into());
        }
        if matches!(
            name,
            "__new__" | "__init__" | "__init_subclass__" | "__post_init__"
        ) {
            messages.clear();
        } else if defaults::fixed().special_method(name) {
            messages.retain(|message| message.contains("Number"));
        }
        let range = self.declaration_range(function.name.range);
        if messages.is_empty()
            && defaults_count(&current) < defaults_count(original)
            && !current.variadic
        {
            self.emit(
                "W0222",
                "signature-differs",
                &format!("Signature differs from overridden '{name}' method"),
                range,
            );
        }
        for mut message in messages {
            let (rule, symbol) = if message.contains("renamed") {
                ("W0237", "arguments-renamed")
            } else {
                ("W0221", "arguments-differ")
            };
            if message.contains("Number") {
                message.push_str(&format!(
                    "was {} in '{parent}.{name}' and is now {} in",
                    count(original),
                    count(&current)
                ));
            }
            message.push_str(&format!(" overriding '{class}.{name}' method"));
            self.emit(rule, symbol, &message, range);
        }
    }
}
