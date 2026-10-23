//! Optional native policies for consumers of the shared document layout.
pub trait StylePolicy: std::fmt::Debug + Send + Sync {
    fn literal(&self, source: &str) -> String;
    fn number(&self, source: &str) -> String;
    fn interpolated(&self, source: &str, expression_has_backslash: bool) -> String;
    fn comment(&self, source: &str) -> String;
}
