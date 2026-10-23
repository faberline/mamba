use crate::{
    application::quality::{QualityReport, QualityRequest},
    domain::model::QualityOperation,
    infrastructure::{cache::CachedEngine, filesystem::Filesystem, native::NativeEngine},
};

pub(crate) fn run(request: &QualityRequest) -> QualityReport {
    if request.operation == QualityOperation::Check {
        crate::application::service::execute(
            request,
            &Filesystem,
            &NativeEngine::for_request(request.operation),
        )
    } else {
        crate::application::service::execute(request, &Filesystem, &CachedEngine::new(NativeEngine))
    }
}
