use crate::domain::{
    model::{QualityDiagnostic, QualityTool},
    source::SourceSnapshot,
};
use mamba_language::application::quality_syntax::SyntaxDocument;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Clone)]
pub(super) struct TypeIgnores {
    lines: BTreeMap<u32, (Vec<String>, BTreeSet<String>)>,
    untyped_imports: BTreeSet<u32>,
}

pub(super) fn ignore_errors(document: &SyntaxDocument<'_>) -> bool {
    document
        .tokens
        .iter()
        .filter(|token| token.kind == "Comment")
        .any(|token| {
            let comment = document.text(token).trim_start_matches('#').trim();
            comment.strip_prefix("mypy:").is_some_and(|options| {
                options.split(',').any(|option| {
                    matches!(
                        option.trim(),
                        "ignore-errors" | "ignore-errors=True" | "ignore_errors=True"
                    )
                })
            })
        })
}

impl TypeIgnores {
    pub(super) fn from_document(document: &SyntaxDocument<'_>) -> Self {
        let mut ignores = Self::default();
        for token in document
            .tokens
            .iter()
            .filter(|token| token.kind == "Comment")
        {
            let comment = document.text(token).trim_start_matches('#').trim();
            let Some(suffix) = comment
                .strip_prefix("type:")
                .and_then(|text| text.trim_start().strip_prefix("ignore"))
            else {
                continue;
            };
            let suffix = suffix.trim_start();
            if !suffix.is_empty() && !suffix.starts_with(['[', '#']) {
                continue;
            }
            let codes = suffix
                .strip_prefix('[')
                .and_then(|text| text.split_once(']'))
                .map(|(codes, _)| {
                    codes
                        .split(',')
                        .map(|code| code.trim().to_owned())
                        .collect()
                })
                .unwrap_or_default();
            ignores.lines.insert(
                document.line_column(token.start).0,
                (codes, BTreeSet::new()),
            );
        }
        ignores
    }

    pub(super) fn suppresses(&mut self, line: u32, rule: &str) -> bool {
        if rule == "import-untyped" {
            self.untyped_imports.insert(line);
        }
        let Some((codes, used)) = self.lines.get_mut(&line) else {
            return false;
        };
        if codes.is_empty()
            || codes
                .iter()
                .any(|code| code == rule || (code == "misc" && rule == "prop-decorator"))
        {
            used.insert(rule.into());
            return true;
        }
        false
    }

    pub(super) fn hint(&self, line: u32, rule: &str) -> Option<String> {
        let (codes, _) = self.lines.get(&line)?;
        Some(format!(
            "Error code \"{rule}\" not covered by \"type: ignore[{}]\" comment",
            codes.join(", ")
        ))
    }

    pub(super) fn unused(&self, source: &SourceSnapshot) -> Vec<QualityDiagnostic> {
        // A named filter must refer to an implemented rule before it is audited.
        const IMPLEMENTED: &[&str] = &[
            "name-defined",
            "assignment",
            "var-annotated",
            "return-value",
            "no-untyped-def",
            "untyped-decorator",
            "call-arg",
            "arg-type",
            "misc",
            "pydantic-field",
            "pydantic-alias",
            "pydantic-orm",
            "prop-decorator",
            "attr-defined",
            "syntax",
            "import-not-found",
            "import-untyped",
        ];
        let mut diagnostics = Vec::new();
        for (&line, (codes, used)) in &self.lines {
            if codes
                .iter()
                .any(|code| !IMPLEMENTED.contains(&code.as_str()))
            {
                continue;
            }
            let unused: Vec<_> = codes.iter().filter(|code| !used.contains(*code)).collect();
            if (codes.is_empty() && !used.is_empty()) || (!codes.is_empty() && unused.is_empty()) {
                continue;
            }
            let mut message = if (codes.is_empty() || codes.len() == 1)
                && (self.untyped_imports.contains(&line)
                    || !codes
                        .iter()
                        .any(|code| matches!(code.as_str(), "syntax" | "import-not-found")))
            {
                "Unused \"type: ignore\" comment".into()
            } else {
                format!(
                    "Unused \"type: ignore[{}]\" comment",
                    unused
                        .into_iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            if codes.iter().any(|code| code == "misc") && used.contains("prop-decorator") {
                message.push_str(", use narrower [prop-decorator] instead of [misc] code");
            }
            diagnostics.push(QualityDiagnostic {
                tool: QualityTool::Mypy,
                rule: "unused-ignore".into(),
                message,
                hint: None,
                path: source.path.clone(),
                severity: "error".into(),
                fix: None,
                location: crate::domain::model::QualityLocation {
                    line,
                    column: 0,
                    end_line: Some(line),
                    end_column: Some(-1),
                    offset: None,
                    end_offset: None,
                },
            });
        }
        diagnostics
    }
}
