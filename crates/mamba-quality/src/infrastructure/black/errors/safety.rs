//! Black's reference safety error keeps a real Rust-generated diagnostic file.
use crate::domain::{
    model::QualityIssue,
    source::{QualityWorkspace, SourceSnapshot},
};
use crate::infrastructure::failure_logs::FailureLogs;
use mamba_language::application::quality_syntax::{self, PythonTarget};

mod arguments;
mod ast;
mod compound;
mod diff;
mod expressions;
mod interpolations;
mod patterns;
mod sequence_matcher;
mod statements;
#[cfg(test)]
mod tests;
mod tree;
use std::io::Write;

pub(in crate::infrastructure::black) fn error(
    source: &SourceSnapshot,
    candidate: &str,
    target: PythonTarget,
    workspace: &QualityWorkspace,
    logs: &FailureLogs,
) -> QualityIssue {
    let saved = (|| {
        let mut file = logs.file(workspace, "blk_", ".log")?;
        let difference =
            difference(&source.text, candidate, target).map_err(std::io::Error::other)?;
        file.write_all(difference.as_bytes())?;
        if !difference.is_empty() && !difference.ends_with('\n') {
            file.write_all(b"\n")?;
        }
        let (_, path) = file.keep().map_err(|error| error.error)?;
        Ok::<_, std::io::Error>(path)
    })();
    let path = match saved {
        Ok(path) => path,
        Err(error) => {
            return QualityIssue::new(
                "black-safety-log",
                format!("Could not save the Black program-structure difference: {error}"),
                Some(source.path.clone()),
            );
        }
    };
    // These versions identify the locked comparison runtime, never a Python process.
    let version = match target {
        PythonTarget::Py312 => "3.12.11",
        PythonTarget::Py313 => "3.13.5",
        PythonTarget::Py314 => "3.14.0",
    };
    QualityIssue::new(
        "black.parsing.ASTSafetyError",
        format!(
            "INTERNAL ERROR: Black 26.10.0 on Python (CPython) {version} produced code that is not equivalent to the source.  Please report a bug on https://github.com/psf/black/issues.  This diff might be helpful: {}",
            path.display()
        ),
        Some(source.path.clone()),
    )
}

fn difference(source: &str, candidate: &str, target: PythonTarget) -> Result<String, String> {
    let source = quality_syntax::parse(source, target, false)?;
    let candidate = quality_syntax::parse(candidate, target, false)?;
    if !source.is_valid() || !candidate.is_valid() {
        return Err("Cannot render a CPython AST from an invalid syntax tree".into());
    }
    let source = tree::stringify(&ast::module(&source)?);
    let candidate = tree::stringify(&ast::module(&candidate)?);
    Ok(diff::unified(&source, &candidate))
}
