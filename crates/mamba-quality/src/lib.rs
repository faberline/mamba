//! Fixed-profile, native Python quality analysis. No Python runtime is required.
mod app;
pub mod application;
mod domain;
mod infrastructure;

pub use application::quality::{run, QualityOperation, QualityReport, QualityRequest};
