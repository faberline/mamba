//! Entry failures retain fatal positions and caller input order.
use crate::domain::{
    model::{QualityDiagnostic, QualityLocation, QualityTool},
    source::QualityWorkspace,
};
use std::{fs, io::ErrorKind};

pub(super) fn diagnostics(workspace: &QualityWorkspace) -> Vec<QualityDiagnostic> {
    workspace
        .issues
        .iter()
        .filter_map(|issue| {
            let path = issue.path.as_ref()?;
            if path.extension().is_some_and(|value| value == "pyi") {
                return None;
            }
            let (rule, message) = match issue.code.as_str() {
                "path-read" => {
                    let error = fs::metadata(path).err()?;
                    if !selected_path_error(&error) {
                        return None;
                    }
                    let name = if path.is_absolute()
                        && (error.kind() == ErrorKind::PermissionDenied
                            || selected_path_errno(&error))
                    {
                        let root = crate::infrastructure::metadata::project_root(path);
                        path.strip_prefix(root).ok()?.to_string_lossy().into_owned()
                    } else if path.is_absolute() {
                        path.file_name()?.to_string_lossy().into_owned()
                    } else {
                        path.to_string_lossy().into_owned()
                    };
                    ("F0001", format!("No module named {name}"))
                }
                "source-read" => ("F0010", super::runner_read_errors::message(path)?),
                _ => return None,
            };
            Some(QualityDiagnostic {
                tool: QualityTool::Pylint,
                rule: rule.into(),
                message,
                hint: None,
                path: path.clone(),
                location: QualityLocation {
                    line: 1,
                    column: 0,
                    end_line: None,
                    end_column: None,
                    offset: None,
                    end_offset: None,
                },
                severity: "fatal".into(),
                fix: None,
            })
        })
        .collect()
}

fn selected_path_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::NotFound | ErrorKind::PermissionDenied
    ) || selected_path_errno(error)
}

fn selected_path_errno(error: &std::io::Error) -> bool {
    if matches!(
        error.kind(),
        ErrorKind::NotADirectory | ErrorKind::InvalidFilename
    ) {
        return true;
    }
    // FilesystemLoop is not a stable ErrorKind in Rust 1.97.
    #[cfg(target_os = "macos")]
    const ELOOP: i32 = 62;
    #[cfg(target_os = "linux")]
    const ELOOP: i32 = 40;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        error.raw_os_error() == Some(ELOOP)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        false
    }
}
