//! Literal descriptor assignments retain Astroid's extra-decorator method kinds.
use super::MethodKind;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Expr, Stmt,
};
#[derive(Default)]
struct Collector {
    name: String,
    kind: Option<MethodKind>,
}
impl<'a> Visitor<'a> for Collector {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Assign(assignment)
                if assignment.targets.iter().any(
                    |value| matches!(value, Expr::Name(value) if value.id.as_str() == self.name),
                ) =>
            {
                if let Expr::Call(call) = assignment.value.as_ref() {
                    if let Expr::Name(name) = call.func.as_ref() {
                        self.kind = match name.id.as_str() {
                            "classmethod" => Some(MethodKind::Class),
                            "staticmethod" => Some(MethodKind::Static),
                            _ => self.kind,
                        };
                    }
                }
            }
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => walk_stmt(self, statement),
        }
    }
}
pub(super) fn kind(body: &[Stmt], name: &str) -> Option<MethodKind> {
    let mut collector = Collector {
        name: name.into(),
        ..Collector::default()
    };
    collector.visit_body(body);
    collector.kind
}
