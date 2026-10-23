//! Design diagnostics keep declaration timing and fixed original thresholds.
use super::{classes::MethodKind, defaults, function_metrics, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};

impl Checker<'_> {
    fn design_limit(
        &mut self,
        rule: &str,
        name: &str,
        label: &str,
        count: usize,
        limit: usize,
        range: TextRange,
    ) {
        if count > limit {
            self.emit(rule, name, &format!("{label} ({count}/{limit})"), range);
        }
    }
    pub(super) fn check_function_design_header(
        &mut self,
        function: &StmtFunctionDef,
        method: Option<MethodKind>,
    ) {
        let positional: Vec<_> = function
            .parameters
            .posonlyargs
            .iter()
            .chain(function.parameters.args.iter())
            .skip(usize::from(matches!(
                method,
                Some(MethodKind::Instance | MethodKind::Class | MethodKind::Property)
            )))
            .collect();
        let ignored = positional
            .iter()
            .copied()
            .chain(function.parameters.kwonlyargs.iter())
            .filter(|parameter| defaults::fixed().ignored(parameter.parameter.name.as_str(), true))
            .count();
        let count = positional.len() + function.parameters.kwonlyargs.len() - ignored;
        let pos = positional
            .iter()
            .filter(|parameter| !defaults::fixed().ignored(parameter.parameter.name.as_str(), true))
            .count();
        let limits = defaults::fixed().function_limits();
        let range = self.declaration_range(function.name.range());
        self.design_limit(
            "R0913",
            "too-many-arguments",
            "Too many arguments",
            count,
            limits.arguments,
            range,
        );
        self.design_limit(
            "R0917",
            "too-many-positional-arguments",
            "Too many positional arguments",
            pos,
            limits.positional,
            range,
        );
        let mut symbols = crate::infrastructure::project::symbols::Symbols::collect(
            &function.body,
            self.module_name,
            self.is_package,
        );
        symbols.parameters(&function.parameters);
        for (name, range) in super::scopes::patterns::collect(&function.body) {
            symbols.bindings.entry(name).or_insert(
                crate::infrastructure::project::symbols::Definition {
                    range,
                    kind: crate::infrastructure::project::symbols::DefinitionKind::Value,
                },
            );
        }
        for name in &symbols.globals {
            symbols.bindings.remove(name);
        }
        let locals = symbols
            .bindings
            .len()
            .saturating_sub(ignored + usize::from(symbols.bindings.contains_key("_")));
        self.design_limit(
            "R0914",
            "too-many-locals",
            "Too many local variables",
            locals,
            limits.locals,
            range,
        );
    }
    pub(super) fn check_function_design_end(&mut self, function: &StmtFunctionDef) {
        let Some(count) = self
            .function_metrics
            .functions
            .get(&function.start().to_u32())
            .copied()
        else {
            return;
        };
        let limits = defaults::fixed().function_limits();
        let range = self.declaration_range(function.name.range());
        self.design_limit(
            "R0911",
            "too-many-return-statements",
            "Too many return statements",
            count.returns,
            limits.returns,
            range,
        );
        self.design_limit(
            "R0912",
            "too-many-branches",
            "Too many branches",
            count.branches,
            limits.branches,
            range,
        );
        self.design_limit(
            "R0915",
            "too-many-statements",
            "Too many statements",
            count.statements,
            limits.statements,
            range,
        );
    }
    pub(super) fn check_boolean_design(&mut self, value: &Expr) {
        if matches!(value, Expr::BoolOp(_)) {
            self.design_limit(
                "R0916",
                "too-many-boolean-expressions",
                "Too many boolean expressions in if statement",
                function_metrics::boolean_count(value),
                defaults::fixed().function_limits().boolean,
                value.range(),
            );
        }
    }
}
