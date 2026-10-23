//! Generic base syntax requires an unshadowed import in this complete module body.
use super::*;
use crate::infrastructure::project;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_except_handler, walk_expr, walk_pattern, walk_stmt, Visitor},
    ExceptHandler, Pattern,
};
use ruff_text_size::TextRange;

pub(super) fn generic_bases(scopes: &Scopes, body: &[Stmt], class: &StmtClassDef) -> Vec<TextSize> {
    // The only callers pass enter's full lexical body or attach_modules' parsed body.
    // Do not infer absence of an outer binding from a function/class suite.
    if scopes.current().kind != Kind::Module
        || !body.iter().any(
            |statement| matches!(statement, Stmt::ClassDef(source) if std::ptr::eq(source, class)),
        )
    {
        return Vec::new();
    }
    let Some(bases) = &class.arguments else {
        return Vec::new();
    };
    let Some(lookup) = &scopes.modules else {
        return Vec::new();
    };
    let imports = project::imports(body, "", false);
    bases
        .args
        .iter()
        .filter_map(|base| {
            let Expr::Subscript(subscript) = base else {
                return None;
            };
            if scopes
                .context
                .typing_form(&subscript.value, &scopes.current().variables)
                .as_deref()
                != Some("Generic")
            {
                return None;
            }
            let (root, member) = match subscript.value.as_ref() {
                Expr::Name(name) => (name.id.as_str(), Some("Generic")),
                Expr::Attribute(attribute) if attribute.attr.as_str() == "Generic" => {
                    let Expr::Name(root) = attribute.value.as_ref() else {
                        return None;
                    };
                    (root.id.as_str(), None)
                }
                _ => return None,
            };
            let bound: Vec<_> = imports
                .iter()
                .filter(|import| import.local == root)
                .collect();
            let [import] = bound.as_slice() else {
                return None;
            };
            if !matches!(import.module.as_str(), "typing" | "typing_extensions")
                || lookup.module_exists(&import.module)
                || !lookup.standard_module_exists(&import.module)
                || import.member.as_deref() != member
                || import.range.end() > class.start()
                || scopes.current().redirects.contains_key(root)
                || scopes
                    .current()
                    .bindings
                    .get(root)
                    .is_none_or(|binding| binding.first_definition != import.range.start())
            {
                return None;
            }
            let mut writes = Writes {
                root,
                import: import.range,
                blocked: false,
            };
            writes.visit_body(body);
            (!writes.blocked).then_some(base.start())
        })
        .collect()
}

struct Writes<'s> {
    root: &'s str,
    import: TextRange,
    blocked: bool,
}

impl Writes<'_> {
    fn name(&mut self, name: &str) {
        self.blocked |= name == self.root;
    }

    fn nested_class_globals(&mut self, body: &[Stmt]) {
        let mut globals = ClassGlobals {
            root: self.root,
            found: false,
        };
        globals.visit_body(body);
        self.blocked |= globals.found;
    }
}

impl<'a> Visitor<'a> for Writes<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                for import in project::imports(std::slice::from_ref(statement), "", false) {
                    if import.member.as_deref() == Some("*")
                        || (import.local == self.root && import.range != self.import)
                    {
                        self.blocked = true;
                    }
                }
            }
            Stmt::FunctionDef(function) => {
                self.name(function.name.as_str());
                // Headers execute here; the function suite has a different owner.
                for decorator in &function.decorator_list {
                    self.visit_expr(&decorator.expression);
                }
                self.visit_parameters(&function.parameters);
                if let Some(returns) = &function.returns {
                    self.visit_expr(returns);
                }
                // Type parameter annotation scopes are not proved by this helper.
                self.blocked |= function.type_params.is_some();
            }
            Stmt::ClassDef(class) => {
                self.name(class.name.as_str());
                for decorator in &class.decorator_list {
                    self.visit_expr(&decorator.expression);
                }
                if let Some(arguments) = &class.arguments {
                    for argument in &arguments.args {
                        self.visit_expr(argument);
                    }
                    for keyword in &arguments.keywords {
                        self.visit_expr(&keyword.value);
                    }
                }
                self.blocked |= class.type_params.is_some();
                // Ordinary class-local stores do not shadow this module import.
                self.nested_class_globals(&class.body);
            }
            Stmt::Global(global) => {
                for name in &global.names {
                    self.name(name.as_str());
                }
            }
            Stmt::Nonlocal(nonlocal) => {
                for name in &nonlocal.names {
                    self.name(name.as_str());
                }
            }
            _ => walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Name(name) if !name.ctx.is_load() => self.name(name.id.as_str()),
            Expr::Attribute(attribute) if !attribute.ctx.is_load() => {
                if let Expr::Name(root) = attribute.value.as_ref() {
                    self.name(root.id.as_str());
                } else {
                    // Nested attribute stores have no direct module-owner proof.
                    self.blocked = true;
                }
                walk_expr(self, expression);
            }
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameters(parameters);
                }
            }
            // Targets are local but walrus owners need more semantic state.
            // Retain unknown instead of treating a local target as a module store.
            Expr::Generator(_) | Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) => {
                self.blocked = true;
            }
            _ => walk_expr(self, expression),
        }
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        let name = match pattern {
            Pattern::MatchAs(value) => value.name.as_ref(),
            Pattern::MatchStar(value) => value.name.as_ref(),
            Pattern::MatchMapping(value) => value.rest.as_ref(),
            _ => None,
        };
        if let Some(name) = name {
            self.name(name.as_str());
        }
        walk_pattern(self, pattern);
    }

    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(value) = handler;
        if let Some(name) = &value.name {
            self.name(name.as_str());
        }
        walk_except_handler(self, handler);
    }
}

struct ClassGlobals<'s> {
    root: &'s str,
    found: bool,
}

impl<'a> Visitor<'a> for ClassGlobals<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Global(global) => {
                self.found |= global.names.iter().any(|name| name.as_str() == self.root);
            }
            Stmt::Nonlocal(nonlocal) => {
                self.found |= nonlocal.names.iter().any(|name| name.as_str() == self.root);
            }
            Stmt::FunctionDef(_) => {}
            _ => walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, _expression: &'a Expr) {}
}
