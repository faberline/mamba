//! Original JSON reports attach notes to the latest error at the same position.
use super::*;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn compact_loop_pass_diagnostics(
        &mut self,
        passes: &[std::ops::Range<usize>],
    ) {
        // A provisional container error is a live storage slot. Its lifetime is
        // owned by finish_partial_sets, including its existing read commitment.
        let protected: BTreeSet<_> = self
            .partial_sets
            .values()
            .map(|partial| partial.diagnostic)
            .chain(
                self.partial_defaultdicts
                    .values()
                    .map(|partial| partial.diagnostic),
            )
            .collect();
        let mut seen = BTreeSet::new();
        let mut removed = BTreeSet::new();
        for pass in passes {
            let mut occurrences: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
            for index in pass.clone() {
                if protected.contains(&index) {
                    continue;
                }
                // Keep every field, including hint and exact source bounds.
                // The ordinal preserves multiplicity within one pass. No notes
                // are split, reparented, parsed or unioned by this collector.
                let record = serde_json::to_vec(&self.diagnostics[index])
                    .expect("serialize a native quality diagnostic");
                let occurrence = occurrences.entry(record.clone()).or_default();
                if !seen.insert((record, *occurrence)) {
                    removed.insert(index);
                }
                *occurrence += 1;
            }
        }
        self.remove_loop_diagnostics(&removed);
    }

    pub(in crate::infrastructure::mypy::checker) fn remap_loop_diagnostic_ranges(
        &mut self,
        removed: &BTreeSet<usize>,
    ) {
        for collector in &mut self.iteration_diagnostics {
            collector.remap_diagnostic_ranges(removed);
        }
    }

    pub(in crate::infrastructure::mypy::checker) fn remove_loop_diagnostics(
        &mut self,
        removed: &BTreeSet<usize>,
    ) {
        if removed.is_empty() {
            return;
        }
        self.diagnostics = std::mem::take(&mut self.diagnostics)
            .into_iter()
            .enumerate()
            .filter_map(|(index, diagnostic)| (!removed.contains(&index)).then_some(diagnostic))
            .collect();
        for partial in self.partial_sets.values_mut() {
            partial.diagnostic -= removed.range(..partial.diagnostic).count();
        }
        for partial in self.partial_defaultdicts.values_mut() {
            partial.diagnostic -= removed.range(..partial.diagnostic).count();
        }
        for range in &mut self.settled_diagnostics {
            range.start -= removed.range(..range.start).count();
            range.end -= removed.range(..range.end).count();
        }
        self.remap_loop_diagnostic_ranges(removed);
    }

    pub(super) fn emit(&mut self, rule: &str, message: String, node: &impl Ranged) {
        self.emit_with_hint(rule, message, node, None);
    }

    pub(super) fn emit_with_hint(
        &mut self,
        rule: &str,
        message: String,
        node: &impl Ranged,
        hint: Option<String>,
    ) {
        if let Some(failed) = &mut self.typed_dict_trial_failed {
            *failed = true;
            return;
        }
        let diagnostic = super::super::super::diagnostics::at(
            self.source,
            QualityTool::Mypy,
            rule,
            message,
            node.range(),
        );
        if self.directives.suppresses(diagnostic.location.line, rule) {
            return;
        }
        let mut diagnostic = diagnostic;
        diagnostic.hint = match (hint, self.directives.hint(diagnostic.location.line, rule)) {
            (Some(hint), Some(ignore)) => Some(format!("{ignore}\n{hint}")),
            (hint, ignore) => hint.or(ignore),
        };
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn filter_import_notes(
        &mut self,
        rule: &str,
        hint: Option<String>,
    ) -> Option<String> {
        if !matches!(rule, "import-not-found" | "import-untyped") {
            return hint;
        }
        let hint = hint?;
        let mut removed = false;
        let retained: Vec<_> = hint
            .split('\n')
            .filter(|note| {
                let keep = !super::super::modules::only_once_import_note(note)
                    || self.import_notes.insert((*note).to_owned());
                removed |= !keep;
                keep
            })
            .collect();
        if !removed {
            return Some(hint);
        }
        let retained = retained.join("\n");
        (!retained.is_empty()).then_some(retained)
    }

    fn allocate_import_notes(&mut self) {
        let mut imports: Vec<_> = self
            .diagnostics
            .iter()
            .enumerate()
            .filter(|(_, diagnostic)| {
                matches!(
                    diagnostic.rule.as_str(),
                    "import-not-found" | "import-untyped"
                )
            })
            .map(|(index, _)| index)
            .collect();
        let depth = |index: usize| {
            self.diagnostics[index]
                .location
                .offset
                .and_then(|offset| self.import_note_depths.get(&offset).copied().flatten())
        };
        // Match Mypy's stable child-before-parent dependency order.
        // An ambiguous source statement keeps the existing emission order.
        if imports.iter().all(|&index| depth(index).is_some()) {
            imports.sort_by_key(|&index| std::cmp::Reverse(depth(index).unwrap_or_default()));
        }
        for index in imports {
            let rule = self.diagnostics[index].rule.clone();
            let hint = self.diagnostics[index].hint.take();
            let hint = self.filter_import_notes(&rule, hint);
            self.diagnostics[index].hint = hint;
        }
    }

    pub(super) fn emit_import_diagnostic(
        &mut self,
        range: ruff_text_size::TextRange,
        diagnostic: super::super::modules::ImportDiagnostic,
        statement: &Stmt,
    ) {
        let depth = match &diagnostic {
            super::super::modules::ImportDiagnostic::Missing(module, _) => {
                Some(module.matches('.').count())
            }
            super::super::modules::ImportDiagnostic::Issue(_) => {
                self.scopes.modules.as_ref().and_then(|lookup| {
                    let imports = lookup.imports(std::slice::from_ref(statement));
                    let first = imports.first()?;
                    imports
                        .iter()
                        .all(|import| import.module == first.module)
                        .then_some(first.module.matches('.').count())
                })
            }
        };
        let first = self.diagnostics.len();
        match diagnostic {
            super::super::modules::ImportDiagnostic::Issue(message) => {
                self.emit_import_issue(message, statement);
            }
            super::super::modules::ImportDiagnostic::Missing(module, reason) => {
                if !self.missing_import_modules.insert(module.clone()) {
                    return;
                }
                self.emit_missing_import(range, &module, reason);
            }
        }
        for emitted in &self.diagnostics[first..] {
            if !matches!(emitted.rule.as_str(), "import-not-found" | "import-untyped") {
                continue;
            }
            let Some(offset) = emitted.location.offset else {
                continue;
            };
            self.import_note_depths
                .entry(offset)
                .and_modify(|previous| {
                    if *previous != depth {
                        *previous = None;
                    }
                })
                .or_insert(depth);
        }
    }

    fn import_line_start_range(
        &self,
        offset: ruff_text_size::TextSize,
    ) -> ruff_text_size::TextRange {
        let offset = ruff_text_size::TextSize::new(
            self.source
                .text
                .floor_char_boundary(offset.to_usize().min(self.source.text.len()))
                as u32,
        );
        let index = ruff_source_file::LineIndex::from_source_text(&self.source.text);
        let line = index.line_column(offset, &self.source.text).line;
        let line_start = index.line_start(line, &self.source.text).to_usize();
        let first_char_len = self.source.text[line_start..]
            .chars()
            .next()
            .map_or(0, char::len_utf8);
        ruff_text_size::TextRange::new(
            ruff_text_size::TextSize::new(line_start as u32),
            ruff_text_size::TextSize::new((line_start + first_char_len) as u32),
        )
    }

    pub(super) fn emit_missing_import(
        &mut self,
        range: ruff_text_size::TextRange,
        module: &str,
        reason: super::super::modules::MissingImport,
    ) {
        let point = self.import_line_start_range(range.start());
        let (rule, message, hint) = match reason {
            super::super::modules::MissingImport::ApprovedStub(distribution) => (
                "import-untyped",
                format!("Library stubs not installed for \"{module}\""),
                super::super::modules::approved_stub_notes(distribution),
            ),
            super::super::modules::MissingImport::NotFound => (
                "import-not-found",
                format!("Cannot find implementation or library stub for module named \"{module}\""),
                "See https://mypy.readthedocs.io/en/stable/running_mypy.html#missing-imports"
                    .into(),
            ),
        };
        self.emit_with_hint(rule, message, &point, Some(hint));
    }

    pub(super) fn emit_missing_module_attribute(
        &mut self,
        module: &str,
        attribute: &quality_syntax::python_ast::ExprAttribute,
    ) {
        let message = self.context.missing_module_attribute(
            module,
            attribute.attr.as_str(),
            self.python_target,
        );
        self.emit("attr-defined", message, attribute);
    }
    pub(super) fn emit_import_issue(&mut self, message: String, statement: &Stmt) {
        if message.starts_with("Skipping analyzing ") {
            let point = self.import_line_start_range(statement.start());
            self.emit_with_hint(
                "import-untyped",
                message,
                &point,
                Some(
                    "See https://mypy.readthedocs.io/en/stable/running_mypy.html#missing-imports"
                        .into(),
                ),
            );
            return;
        }
        let hint = self
            .context
            .typing_import_hint(&message, self.python_target);
        self.emit_with_hint("attr-defined", message, statement, hint);
    }
    pub(in crate::infrastructure::mypy) fn finish(mut self) -> Vec<QualityDiagnostic> {
        self.finish_partial_sets();
        self.allocate_import_notes();
        self.diagnostics.extend(self.directives.unused(self.source));
        let mut diagnostics = merge_unsettled(self.diagnostics, &self.settled_diagnostics);
        diagnostics.sort_by_key(|diagnostic| {
            (
                diagnostic.location.line,
                diagnostic.location.column,
                diagnostic.rule == "used-before-def",
            )
        });
        diagnostics
    }

    pub(in crate::infrastructure::mypy::checker) fn take_variant_diagnostics(
        &mut self,
        first: usize,
    ) -> Vec<QualityDiagnostic> {
        let ranges: Vec<_> = self
            .settled_diagnostics
            .iter()
            .filter(|range| range.start >= first)
            .map(|range| range.start - first..range.end - first)
            .collect();
        self.settled_diagnostics.retain(|range| range.start < first);
        merge_unsettled(self.diagnostics.split_off(first), &ranges)
    }
}

fn merge_unsettled(
    diagnostics: Vec<QualityDiagnostic>,
    ranges: &[std::ops::Range<usize>],
) -> Vec<QualityDiagnostic> {
    let mut ranges = ranges.to_vec();
    ranges.sort_by_key(|range| range.start);
    let mut items = diagnostics.into_iter();
    let mut result = Vec::new();
    let mut offset = 0;
    for range in ranges {
        result.extend(merge_notes(
            items.by_ref().take(range.start - offset).collect(),
        ));
        result.extend(items.by_ref().take(range.len()));
        offset = range.end;
    }
    result.extend(merge_notes(items.collect()));
    result
}

pub(in crate::infrastructure::mypy::checker) fn append_variant(
    result: &mut Vec<QualityDiagnostic>,
    diagnostics: Vec<QualityDiagnostic>,
) {
    for diagnostic in diagnostics {
        if let Some(previous) = result.iter_mut().find(|previous| {
            previous.rule == diagnostic.rule
                && previous.message == diagnostic.message
                && previous.location == diagnostic.location
                && previous.severity == diagnostic.severity
        }) {
            if let Some(hint) = diagnostic.hint {
                append_hint(&mut previous.hint, &hint);
            }
            continue;
        }
        if diagnostic.severity == "note" {
            if let Some(error) = result.iter_mut().rev().find(|error| {
                error.severity == "error"
                    && error.location.line == diagnostic.location.line
                    && error.location.column == diagnostic.location.column
            }) {
                append_hint(&mut error.hint, &diagnostic.message);
                continue;
            }
        }
        result.push(diagnostic);
    }
}

fn append_hint(hint: &mut Option<String>, note: &str) {
    let previous = hint.get_or_insert_with(String::new);
    for line in note.lines() {
        if !previous.lines().any(|previous| previous == line) {
            if !previous.is_empty() {
                previous.push('\n');
            }
            previous.push_str(line);
        }
    }
}

pub(super) fn merge_notes(mut diagnostics: Vec<QualityDiagnostic>) -> Vec<QualityDiagnostic> {
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.location.line,
            diagnostic.location.column,
            diagnostic.rule == "used-before-def",
        )
    });
    let mut seen = std::collections::BTreeSet::new();
    diagnostics.retain(|diagnostic| {
        seen.insert((
            diagnostic.location.line,
            diagnostic.severity.clone(),
            diagnostic.message.clone(),
        ))
    });
    let mut latest = BTreeMap::new();
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if diagnostic.severity == "error" {
            latest.insert(
                (diagnostic.location.line, diagnostic.location.column),
                index,
            );
        }
    }
    let mut notes = Vec::new();
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if diagnostic.severity == "note" {
            if let Some(&error) =
                latest.get(&(diagnostic.location.line, diagnostic.location.column))
            {
                notes.push((index, error, diagnostic.message.clone()));
            }
        }
    }
    for (_, error, note) in &notes {
        let hint = &mut diagnostics[*error].hint;
        *hint = Some(match hint.take() {
            Some(previous) => format!("{previous}\n{note}"),
            None => note.clone(),
        });
    }
    let removed: std::collections::BTreeSet<_> = notes.iter().map(|(index, _, _)| *index).collect();
    diagnostics
        .into_iter()
        .enumerate()
        .filter_map(|(index, diagnostic)| (!removed.contains(&index)).then_some(diagnostic))
        .collect()
}
