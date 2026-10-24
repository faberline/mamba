use crate::{
    application::quality::{QualityReport, QualityRequest},
    infrastructure::{cache::CachedEngine, filesystem::Filesystem, native::NativeEngine},
};

#[cfg(test)]
mod tests;

pub(crate) fn run(request: &QualityRequest) -> QualityReport {
    if !request.operation.writes() {
        crate::application::service::execute(
            request,
            &Filesystem,
            &NativeEngine::for_request(request.operation),
        )
    } else {
        crate::application::service::execute(request, &Filesystem, &CachedEngine::new(NativeEngine))
    }
}
