use super::{
    model::QualityIssue,
    source::{QualityWorkspace, SourceSnapshot},
};
use super::{
    model::{QualityTool, ToolReport, TransformResult},
    profile::FixedProfile,
};
use std::path::PathBuf;

#[derive(Debug)]
pub struct PublishFailure {
    pub(crate) issue: QualityIssue,
    pub(crate) applied: bool,
}

impl From<QualityIssue> for PublishFailure {
    fn from(issue: QualityIssue) -> Self {
        Self {
            issue,
            applied: false,
        }
    }
}

pub trait ProjectStore {
    fn load(&self, paths: &[PathBuf], scan: bool) -> QualityWorkspace;
    fn candidate_bytes(&self, source: &SourceSnapshot, text: &str)
        -> Result<Vec<u8>, QualityIssue>;
    fn publish(&self, source: &SourceSnapshot, candidate: &[u8]) -> Result<(), PublishFailure>;
}

pub trait QualityEngine {
    fn remaining(&self, _tool: QualityTool) -> Vec<String> {
        Vec::new()
    }
    fn check(
        &self,
        workspace: &QualityWorkspace,
        profile: &FixedProfile,
        tool: QualityTool,
    ) -> ToolReport;
    fn transform(
        &self,
        workspace: &QualityWorkspace,
        profile: &FixedProfile,
        tool: QualityTool,
    ) -> TransformResult;
    fn validate(
        &self,
        original: &SourceSnapshot,
        candidate: &str,
        workspace: &QualityWorkspace,
        structure_equivalent: bool,
    ) -> Result<(), QualityIssue>;
}
