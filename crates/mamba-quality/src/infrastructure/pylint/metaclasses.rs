//! Declared metaclasses retain first-result inference and Python diagnostic names.
use super::{
    callables::{Returns, Target},
    operand_inference::Operand,
    resolution::Context,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, Stmt, StmtClassDef};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn check_unresolved_metaclasses(&mut self, body: &[Stmt]) {
        for statement in body {
            let Stmt::ClassDef(class) = statement else {
                continue;
            };
            let Some(keyword) = class.arguments.as_ref().and_then(|arguments| {
                arguments.keywords.iter().find(|keyword| {
                    keyword
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "metaclass")
                })
            }) else {
                continue;
            };
            let expression = match &keyword.value {
                Expr::Call(call) => call.func.as_ref(),
                value => value,
            };
            let Some(path) = super::values::path(expression) else {
                continue;
            };
            let Some(name) = path.first() else {
                continue;
            };
            let origin = Context::Scope(self.scopes.current_index());
            let model = std::rc::Rc::new(super::classes::Class::from_statement(
                class,
                self.module_name,
                self.is_package,
            ));
            let inherited = self
                .lineage(model, origin)
                .into_iter()
                .any(|(class, context)| {
                    class.metaclass.as_ref().is_some_and(|value| {
                        !matches!(
                            self.operand_in(value, context, &mut Default::default()),
                            Operand::Unknown
                        )
                    })
                });
            if !self.scopes.declared(name)
                && !self.modules.builtin(name)
                && !super::defaults::fixed().module_attribute(name)
                && !inherited
            {
                self.emit(
                    "E0602",
                    "undefined-variable",
                    &format!("Undefined variable '{name}'"),
                    self.declaration_range(class.name.range()),
                );
            }
        }
    }
    pub(super) fn check_metaclass(&mut self, statement: &StmtClassDef) {
        let Some(value) = statement
            .arguments
            .as_ref()
            .and_then(|arguments| {
                arguments.keywords.iter().find(|keyword| {
                    keyword
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "metaclass")
                })
            })
            .map(|keyword| &keyword.value)
        else {
            return;
        };
        let origin = Context::Scope(self.scopes.current_index());
        let Some(label) = self.invalid_metaclass_label(value, origin, true) else {
            return;
        };
        self.emit(
            "E1139",
            "invalid-metaclass",
            &format!(
                "Invalid metaclass {} used",
                super::string_format::python_string_repr(&label)
            ),
            self.declaration_range(statement.name.range()),
        );
    }
    fn invalid_metaclass_label(
        &self,
        expression: &Expr,
        origin: Context,
        constructor: bool,
    ) -> Option<String> {
        let mut nodes = self.operand_nodes(expression, origin.clone());
        let (expression, origin) = nodes.drain(..).next()?;
        if let Some(resolved) = self.expression_target_in(&expression, origin.clone()) {
            if let Target::Function(function) = resolved.target {
                if function.category != "lambda" && constructor {
                    if let Some(result) = function.result_expressions.first() {
                        return self.invalid_metaclass_label(result, resolved.context, false);
                    }
                    return (function.returns == Returns::None && !function.pass_body)
                        .then(|| "None".into());
                }
            }
        }
        if let Expr::Call(call) = &expression {
            if self.expression_target_in(&call.func, origin.clone()).is_some_and(|resolved| matches!(resolved.target, Target::Class(class) if class.identity.0 == "builtins" && class.name == "type")) {
                return (call.arguments.args.len() == 1).then(|| "Instance of builtins.type".into());
            }
        }
        match self.operand_in(&expression, origin.clone(), &mut Default::default()) {
            Operand::Class(class, context) => {
                let valid = self.mro(class.clone(), context).is_ok_and(|order| {
                    order
                        .iter()
                        .any(|(class, _)| class.identity.0 == "builtins" && class.name == "type")
                });
                (!valid).then(|| class.name.clone())
            }
            Operand::Instance(class, _) => {
                Some(format!("Instance of {}.{}", class.identity.0, class.name))
            }
            Operand::Unknown => None,
            _ => {
                let module = match &origin {
                    Context::Module(module) => module.as_str(),
                    Context::Scope(_) => self.module_name,
                };
                let source = self
                    .modules
                    .source(self.source.project, module)
                    .unwrap_or(&self.source.text);
                Some(super::display::expression(&expression, source))
            }
        }
    }
}
