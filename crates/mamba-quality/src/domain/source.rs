use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PythonVersion {
    #[serde(rename = "3.12")]
    Py312,
    #[serde(rename = "3.13")]
    Py313,
    #[serde(rename = "3.14")]
    Py314,
}

impl PythonVersion {
    pub fn name(self) -> &'static str {
        match self {
            Self::Py312 => "3.12",
            Self::Py313 => "3.13",
            Self::Py314 => "3.14",
        }
    }

    pub fn minor(self) -> u8 {
        match self {
            Self::Py312 => 12,
            Self::Py313 => 13,
            Self::Py314 => 14,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencySource {
    pub kind: String,
    pub path: Option<PathBuf>,
    pub identity: String,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub root: PathBuf,
    pub python_version: PythonVersion,
    pub requires_python: Option<String>,
    pub metadata_sha256: Option<String>,
    pub dependencies: Vec<DependencySource>,
}

#[derive(Debug, Clone)]
pub struct SourceSnapshot {
    pub path: PathBuf,
    pub project: usize,
    pub bytes: Vec<u8>,
    pub text: String,
    pub sha256: String,
    pub encoding: SourceEncoding,
    pub decoding_issue: Option<super::model::QualityIssue>,
    pub is_stub: bool,
}

#[derive(Debug, Clone)]
pub enum SourceEncoding {
    Utf8 { bom: bool },
    Latin1,
    Ascii,
    Named(&'static str),
}

#[derive(Debug, Default, Clone)]
pub struct QualityWorkspace {
    pub projects: Vec<ProjectInfo>,
    pub explicit_files: Vec<PathBuf>,
    pub scan_roots: Vec<PathBuf>,
    pub source_requests: Vec<SourceRequest>,
    pub discovered: usize,
    pub sources: Vec<SourceSnapshot>,
    pub issues: Vec<super::model::QualityIssue>,
}

/// Input spelling and order are separate from shared canonical source storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRequest {
    pub path: PathBuf,
    pub canonical_path: PathBuf,
    pub directory: bool,
    pub sources: Vec<RequestedSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestedSource {
    pub path: PathBuf,
    pub source_path: PathBuf,
    pub package_files: Vec<PathBuf>,
}
