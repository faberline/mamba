use super::*;
use mamba_quality::{QualityOperation, QualityRequest};
use std::path::PathBuf;

pub(super) fn cmd_quality(args: &ArgMatches) -> Result<()> {
    let (operation, arguments) = match args.subcommand() {
        Some(("check", arguments)) => (QualityOperation::Check, arguments),
        Some(("fix", arguments)) => (
            QualityOperation::Fix {
                diff: arguments.get_flag("diff"),
            },
            arguments,
        ),
        Some(("format", arguments)) => (
            QualityOperation::Format {
                check: arguments.get_flag("check"),
                diff: arguments.get_flag("diff"),
            },
            arguments,
        ),
        Some(("info", arguments)) => (QualityOperation::Info, arguments),
        _ => (QualityOperation::Check, args),
    };
    let paths = arguments
        .get_many::<String>("paths")
        .map(|paths| paths.map(PathBuf::from).collect())
        .unwrap_or_default();
    let report = mamba_quality::run(&QualityRequest::new(operation, paths));
    let json = arguments
        .try_get_one::<bool>("json")
        .ok()
        .flatten()
        .copied()
        .unwrap_or(false);
    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("quality-output: cannot serialize report: {error}");
                std::process::exit(2);
            }
        }
    } else {
        print_report(&report);
    }
    if report.exit_code != 0 {
        std::process::exit(report.exit_code);
    }
    Ok(())
}

fn print_report(report: &mamba_quality::QualityReport) {
    for project in &report.projects {
        println!(
            "{}: Python {}",
            project.root.display(),
            project.python_version.name()
        );
    }
    if report.operation == QualityOperation::Info {
        for (tool, version) in &report.profile.versions {
            println!("{tool}: {version}");
        }
        println!("Line width: {}", report.profile.line_width);
        println!(
            "Ruff: {} (preview off; safe fixes only)",
            report.profile.ruff.select.join(",")
        );
        println!("isort: Black profile");
        println!(
            "mypy strict: {}",
            report.profile.mypy.strict_flags.join(" ")
        );
        println!(
            "Pylint: {} default enabled messages",
            report.profile.pylint.rules.len()
        );
        println!("Black: stable style; string normalization; magic trailing commas");
        println!("Pydantic: init_typed; init_forbid_extra; warn_required_dynamic_aliases");
        for tool in &report.tools {
            for remaining in &tool.remaining {
                println!("Incomplete: {remaining}");
            }
        }
        for project in &report.projects {
            for dependency in &project.dependencies {
                println!(
                    "Dependency data: {}{}",
                    dependency.identity,
                    dependency
                        .path
                        .as_ref()
                        .map(|path| format!(" ({})", path.display()))
                        .unwrap_or_default()
                );
            }
        }
    } else {
        for tool in &report.tools {
            for diagnostic in &tool.diagnostics {
                if diagnostic.location.line
                    == mamba_quality::application::quality::QualityLocation::UNPOSITIONED_LINE
                {
                    println!(
                        "{}: {} {}: {}",
                        diagnostic.path.display(),
                        diagnostic.tool.name(),
                        diagnostic.rule,
                        diagnostic.message
                    );
                    continue;
                }
                println!(
                    "{}:{}:{}: {} {}: {}",
                    diagnostic.path.display(),
                    diagnostic.location.line,
                    diagnostic.location.column,
                    diagnostic.tool.name(),
                    diagnostic.rule,
                    diagnostic.message
                );
            }
            for remaining in &tool.remaining {
                println!("Incomplete: {remaining}");
            }
        }
        for change in &report.changes {
            if let Some(diff) = &change.diff {
                print!("{diff}");
            }
        }
        println!(
            "{} files; {} changed; {} written",
            report.files.discovered, report.files.changed, report.files.written
        );
    }
    for issue in &report.issues {
        let path = issue
            .path
            .as_ref()
            .map(|path| format!("{}: ", path.display()))
            .unwrap_or_default();
        eprintln!("{path}{}: {}", issue.code, issue.message);
    }
}
