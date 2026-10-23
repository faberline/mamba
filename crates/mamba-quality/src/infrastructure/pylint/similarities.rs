//! Project duplicate ranges retain the fixed default exclusions and threshold.
use super::{directives, similarity_groups, similarity_lines::Lines};
use crate::{
    domain::{
        model::{QualityDiagnostic, QualityTool},
        source::QualityWorkspace,
    },
    infrastructure::project::ProjectIndex,
};

pub(super) fn check(
    workspace: &QualityWorkspace,
    index: &ProjectIndex<'_>,
) -> Vec<QualityDiagnostic> {
    let mut result = Vec::new();
    for project in 0..workspace.projects.len() {
        let modules: Vec<_> = index
            .modules
            .iter()
            .enumerate()
            .filter(|(id, module)| {
                module.project == project
                    && !workspace.sources[*id].is_stub
                    && module.document.as_ref().is_some_and(|document| {
                        !directives::instructions(document)
                            .iter()
                            .any(|entry| entry.skip)
                    })
            })
            .collect();
        if modules.len() < 2 {
            continue;
        }
        let lines: Vec<_> = modules
            .iter()
            .map(|(id, module)| {
                Lines::new(
                    &workspace.sources[*id].text,
                    module.document.as_ref().unwrap(),
                )
            })
            .collect();
        let source = &workspace.sources[modules.last().unwrap().0];
        for group in similarity_groups::groups(&lines) {
            let mut chunks = group.chunks;
            chunks.sort_by_key(|chunk| (&modules[chunk.file].1.name, chunk.start, chunk.end));
            let mut message = format!("Similar lines in {} files", chunks.len());
            for chunk in &chunks {
                message.push_str(&format!(
                    "\n=={}:[{}:{}]",
                    modules[chunk.file].1.name, chunk.start, chunk.end
                ));
            }
            // The original set iteration selects an unstable display fragment.
            // Use the final named module; exact message parity remains incomplete.
            let chunk = chunks.last().unwrap();
            for line in &lines[chunk.file].real[chunk.start..chunk.end] {
                message.push('\n');
                message.push_str(line.trim_end_matches(super::source_layout::whitespace));
            }
            result.push(super::super::diagnostics::at(
                source,
                QualityTool::Pylint,
                "R0801",
                message,
                super::super::diagnostics::point(0),
            ));
        }
    }
    result
}
