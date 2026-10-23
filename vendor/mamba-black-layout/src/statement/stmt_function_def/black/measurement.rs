//! Print a shared document fragment without changing the Python formatting state.
use crate::{PyFormatContext, PyFormatOptions};
use ruff_formatter::prelude::Document;
use ruff_formatter::{FormatContext, FormatElement, FormatResult, Formatted, SourceCode};

struct PrintContext<'c, 's>(&'c PyFormatContext<'s>);

impl FormatContext for PrintContext<'_, '_> {
    type Options = PyFormatOptions;

    fn options(&self) -> &Self::Options {
        self.0.options()
    }

    fn source_code(&self) -> SourceCode<'_> {
        self.0.source_code()
    }
}

pub(super) fn flat_width(
    elements: &[FormatElement],
    context: &PyFormatContext<'_>,
) -> FormatResult<Option<usize>> {
    let printed =
        Formatted::new(Document::from(elements.to_vec()), PrintContext(context)).print()?;
    Ok((!printed.as_code().contains(['\r', '\n']))
        .then(|| ruff_formatter::black_width::measure(printed.as_code())))
}
