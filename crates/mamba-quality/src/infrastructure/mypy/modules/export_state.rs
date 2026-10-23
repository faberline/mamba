//! Source export gaps follow actual module reads, rather than every loaded asset.
use super::{ExportState, Lookup, ModuleId};
use crate::domain::{model::QualityTool, source::SourceSnapshot};
use std::collections::BTreeSet;

pub(super) fn messages(source: &SourceSnapshot, state: Option<&ExportState>) -> BTreeSet<String> {
    let Some(state) = state else {
        return BTreeSet::from([format!(
            "{}: source export state is unsupported: module declarations and source range are unavailable",
            source.path.display(),
        )]);
    };
    state
        .gaps
        .iter()
        .map(|(range, reason)| {
            let location = crate::infrastructure::diagnostics::at(
                source,
                QualityTool::Mypy,
                "attr-defined",
                String::new(),
                *range,
            )
            .location;
            format!(
                "{}:{}:{} [bytes {}..{}]: source export state is unsupported: {}",
                source.path.display(),
                location.line,
                location.column,
                u32::from(range.start()),
                u32::from(range.end()),
                reason,
            )
        })
        .collect()
}

impl Lookup {
    pub(super) fn record_export_use(&self, id: ModuleId) {
        self.modules.export_uses.borrow_mut()[self.current].insert(id);
    }

    pub(in crate::infrastructure::mypy) fn source_has_all(&self, module: &str) -> Option<bool> {
        let id = self.module_id(module)?;
        self.record_export_use(id);
        self.modules.export_states.borrow()[id]
            .as_ref()
            .filter(|state| state.gaps.is_empty())
            .map(|state| state.has_all)
    }

    pub(in crate::infrastructure::mypy) fn source_public_names(
        &self,
        module: &str,
    ) -> Result<Option<(bool, BTreeSet<String>)>, String> {
        let Some(id) = self.module_id(module) else {
            if !self.module_exists(module)
                && !self.external_module_exists(module)
                && !self.standard_module_exists(module)
                && !self.typed_child_module_exists(module)
            {
                // Mypy's missing-module branch does not copy all_exports.
                // Existing import diagnostics remain responsible for the error.
                return Ok(None);
            }
            return Err(format!(
                "source public names for \"{module}\" are unavailable"
            ));
        };
        self.record_export_use(id);
        let states = self.modules.export_states.borrow();
        let state = states[id]
            .as_ref()
            .ok_or_else(|| format!("source export state for \"{module}\" is unavailable"))?;
        if !state.gaps.is_empty() {
            return Err(format!(
                "source export state for \"{module}\" is incomplete"
            ));
        }
        let exports = self.modules.exports.borrow();
        let exports = &exports[id];
        if let Some(name) = state.globals.iter().find(|name| {
            (!state.has_all || state.names.contains(*name)) && !exports.contains_key(*name)
        }) {
            return Err(format!(
                "global \"{name}\" in \"{module}\" has no native export binding"
            ));
        }
        Ok(Some((
            state.has_all,
            exports
                .iter()
                .filter(|(_, export)| export.explicit)
                .map(|(name, _)| name.clone())
                .collect(),
        )))
    }

    pub(in crate::infrastructure::mypy) fn export_state_gaps(&self) -> Vec<String> {
        let mut pending = vec![self.current];
        let mut visited = BTreeSet::new();
        let mut messages = BTreeSet::new();
        let uses = self.modules.export_uses.borrow();
        let gaps = self.modules.export_gaps.borrow();
        while let Some(id) = pending.pop() {
            if visited.insert(id) {
                messages.extend(gaps[id].iter().cloned());
                let (project, module, _) = &self.modules.info[id];
                messages.extend(
                    self.modules
                        .context
                        .returned_callable_gaps(&format!("{project}:{module}")),
                );
                pending.extend(uses[id].iter().copied());
            }
        }
        messages.into_iter().collect()
    }
}
