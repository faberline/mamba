//! Public request and report API. The CLI does not own analysis policy.
pub use crate::domain::model::*;
pub use crate::domain::profile::*;
pub use crate::domain::source::{DependencySource, ProjectInfo, PythonVersion};

pub fn run(request: &QualityRequest) -> QualityReport {
    crate::app::run(request)
}
