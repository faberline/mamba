//! Versioned, lossless Python syntax for static project tools.
//!
//! This service does not change the compiler's lexer, AST, or parsing entry points.
//! Byte ranges refer to the original UTF-8 source. Line and column are one-based.
pub use ruff_python_ast as python_ast;
use ruff_python_ast::{ModModule, PySourceType, PythonVersion};
use ruff_python_parser::ParseOptions;
pub use ruff_python_parser::Parsed;
pub use ruff_python_parser::UnsupportedSyntaxErrorKind;
use std::sync::Arc;
mod token_projection;
mod version;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonTarget {
    Py312,
    Py313,
    Py314,
}

impl PythonTarget {
    pub fn native(self) -> PythonVersion {
        match self {
            Self::Py312 => PythonVersion::PY312,
            Self::Py313 => PythonVersion::PY313,
            Self::Py314 => PythonVersion::PY314,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxToken {
    pub kind: String,
    pub start: u32,
    pub end: u32,
    pub trivia: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxIssue {
    pub message: String,
    pub start: u32,
    pub end: u32,
    pub version_error: bool,
}

#[derive(Debug)]
pub struct SyntaxDocumentData {
    pub target: PythonTarget,
    pub parsed: Parsed<ModModule>,
    /// Full rich tokens from parse/parse_data; comments only from type inference.
    /// The complete native token stream always remains in parsed.
    pub tokens: Vec<SyntaxToken>,
    pub issues: Vec<SyntaxIssue>,
}

#[derive(Debug)]
pub struct SyntaxDocument<'a> {
    pub source: &'a str,
    data: Arc<SyntaxDocumentData>,
}

impl<'a> SyntaxDocument<'a> {
    pub fn from_data(source: &'a str, data: Arc<SyntaxDocumentData>) -> Self {
        Self { source, data }
    }
}

impl std::ops::Deref for SyntaxDocument<'_> {
    type Target = SyntaxDocumentData;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl SyntaxDocument<'_> {
    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn has_only_type_parameter_default_errors(&self) -> bool {
        self.target == PythonTarget::Py312
            && !self.issues.is_empty()
            && self.issues.iter().all(|issue| issue.version_error)
            && self.parsed.errors().is_empty()
            && self.parsed.unsupported_syntax_errors().iter().all(|error| {
                error.kind == ruff_python_parser::UnsupportedSyntaxErrorKind::TypeParamDefault
            })
    }

    pub fn has_only_type_inference_version_errors(&self) -> bool {
        !self.issues.is_empty()
            && self.issues.iter().all(|issue| issue.version_error)
            && self.parsed.errors().is_empty()
            && self.parsed.unsupported_syntax_errors().iter().all(|error| {
                matches!(
                    error.kind,
                    ruff_python_parser::UnsupportedSyntaxErrorKind::TypeParamDefault
                        | ruff_python_parser::UnsupportedSyntaxErrorKind::TemplateStrings
                )
            })
    }

    pub fn text(&self, token: &SyntaxToken) -> &str {
        &self.source[token.start as usize..token.end as usize]
    }

    pub fn line_column(&self, offset: u32) -> (u32, u32) {
        let offset = (offset as usize).min(self.source.len());
        let position = ruff_source_file::LineIndex::from_source_text(self.source).line_column(
            ruff_text_size::TextSize::from(self.source.floor_char_boundary(offset) as u32),
            self.source,
        );
        (position.line.get() as u32, position.column.get() as u32)
    }
}

pub fn parse(
    source: &str,
    target: PythonTarget,
    is_stub: bool,
) -> Result<SyntaxDocument<'_>, String> {
    Ok(SyntaxDocument::from_data(
        source,
        Arc::new(parse_data(source, target, is_stub)?),
    ))
}

pub fn parse_data(
    source: &str,
    target: PythonTarget,
    is_stub: bool,
) -> Result<SyntaxDocumentData, String> {
    parse_data_with_projection(source, target, is_stub, token_projection::Projection::Full)
}

/// Parse complete syntax with only comments in the derived rich token view.
///
/// The AST, native tokens, issues, ranges, and version checks remain complete.
/// Use parse/parse_data when the rich view must reconstruct the full source.
/// This entry point does not filter source or dependencies.
pub fn parse_data_for_type_inference(
    source: &str,
    target: PythonTarget,
    is_stub: bool,
) -> Result<SyntaxDocumentData, String> {
    parse_data_with_projection(
        source,
        target,
        is_stub,
        token_projection::Projection::Comments,
    )
}

fn parse_data_with_projection(
    source: &str,
    target: PythonTarget,
    is_stub: bool,
    projection: token_projection::Projection,
) -> Result<SyntaxDocumentData, String> {
    if source.len() > u32::MAX as usize {
        return Err("Source exceeds the byte-offset range".into());
    }
    let source_type = if is_stub {
        PySourceType::Stub
    } else {
        PySourceType::Python
    };
    let parsed = ruff_python_parser::parse_unchecked(
        source,
        ParseOptions::from(source_type).with_target_version(target.native()),
    )
    .try_into_module()
    .expect("module parse mode returns a module");
    let mut issues: Vec<_> = parsed
        .errors()
        .iter()
        .map(|error| SyntaxIssue {
            message: error.error.to_string(),
            start: error.location.start().to_u32(),
            end: error.location.end().to_u32(),
            version_error: false,
        })
        .collect();
    issues.extend(
        parsed
            .unsupported_syntax_errors()
            .iter()
            .map(|error| SyntaxIssue {
                message: error.to_string(),
                start: error.range.start().to_u32(),
                end: error.range.end().to_u32(),
                version_error: true,
            }),
    );
    version::extend(&parsed, target, &mut issues);
    let tokens = token_projection::project(source, parsed.tokens(), projection);
    Ok(SyntaxDocumentData {
        target,
        parsed,
        tokens,
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_comments_crlf_and_unicode_bytes() {
        let source = "# comment\r\n\t# noqa: F401\r\nlabel = '水'  # end\r\n";
        let document = parse(source, PythonTarget::Py312, false).unwrap();
        assert!(document.is_valid(), "{:?}", document.issues);
        assert_eq!(
            document
                .tokens
                .iter()
                .map(|token| document.text(token))
                .collect::<String>(),
            source
        );
        let offset = source.find("水").unwrap() as u32;
        assert_eq!(document.line_column(offset), (3, 10));
    }

    #[test]
    fn template_strings_are_versioned_and_lossless() {
        let source = "name = 'world'\nmessage = t\"hello {name!r}\"\n";
        let document = parse(source, PythonTarget::Py314, false).unwrap();
        assert!(document.is_valid(), "{:?}", document.issues);
        assert!(document
            .tokens
            .iter()
            .any(|token| token.kind == "TStringStart"));
        assert_eq!(
            document
                .tokens
                .iter()
                .map(|token| document.text(token))
                .collect::<String>(),
            source
        );
        assert!(parse(source, PythonTarget::Py313, false)
            .unwrap()
            .issues
            .iter()
            .any(|issue| issue.version_error));
    }

    #[test]
    fn type_parameter_defaults_require_python313() {
        let source = "type Items[T = int] = list[T]\n";
        assert!(parse(source, PythonTarget::Py313, false)
            .unwrap()
            .is_valid());
        let document = parse(source, PythonTarget::Py312, false).unwrap();
        assert!(!document.is_valid());
        assert!(document.has_only_type_parameter_default_errors());
        for invalid in [
            "class Items[T = int]:\n    value = (\n",
            "class Items[T = int]:\n    value = t\"text\"\n",
        ] {
            assert!(!parse(invalid, PythonTarget::Py312, false)
                .unwrap()
                .has_only_type_parameter_default_errors());
        }
    }

    #[test]
    fn variadic_defaults_are_versioned_with_exact_ranges() {
        for param in ["**P = [int, str]", "*Ts = *tuple[int, str]"] {
            for source in [
                format!("type Items[{param}] = tuple[int, str]\n"),
                format!("def run[{param}](): ...\n"),
                format!("class Items[{param}]: ...\n"),
            ] {
                let document = parse(&source, PythonTarget::Py312, false).unwrap();
                assert_eq!(document.issues.len(), 1, "{:?}", document.issues);
                let issue = &document.issues[0];
                assert!(issue.version_error);
                assert_eq!(issue.start as usize, source.find('=').unwrap());
                assert!(source[issue.start as usize..issue.end as usize].starts_with("= "));
                for target in [PythonTarget::Py313, PythonTarget::Py314] {
                    assert!(parse(&source, target, false).unwrap().is_valid());
                }
            }
        }
        assert!(
            parse("type Items[**P, *Ts] = int\n", PythonTarget::Py312, false)
                .unwrap()
                .is_valid()
        );
    }
}
