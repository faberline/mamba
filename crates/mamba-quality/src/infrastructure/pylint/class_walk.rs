//! Class declarations retain decorator, base, and lexical traversal order.
use super::{docstring, scopes::Kind, Checker};
use mamba_language::application::quality_syntax::python_ast::{visitor::Visitor, StmtClassDef};
use ruff_text_size::Ranged;
impl Checker<'_> {
    pub(super) fn walk_class_declaration(&mut self, class: &StmtClassDef) {
        self.check_declaration_name(
            class.name.as_str(),
            "class",
            self.declaration_range(class.name.range()),
        );
        if !class.name.as_str().starts_with('_')
            && !docstring(&class.body)
            && !self.assigned_class_docstring(&class.body)
        {
            self.emit(
                "C0115",
                "missing-class-docstring",
                "Missing class docstring",
                self.declaration_range(class.name.range()),
            );
        }
        self.check_empty_docstring(
            &class.body,
            "class",
            self.declaration_range(class.name.range()),
        );
        self.check_non_ascii_name(
            class.name.as_str(),
            "Class",
            self.declaration_range(class.name.range()),
        );
        self.check_bases(class);
        self.check_redefinition(class.name.range(), "class");
        self.check_metaclass(class);
        self.check_deprecated_decorators(&class.decorator_list);
        for decorator in &class.decorator_list {
            self.visit_expr(&decorator.expression);
        }
        self.enter_generic_names(class.type_params.as_deref());
        if let Some(parameters) = &class.type_params {
            self.visit_type_params(parameters);
        }
        if let Some(arguments) = &class.arguments {
            for base in &arguments.args {
                self.visit_expr(base);
            }
            for keyword in &arguments.keywords {
                let metaclass = keyword
                    .arg
                    .as_ref()
                    .is_some_and(|name| name.as_str() == "metaclass");
                if metaclass {
                    if let Some(path) = super::values::path(&keyword.value) {
                        if let Some(name) = path.first() {
                            self.scopes
                                .use_name(if name == "_py_abc" { "abc" } else { name });
                        }
                    }
                } else {
                    self.visit_expr(&keyword.value);
                }
            }
        }
        self.enter_class(class);
        let previous_method = self.method_context.take();
        let previous_receiver = self.mandatory_receiver.take();
        self.scopes.enter(Kind::Class, &class.body, None);
        self.visit_body(&class.body);
        self.leave_scope();
        self.leave_class(class);
        self.generic_names.pop();
        self.method_context = previous_method;
        self.mandatory_receiver = previous_receiver;
    }
}
