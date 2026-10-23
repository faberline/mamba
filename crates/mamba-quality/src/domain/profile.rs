use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PROFILE_JSON: &str = include_str!("../../assets/profile.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixedProfile {
    pub schema: String,
    pub versions: BTreeMap<String, String>,
    pub python_versions: Vec<String>,
    pub default_python: String,
    pub line_width: u32,
    pub ruff: RuffProfile,
    pub isort: serde_json::Value,
    pub mypy: MypyProfile,
    pub pylint: PylintProfile,
    pub black: serde_json::Value,
    pub pydantic: serde_json::Value,
    pub standard_library: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuffProfile {
    pub select: Vec<String>,
    pub preview: bool,
    pub safe_fixes_only: bool,
    pub rules: Vec<RuffRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuffRule {
    pub code: String,
    pub name: String,
    pub message_formats: Vec<String>,
    pub fix_availability: String,
    pub preview: bool,
    pub status: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MypyProfile {
    pub strict_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PylintProfile {
    pub rules: Vec<PylintRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PylintRule {
    pub code: String,
    pub name: String,
    pub message: String,
    pub minimum_python: Option<Vec<u32>>,
    pub maximum_python: Option<Vec<u32>>,
}

impl FixedProfile {
    pub fn locked() -> Self {
        serde_json::from_str(PROFILE_JSON).expect("the embedded quality profile is valid JSON")
    }
}
