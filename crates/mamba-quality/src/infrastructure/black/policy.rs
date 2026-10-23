//! Fixed Black behavior implemented in Rust. These are not public settings.
use mamba_black_layout::style::StylePolicy;

#[derive(Debug)]
pub struct BlackPolicy;

impl StylePolicy for BlackPolicy {
    fn literal(&self, source: &str) -> String {
        super::literals::string(source)
    }

    fn number(&self, source: &str) -> String {
        super::literals::number(source)
    }

    fn interpolated(&self, source: &str, expression_has_backslash: bool) -> String {
        if expression_has_backslash {
            source.into()
        } else {
            super::literals::string(source)
        }
    }

    fn comment(&self, source: &str) -> String {
        super::comments::normalize(source)
    }
}
