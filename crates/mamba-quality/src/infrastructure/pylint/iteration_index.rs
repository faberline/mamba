//! Redundant lookups retain original immediate emissions and deferred loop candidates.
use super::{
    iteration_index_scan::{self, Scan},
    iteration_index_values::Lookup,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Comprehension, Expr, StmtFor};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};

impl Checker<'_> {
    fn index_name_rebound(&self, name: &str, loop_range: TextRange, access: TextRange) -> bool {
        self.scopes.last_assignment(name).is_some_and(|range| {
            let index = LineIndex::from_source_text(&self.source.text);
            let line = index.line_column(range.start(), &self.source.text).line;
            line > index
                .line_column(loop_range.start(), &self.source.text)
                .line
                && (range.start() <= access.start()
                    || line == index.line_column(access.start(), &self.source.text).line)
        })
    }
    fn indexed_candidates(
        &mut self,
        lookup: &Lookup,
        scan: &Scan<'_>,
        loop_range: Option<TextRange>,
    ) {
        let mut pending = Vec::new();
        for subscript in &scan.values {
            if lookup.dictionary
                && !matches!(
                    subscript.value.value.as_ref(),
                    Expr::Name(_) | Expr::Attribute(_)
                )
            {
                continue;
            }
            if (loop_range.is_some() && subscript.assigned) || subscript.deleted {
                return;
            }
            let Some(replacement) = self.index_replacement(lookup, subscript.value) else {
                continue;
            };
            if loop_range.is_some_and(|range| {
                self.index_name_rebound(&lookup.key, range, subscript.value.range())
                    || (!lookup.dictionary
                        && self.index_name_rebound(&lookup.value, range, subscript.value.range()))
            }) {
                continue;
            }
            if scan.nested_loops {
                pending.push((replacement, subscript.value.range()));
            } else if lookup.dictionary || !scan.conditional {
                self.emit_index_lookup(lookup.dictionary, &replacement, subscript.value.range());
            }
        }
        for (replacement, range) in pending {
            self.emit_index_lookup(lookup.dictionary, &replacement, range);
        }
    }
    fn emit_index_lookup(&mut self, dictionary: bool, replacement: &str, range: TextRange) {
        let (rule, symbol, kind) = if dictionary {
            ("R1733", "unnecessary-dict-index-lookup", "dictionary")
        } else {
            ("R1736", "unnecessary-list-index-lookup", "list")
        };
        self.emit(
            rule,
            symbol,
            &format!("Unnecessary {kind} index lookup, use '{replacement}' instead"),
            range,
        );
    }
    pub(super) fn check_indexed_iteration(&mut self, statement: &StmtFor) {
        if statement.is_async {
            return;
        }
        let Some(lookup) = self.index_lookup(&statement.target, &statement.iter) else {
            return;
        };
        self.indexed_candidates(
            &lookup,
            &iteration_index_scan::body(&statement.body),
            Some(statement.range()),
        );
    }
    pub(super) fn check_indexed_comprehensions<'a>(
        &mut self,
        generators: &'a [Comprehension],
        values: &[&'a Expr],
    ) {
        let scan = iteration_index_scan::comprehension(generators, values);
        for generator in generators {
            if let Some(lookup) = self.index_lookup(&generator.target, &generator.iter) {
                self.indexed_candidates(&lookup, &scan, None);
            }
        }
    }
}
