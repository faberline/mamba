use super::interpolated_string_element::FormatInterpolatedStringElement;
use crate::context::WithInterpolatedStringState;
use crate::other::interpolated_string::{InterpolatedStringContext, InterpolatedStringLayout};
use crate::prelude::*;
use crate::string::{StringNormalizer, StringQuotes};
use ruff_formatter::write;
use ruff_python_ast::{StringFlags, TString};
use ruff_text_size::Ranged;

/// Formats a t-string which is part of a larger t-string expression.
///
/// For example, this would be used to format the t-string part in `"foo" t"bar {x}"`
/// or the standalone t-string in `t"foo {x} bar"`.
#[derive(Default)]
pub struct FormatTString;

impl FormatNodeRule<TString> for FormatTString {
    fn fmt_fields(&self, item: &TString, f: &mut PyFormatter) -> FormatResult<()> {
        if let Some(policy) = f.options().style_policy() {
            let source = f.context().source();
            let backslash = item
                .elements
                .interpolations()
                .any(|part| source[part.range()].contains('\\'));
            let normalized = policy.interpolated(&source[item.range()], backslash);
            let candidate = crate::other::f_string::black_skipped_interpolation_candidate(
                &item.elements,
                f,
                &normalized,
            )
            .unwrap_or(normalized);
            f.context()
                .comments()
                .mark_verbatim_node_comments_formatted(item.into());
            return text(&candidate).fmt(f);
        }
        let normalizer = StringNormalizer::from_context(f.context());

        let string_kind = normalizer.choose_quotes(item.into()).flags();

        let context = InterpolatedStringContext::new(
            string_kind,
            InterpolatedStringLayout::from_interpolated_string_elements(
                &item.elements,
                f.context().source(),
            ),
        );

        // Starting prefix and quote
        let quotes = StringQuotes::from(string_kind);
        write!(f, [string_kind.prefix(), quotes])?;

        {
            let state = f
                .context()
                .interpolated_string_state()
                .enter_string(context);
            let f = &mut WithInterpolatedStringState::new(state, &mut *f);
            for element in &item.elements {
                FormatInterpolatedStringElement::new(element, context).fmt(f)?;
            }
        }

        // Ending quote
        quotes.fmt(f)
    }
}
