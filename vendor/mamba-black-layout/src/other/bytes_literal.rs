use ruff_python_ast::BytesLiteral;
use ruff_text_size::Ranged;

use crate::prelude::*;
use crate::string::StringNormalizer;

#[derive(Default)]
pub struct FormatBytesLiteral;

impl FormatNodeRule<BytesLiteral> for FormatBytesLiteral {
    fn fmt_fields(&self, item: &BytesLiteral, f: &mut PyFormatter) -> FormatResult<()> {
        if let Some(policy) = f.options().style_policy() {
            let normalized = policy.literal(&f.context().source()[item.range()]);
            return text(&normalized).fmt(f);
        }
        StringNormalizer::from_context(f.context())
            .normalize(item.into())
            .fmt(f)
    }
}
