//! Missing import reasons preserve approved stub metadata and source priority.
use super::Lookup;
use std::{collections::BTreeMap, sync::OnceLock};

const MISSING_IMPORT_NOTE: &str =
    "See https://mypy.readthedocs.io/en/stable/running_mypy.html#missing-imports";
const INSTALL_TYPES_NOTE: &str =
    "(or run \"mypy --install-types\" to install all missing stub packages)";

#[derive(Clone, Copy)]
pub(in crate::infrastructure::mypy) enum MissingImport {
    NotFound,
    ApprovedStub(&'static str),
}

#[derive(serde::Deserialize)]
struct StubDistributions {
    flat: BTreeMap<String, String>,
    namespace: BTreeMap<String, BTreeMap<String, String>>,
}

fn distributions() -> &'static StubDistributions {
    static DATA: OnceLock<StubDistributions> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../assets/stdlib/approved-stub-distributions.json"
        ))
        .expect("locked approved stub distributions")
    })
}

fn stub_distribution_name(module: &str) -> Option<&'static str> {
    let top_level = module.split('.').next()?;
    let data = distributions();
    if let Some(distribution) = data.flat.get(top_level) {
        return Some(distribution);
    }
    let namespace = data.namespace.get(top_level)?;
    let mut prefix = module;
    loop {
        if let Some(distribution) = namespace.get(prefix) {
            return Some(distribution);
        }
        prefix = prefix.rsplit_once('.')?.0;
    }
}

pub(in crate::infrastructure::mypy) fn approved_stub_notes(distribution: &str) -> String {
    format!(
        "Hint: \"python3 -m pip install {distribution}\"\n{INSTALL_TYPES_NOTE}\n{MISSING_IMPORT_NOTE}"
    )
}

pub(in crate::infrastructure::mypy) fn only_once_import_note(note: &str) -> bool {
    if matches!(note, MISSING_IMPORT_NOTE | INSTALL_TYPES_NOTE) {
        return true;
    }
    let Some(distribution) = note
        .strip_prefix("Hint: \"python3 -m pip install ")
        .and_then(|note| note.strip_suffix('"'))
    else {
        return false;
    };
    let data = distributions();
    data.flat
        .values()
        .chain(data.namespace.values().flat_map(BTreeMap::values))
        .any(|known| known.as_str() == distribution)
}

impl Lookup {
    pub(super) fn approved_stub_issue(&self, module: &str) -> Option<MissingImport> {
        if self.module_exists(module) || self.semantic_external_module_exists(module) {
            return None;
        }
        let distribution = stub_distribution_name(module)?;
        for (offset, _) in module.match_indices('.') {
            let parent = &module[..offset];
            if stub_distribution_name(parent) == Some(distribution) {
                return Some(match self.approved_stub_issue(parent) {
                    Some(MissingImport::ApprovedStub(found)) if found == distribution => {
                        MissingImport::ApprovedStub(distribution)
                    }
                    _ => MissingImport::NotFound,
                });
            }
        }
        Some(MissingImport::ApprovedStub(distribution))
    }

    pub(super) fn untyped_import_issue(&self, module: &str) -> Option<String> {
        if self.module_exists(module)
            || self.semantic_external_module_exists(module)
            || self.approved_stub_issue(module).is_some()
        {
            return None;
        }
        let project = self.modules.info[self.current].0;
        self.modules
            .untyped
            .contains(&(project, module.into()))
            .then(|| {
                format!(
                    "Skipping analyzing \"{module}\": module is installed, but missing library stubs or py.typed marker"
                )
            })
    }
}
