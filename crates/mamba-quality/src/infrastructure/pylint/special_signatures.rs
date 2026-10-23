//! Special-method arities use the pinned original method table.
use super::{
    classes::MethodKind,
    defaults::{self, SpecialParameters},
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::StmtFunctionDef;
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn check_special_signature(&mut self, function: &StmtFunctionDef, kind: MethodKind) {
        let Some(expected) = defaults::fixed().special_parameters(function.name.as_str()) else {
            return;
        };
        let parameters = &function.parameters;
        if parameters.args.is_empty() && parameters.vararg.is_none() {
            return;
        }
        let count = parameters
            .args
            .len()
            .saturating_sub(usize::from(kind != MethodKind::Static));
        let optional = parameters
            .posonlyargs
            .iter()
            .chain(&parameters.args)
            .filter(|value| value.default.is_some())
            .count();
        let mandatory = count as isize - optional as isize;
        let (emit, expected) = match expected {
            SpecialParameters::Exact(expected) => (
                mandatory > *expected as isize
                    || (count < *expected && parameters.vararg.is_none()),
                expected.to_string(),
            ),
            SpecialParameters::Range([min, max]) => (
                mandatory != *min as isize && mandatory != *max as isize,
                format!("between {min} or {max}"),
            ),
        };
        if emit {
            let verb = if count <= 1 { "was" } else { "were" };
            self.emit(
                "E0302",
                "unexpected-special-method-signature",
                &format!(
                    "The special method '{}' expects {expected} param(s), {count} {verb} given",
                    function.name
                ),
                self.declaration_range(function.name.range()),
            );
        }
    }
}
