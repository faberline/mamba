//! Measure native Black layouts with their source suite's indentation.
use crate::{PyFormatContext, PyFormatOptions};
use ruff_formatter::prelude::Document;
use ruff_formatter::{
    FormatContext, FormatElement, FormatOptions, FormatResult, Formatted, LineWidth, SourceCode,
};

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

struct WidePrintContext<'c, 's> {
    context: &'c PyFormatContext<'s>,
    options: PyFormatOptions,
}

impl FormatContext for WidePrintContext<'_, '_> {
    type Options = PyFormatOptions;
    fn options(&self) -> &Self::Options {
        &self.options
    }
    fn source_code(&self) -> SourceCode<'_> {
        self.context.source_code()
    }
}

pub(crate) fn flat_line_fits(
    elements: &[FormatElement],
    context: &PyFormatContext,
) -> FormatResult<bool> {
    let indent_width = context.options().indent_width();
    let indentation = (u32::from(context.indent_level().to_ascii_spaces(indent_width))
        / indent_width.value()) as u16;
    let wide = WidePrintContext {
        context,
        options: context
            .options()
            .clone()
            .with_line_width(LineWidth::try_from(u16::MAX).unwrap()),
    };
    let printed =
        Formatted::new(Document::from(elements.to_vec()), wide).print_with_indent(indentation)?;
    let code = printed.as_code().trim_end_matches(['\r', '\n']);
    Ok(!code.contains(['\r', '\n'])
        && ruff_formatter::black_width::measure(code)
            <= usize::from(context.options().line_width().value()))
}

pub(crate) fn first_line_fits(
    elements: &[FormatElement],
    context: &PyFormatContext<'_>,
) -> FormatResult<bool> {
    let width = context.options().indent_width();
    let indentation =
        (u32::from(context.indent_level().to_ascii_spaces(width)) / width.value()) as u16;
    let printed = Formatted::new(Document::from(elements.to_vec()), PrintContext(context))
        .print_with_indent(indentation)?;
    Ok(ruff_formatter::black_width::measure(
        printed.as_code().split(['\r', '\n']).next().unwrap_or(""),
    ) <= usize::from(context.options().line_width().value()))
}
