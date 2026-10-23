use super::profile::FixedProfile;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
mod position;
mod rule;
#[cfg(test)]
mod tests;

pub const REPORT_SCHEMA: &str = "mamba.quality.v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum QualityOperation {
    Check,
    Fix { diff: bool },
    Format { check: bool, diff: bool },
    Info,
}

impl QualityOperation {
    pub fn writes(self) -> bool {
        matches!(
            self,
            Self::Fix { diff: false }
                | Self::Format {
                    check: false,
                    diff: false
                }
        )
    }

    pub fn checks(self) -> bool {
        matches!(self, Self::Check | Self::Fix { .. })
    }
}

#[derive(Debug, Clone)]
pub struct QualityRequest {
    pub operation: QualityOperation,
    pub paths: Vec<PathBuf>,
}

impl QualityRequest {
    pub fn new(operation: QualityOperation, paths: Vec<PathBuf>) -> Self {
        Self { operation, paths }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QualityTool {
    Ruff,
    Isort,
    Mypy,
    Pylint,
    Black,
}

impl QualityTool {
    pub const ALL: [Self; 5] = [
        Self::Ruff,
        Self::Isort,
        Self::Mypy,
        Self::Pylint,
        Self::Black,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Ruff => "ruff",
            Self::Isort => "isort",
            Self::Mypy => "mypy",
            Self::Pylint => "pylint",
            Self::Black => "black",
        }
    }

    pub fn reference_version(self) -> &'static str {
        match self {
            Self::Ruff => "0.16.10",
            Self::Isort => "9.0.2",
            Self::Mypy => "2.4.0",
            Self::Pylint => "4.1.2",
            Self::Black => "26.10.0",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionStatus {
    Complete,
    Incomplete,
    Unsupported,
    Error,
    NotRequested,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityLocation {
    #[serde(
        serialize_with = "position::serialize",
        deserialize_with = "position::deserialize"
    )]
    pub line: u32,
    pub column: u32,
    #[serde(
        serialize_with = "position::serialize_optional",
        deserialize_with = "position::deserialize_optional"
    )]
    pub end_line: Option<u32>,
    pub end_column: Option<i64>,
    pub offset: Option<u32>,
    pub end_offset: Option<u32>,
}

impl QualityLocation {
    /// The pinned original uses -1 when a diagnostic has no source position.
    pub const UNPOSITIONED_LINE: u32 = u32::MAX;

    pub fn unpositioned() -> Self {
        Self {
            line: Self::UNPOSITIONED_LINE,
            column: 0,
            end_line: Some(Self::UNPOSITIONED_LINE),
            end_column: Some(0),
            offset: None,
            end_offset: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityEdit {
    pub start: u32,
    pub end: u32,
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityFix {
    pub applicability: String,
    pub message: Option<String>,
    pub edits: Vec<QualityEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityDiagnostic {
    pub tool: QualityTool,
    #[serde(
        serialize_with = "rule::serialize",
        deserialize_with = "rule::deserialize"
    )]
    pub rule: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    pub path: PathBuf,
    pub location: QualityLocation,
    pub severity: String,
    pub fix: Option<QualityFix>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolReport {
    pub tool: QualityTool,
    pub reference_version: String,
    pub completion: CompletionStatus,
    pub reference_exit_code: Option<i32>,
    pub checked_files: usize,
    pub diagnostics: Vec<QualityDiagnostic>,
    pub remaining: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityIssue {
    pub code: String,
    pub message: String,
    pub path: Option<PathBuf>,
}

impl QualityIssue {
    pub fn new(code: &str, message: impl Into<String>, path: Option<PathBuf>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            path,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: PathBuf,
    pub original_sha256: String,
    pub candidate_sha256: String,
    pub applied: bool,
    pub diff: Option<String>,
    pub tools: Vec<QualityTool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileCounts {
    pub discovered: usize,
    pub checked: usize,
    pub changed: usize,
    pub written: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReport {
    pub schema: String,
    pub operation: QualityOperation,
    pub profile: FixedProfile,
    pub projects: Vec<super::source::ProjectInfo>,
    pub tools: Vec<ToolReport>,
    pub issues: Vec<QualityIssue>,
    pub changes: Vec<FileChange>,
    pub files: FileCounts,
    pub analysis_completed: bool,
    pub passed: bool,
    pub exit_code: i32,
}

impl QualityReport {
    pub fn finish(&mut self) {
        self.analysis_completed = self.issues.is_empty()
            && self.tools.iter().all(|tool| {
                matches!(
                    tool.completion,
                    CompletionStatus::Complete | CompletionStatus::NotRequested
                )
            });
        let clean = self.tools.iter().all(|tool| {
            tool.diagnostics
                .iter()
                .all(|diagnostic| matches!(diagnostic.severity.as_str(), "info" | "note"))
        });
        let needs_format = matches!(self.operation, QualityOperation::Format { check: true, .. })
            && !self.changes.is_empty();
        self.passed = self.analysis_completed && clean && !needs_format;
        self.exit_code = if !self.analysis_completed {
            2
        } else if self.passed {
            0
        } else {
            1
        };
    }
}

#[derive(Debug)]
pub struct TransformResult {
    pub completion: CompletionStatus,
    pub candidates: Vec<(usize, String)>,
    pub issues: Vec<QualityIssue>,
    pub remaining: Vec<String>,
}
