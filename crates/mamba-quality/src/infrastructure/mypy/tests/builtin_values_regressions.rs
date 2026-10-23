//! Record the builtin selection from the complete corpus's existing reports.
use crate::domain::{model::ToolReport, source::PythonVersion};
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs, io::Write};

pub(super) struct BuiltinRegressions {
    selected: BTreeSet<String>,
    evidence: Option<std::io::BufWriter<fs::File>>,
    first_result: bool,
    cases_seen: usize,
    comparisons: usize,
    failures: usize,
}

impl BuiltinRegressions {
    pub(super) fn new() -> Self {
        let selected: BTreeSet<String> =
            serde_json::from_str::<Vec<String>>(include_str!("builtin_values_regressions.json"))
                .unwrap()
                .into_iter()
                .collect();
        assert_eq!(
            selected.len(),
            2990,
            "Frozen builtin selection count changed"
        );
        let evidence =
            std::env::var_os("MAMBA_QUALITY_MYPY_BUILTIN_REGRESSION_EVIDENCE").map(|destination| {
                let file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(destination)
                    .expect("create new builtin regression evidence file");
                let mut writer = std::io::BufWriter::new(file);
                writer
                    .write_all(b"{\"schema\":\"mamba.quality.builtin-compatibility-regression-comparison.v1\",\"corpus_sha256\":")
                    .expect("write builtin regression evidence header");
                serde_json::to_writer(&mut writer, super::catalog::CORPUS_SHA256)
                    .expect("write builtin regression corpus hash");
                writer
                    .write_all(b",\"cases\":[")
                    .expect("write builtin regression cases header");
                writer
            });
        Self {
            selected,
            evidence,
            first_result: true,
            cases_seen: 0,
            comparisons: 0,
            failures: 0,
        }
    }

    pub(super) fn select(&mut self, name: &str) -> bool {
        let selected = self.selected.contains(name);
        self.cases_seen += usize::from(selected);
        selected
    }

    pub(super) fn record(
        &mut self,
        case: &Value,
        version: PythonVersion,
        target: usize,
        report: &ToolReport,
        actual: &Value,
    ) {
        // Reuse the same nine-field projection, but retain the serialized rule check.
        let mut actual = actual.clone();
        for (row, diagnostic) in actual
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(&report.diagnostics)
        {
            row["rule"] = serde_json::to_value(diagnostic).unwrap()["rule"].clone();
        }
        let source = case["source"].as_str().unwrap();
        let expected = &case["expected"][target];
        let equal = actual == *expected;
        self.failures += usize::from(!equal);
        self.comparisons += 1;
        if let Some(writer) = self.evidence.as_mut() {
            if !self.first_result {
                writer
                    .write_all(b",")
                    .expect("write builtin regression evidence separator");
            }
            serde_json::to_writer(
                &mut *writer,
                &json!({"name":case["name"],"source":source,"python":version.name(),
                "expected":expected,"actual":actual,"equal":equal}),
            )
            .expect("write builtin regression evidence row");
            self.first_result = false;
        }
    }

    pub(super) fn finish(self) {
        assert_eq!(self.cases_seen, self.selected.len());
        assert_eq!(self.comparisons, self.cases_seen * 3);
        assert_eq!(
            self.comparisons, 8970,
            "Frozen builtin comparison count changed"
        );
        if let Some(mut writer) = self.evidence {
            write!(
                writer,
                "],\"failed\":{},\"whole_suite_accepted\":false}}",
                self.failures
            )
            .expect("write builtin regression evidence footer");
            writer
                .flush()
                .expect("flush complete builtin regression evidence");
        }
        assert_eq!(
            self.failures, 0,
            "Pinned mypy diagnostic comparisons failed"
        );
    }
}
