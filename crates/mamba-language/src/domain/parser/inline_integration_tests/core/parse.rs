use super::*;

pub(super) fn parse(src: &str) -> Module {
    parser::parse(src, FileId(0)).expect("parse failed")
}
