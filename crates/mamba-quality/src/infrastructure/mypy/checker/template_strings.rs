//! Template version errors do not stop checking their interpolated expressions.
use super::*;
use quality_syntax::python_ast::{ExprBinOp, Operator};
use ruff_text_size::{TextRange, TextSize};

impl Checker<'_> {
    pub(super) fn prepare_template_strings(
        &mut self,
        document: &quality_syntax::SyntaxDocument<'_>,
    ) {
        if self.python_target == quality_syntax::PythonTarget::Py314 {
            return;
        }
        let mut templates = Templates::default();
        for statement in &document.parsed.syntax().body {
            templates.visit_stmt(statement);
        }
        if templates.starts.is_empty() {
            return;
        }
        let point = TextRange::new(TextSize::from(0), TextSize::from(1));
        self.emit_with_hint(
            "import-not-found",
            "Cannot find implementation or library stub for module named \"string.templatelib\""
                .into(),
            &point,
            Some(
                "See https://mypy.readthedocs.io/en/stable/running_mypy.html#missing-imports"
                    .into(),
            ),
        );
        for start in templates.starts {
            let point = TextRange::new(start, start + TextSize::from(1));
            self.diagnostics.push(super::super::super::diagnostics::at(
                self.source,
                QualityTool::Mypy,
                "syntax",
                format!(
                    "T-strings: requires Python 3.14 or newer (current target: Python {})",
                    self.python_target.native()
                ),
                point,
            ));
        }
    }

    pub(super) fn visit_template_string<'a>(&mut self, expression: &'a Expr) -> bool
    where
        Self: Visitor<'a>,
    {
        if !matches!(expression, Expr::TString(_)) {
            return false;
        }
        self.template_depth += 1;
        walk_expr(self, expression);
        self.template_depth -= 1;
        true
    }

    pub(super) fn check_template_operator(&mut self, binary: &ExprBinOp) {
        if binary.op != Operator::Add {
            return;
        }
        let (Some(left), Some(right)) = (self.infer(&binary.left), self.infer(&binary.right))
        else {
            return;
        };
        let incompatible = self
            .context
            .template_addition(left, right)
            .is_some_and(|(_, incompatible)| incompatible);
        let incompatible_interpolation = self.template_depth > 0
            && matches!(
                left,
                types::Type::Int | types::Type::Float | types::Type::Bool
            )
            && right == types::Type::Str;
        if incompatible || incompatible_interpolation {
            self.emit(
                "operator",
                format!(
                    "Unsupported operand types for + (\"{}\" and \"{}\")",
                    self.context.name(left),
                    self.context.name(right)
                ),
                binary.right.as_ref(),
            );
        }
    }
}

#[derive(Default)]
struct Templates {
    starts: Vec<TextSize>,
}

impl<'a> Visitor<'a> for Templates {
    fn visit_expr(&mut self, expression: &'a Expr) {
        if matches!(expression, Expr::TString(_)) {
            self.starts.push(expression.start());
        }
        walk_expr(self, expression);
    }
}
