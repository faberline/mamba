//! Function inference borrows the already parsed project syntax.
use super::{callables::Signature, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    Stmt, StmtFunctionDef,
};
use ruff_text_size::Ranged;

struct Finder<'a> {
    offset: u32,
    found: Option<&'a StmtFunctionDef>,
}
impl<'a> Visitor<'a> for Finder<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if let Stmt::FunctionDef(function) = statement {
            if function.name.start().to_u32() == self.offset {
                self.found = Some(function);
                return;
            }
        }
        walk_stmt(self, statement);
    }
}
impl Checker<'_> {
    pub(super) fn source_function(
        &self,
        signature: &Signature,
        origin: &Context,
    ) -> Option<&StmtFunctionDef> {
        let module = match origin {
            Context::Module(module) => module.as_str(),
            Context::Scope(_) => self.module_name,
        };
        let index = self.modules.source_index(self.source.project, module)?;
        let document = self.index.modules[index].document.as_ref()?;
        let mut finder = Finder {
            offset: signature.definition_range.start().to_u32(),
            found: None,
        };
        finder.visit_body(&document.parsed.syntax().body);
        finder.found
    }
}
