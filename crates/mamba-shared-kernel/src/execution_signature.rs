use crate::ast::Param;

/// Static function shape extracted from a literal execution statement.
#[derive(Clone)]
pub struct ExecLiteralFnDef {
    pub name: String,
    pub params: Vec<Param>,
}
