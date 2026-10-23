//! Comprehension walrus declarations retain owner and execution state.
use super::*;
use mamba_language::application::quality_syntax::python_ast::visitor::{walk_expr, Visitor};

#[derive(Default)]
struct FilterWalruses(BTreeMap<String, TextSize>);

impl<'a> Visitor<'a> for FilterWalruses {
    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Named(named) => {
                if let Expr::Name(name) = named.target.as_ref() {
                    self.0.entry(name.id.to_string()).or_insert(name.start());
                }
                self.visit_expr(&named.value);
            }
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    for parameter in parameters.iter_non_variadic_params() {
                        if let Some(default) = parameter.default.as_deref() {
                            self.visit_expr(default);
                        }
                    }
                }
            }
            Expr::Generator(_) | Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) => {}
            _ => walk_expr(self, expression),
        }
    }
}

impl Scopes {
    pub(super) fn declare_filter_walruses(
        &mut self,
        generators: &[mamba_language::application::quality_syntax::python_ast::Comprehension],
    ) {
        let mut targets = FilterWalruses::default();
        for generator in generators {
            for condition in &generator.ifs {
                targets.visit_expr(condition);
            }
        }
        let owner = self
            .current()
            .comprehension_owner
            .expect("comprehension scope");
        for (name, first_definition) in targets.0 {
            let index = self.stack[owner]
                .redirects
                .get(&name)
                .copied()
                .unwrap_or(owner);
            let assigned = self.stack[index]
                .bindings
                .get(&name)
                .is_some_and(|binding| {
                    // Initial binding availability is independent of type inference.
                    binding.first_definition <= first_definition
                });
            // A checked function does not create missing globals/nonlocals outside its owner.
            if index == owner {
                self.stack[index]
                    .bindings
                    .entry(name.clone())
                    .or_insert(Binding {
                        ty: None,
                        first_definition,
                        infer_from_assignment: true,
                        callable: None,
                        module: None,
                        explicit: true,
                        module_hidden: Some(false),
                        annotation_form: None,
                        annotation_module: None,
                    });
            }
            let scope = self.stack.last_mut().unwrap();
            scope.redirects.insert(name.clone(), index);
            scope.walrus_assigned.insert(name, assigned);
        }
    }
}
