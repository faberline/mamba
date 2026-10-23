//! Reference build failures retain the original fatal diagnostic and an actual report.
use crate::domain::{
    model::{QualityDiagnostic, QualityTool},
    source::{QualityWorkspace, SourceSnapshot},
};
use crate::infrastructure::failure_logs::FailureLogs;
use std::{io::Write, time::SystemTime};

pub(super) fn diagnostic(
    source: &SourceSnapshot,
    workspace: &QualityWorkspace,
    logs: &FailureLogs,
) -> Result<QualityDiagnostic, std::io::Error> {
    report(
        source,
        workspace,
        logs,
        "Native Rust reproduction: sorting mixed bytes and str slot constants.\n\
         Reference exception: TypeError: '<' not supported between instances of 'bytes' and 'str'",
    )
}

pub(super) fn import_permission(
    source: &SourceSnapshot,
    error: &str,
    workspace: &QualityWorkspace,
    logs: &FailureLogs,
) -> Result<QualityDiagnostic, std::io::Error> {
    report(
        source,
        workspace,
        logs,
        &format!(
            "Native Rust reproduction: isort source classification at Pylint module exit.\n\
             Reference exception: PermissionError: {error}"
        ),
    )
}

fn report(
    source: &SourceSnapshot,
    workspace: &QualityWorkspace,
    logs: &FailureLogs,
    reason: &str,
) -> Result<QualityDiagnostic, std::io::Error> {
    let seconds = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let directory = logs.directory(workspace, "mamba-quality-pylint-")?;
    let path = directory.path().join(crash_filename(seconds));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    writeln!(
        file,
        "Pinned Pylint 4.1.2 / Astroid 4.3.3 build failure\n\
         {reason}\n\
         Source: {}\nSHA-256: {}\n\n```python\n{}\n```",
        source.path.display(),
        source.sha256,
        source.text
    )?;
    file.sync_all()?;
    let _retained = directory.keep();
    let name = source.path.display();
    let mut diagnostic = super::super::diagnostics::at(
        source,
        QualityTool::Pylint,
        "F0002",
        format!(
            "{name}: Fatal error while checking '{name}'. Please open an issue in our bug tracker so we address this. There is a pre-filled template that you can use in '{}'.",
            path.display()
        ),
        super::super::diagnostics::point(0),
    );
    diagnostic.severity = "fatal".into();
    diagnostic.location.end_line = None;
    diagnostic.location.end_column = None;
    diagnostic.location.end_offset = None;
    Ok(diagnostic)
}

fn crash_filename(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day = days - era * 146_097;
    let year = (day - day / 1460 + day / 36_524 - day / 146_096) / 365;
    let day_of_year = day - (365 * year + year / 4 - year / 100);
    let month = (5 * day_of_year + 2) / 153;
    let date = day_of_year - (153 * month + 2) / 5 + 1;
    let calendar_month = month + if month < 10 { 3 } else { -9 };
    let calendar_year = year + era * 400 + i64::from(calendar_month <= 2);
    format!(
        "pylint-crash-{calendar_year:04}-{calendar_month:02}-{date:02}-{:02}-{:02}-{:02}.txt",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn utc_filename_retains_reference_shape() {
        for (seconds, expected) in [
            (0, "pylint-crash-1970-01-01-00-00-00.txt"),
            (951_782_400, "pylint-crash-2000-02-29-00-00-00.txt"),
            (1_709_164_799, "pylint-crash-2024-02-28-23-59-59.txt"),
        ] {
            assert_eq!(super::crash_filename(seconds), expected);
        }
    }
}
