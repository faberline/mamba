//! Version checks absent from the pinned parser's variadic type-parameter branches.
use super::{PythonTarget, SyntaxIssue};
use ruff_python_ast::{
    token::TokenKind,
    visitor::{self, Visitor},
    ModModule, TypeParam,
};
use ruff_python_parser::{Parsed, UnsupportedSyntaxError, UnsupportedSyntaxErrorKind};
use ruff_text_size::{Ranged, TextRange};

pub(super) fn extend(
    parsed: &Parsed<ModModule>,
    target: PythonTarget,
    issues: &mut Vec<SyntaxIssue>,
) {
    if target != PythonTarget::Py312 {
        return;
    }
    struct Check<'a> {
        parsed: &'a Parsed<ModModule>,
        target: PythonTarget,
        issues: &'a mut Vec<SyntaxIssue>,
    }
    impl<'ast> Visitor<'ast> for Check<'_> {
        fn visit_type_param(&mut self, param: &'ast TypeParam) {
            let default = match param {
                TypeParam::ParamSpec(param) => param.default.as_deref(),
                TypeParam::TypeVarTuple(param) => param.default.as_deref(),
                TypeParam::TypeVar(_) => None,
            };
            if let Some(default) = default {
                let start = self
                    .parsed
                    .tokens()
                    .in_range(TextRange::new(param.start(), default.start()))
                    .iter()
                    .rev()
                    .find(|token| token.kind() == TokenKind::Equal)
                    .map_or(default.start(), Ranged::start);
                let error = UnsupportedSyntaxError {
                    kind: UnsupportedSyntaxErrorKind::TypeParamDefault,
                    range: TextRange::new(start, default.end()),
                    target_version: self.target.native(),
                };
                self.issues.push(SyntaxIssue {
                    message: error.to_string(),
                    start: error.range.start().to_u32(),
                    end: error.range.end().to_u32(),
                    version_error: true,
                });
            }
            visitor::walk_type_param(self, param);
        }
    }
    Check {
        parsed,
        target,
        issues,
    }
    .visit_body(&parsed.syntax().body);
}
