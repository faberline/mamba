use super::*;

pub(super) fn desugared(src: &str) -> Module {
    let mut module = parser::parse(src, FileId(0)).expect("parse failed");
    desugar_module(&mut module);
    module
}
