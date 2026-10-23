/// Source provenance licensing a type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeProvenance {
    /// Authored concrete annotation or unresolved type (not explicit Any).
    Explicit,
    /// Explicitly authored `Any` or `typing.Any`.
    ExplicitAny,
    /// Omitted annotation with successful inference.
    Inferred { inference_path: String },
    /// Omitted annotation with failed inference.
    ImplicitUnknown { inference_path: String },
}

/// Closed value object representing a dynamic boundary license granted by explicit `Any`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DynamicBoundaryLicense {
    ExplicitAny,
}

impl DynamicBoundaryLicense {
    /// Derive a boundary license from stored type provenance.
    pub fn from_provenance(provenance: &TypeProvenance) -> Option<Self> {
        match provenance {
            TypeProvenance::ExplicitAny => Some(DynamicBoundaryLicense::ExplicitAny),
            TypeProvenance::Explicit
            | TypeProvenance::Inferred { .. }
            | TypeProvenance::ImplicitUnknown { .. } => None,
        }
    }
}
