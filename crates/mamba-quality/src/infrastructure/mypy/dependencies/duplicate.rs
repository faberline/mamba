//! One source path cannot acquire a second module owner.
use crate::domain::model::{QualityDiagnostic, QualityLocation, QualityTool};
use std::path::PathBuf;

pub(super) fn diagnostic(path: PathBuf, previous: &str, selected: &str) -> QualityDiagnostic {
    QualityDiagnostic {
        tool: QualityTool::Mypy,
        rule: String::new(),
        message: format!(
            "Source file found twice under different module names: \"{previous}\" and \"{selected}\""
        ),
        hint: Some(
            "See https://mypy.readthedocs.io/en/stable/running_mypy.html#mapping-file-paths-to-modules for more info\nCommon resolutions include:\n    a) adding `__init__.py` somewhere,\n    b) using `--explicit-package-bases` or adjusting `MYPYPATH`".into(),
        ),
        path,
        location: QualityLocation::unpositioned(),
        severity: "error".into(),
        fix: None,
    }
}
