use super::*;

fn paths(command: Command) -> Command {
    command.arg(Arg::new("paths").value_name("PATH").num_args(0..))
}

fn json(command: Command) -> Command {
    command.arg(
        Arg::new("json")
            .long("json")
            .action(ArgAction::SetTrue)
            .help("Print a mamba.quality.v2 JSON report"),
    )
}

pub(super) fn command_quality() -> Command {
    paths(
        Command::new("quality")
            .about("Check Python project quality with a fixed Rust-native profile")
            .subcommand_precedence_over_arg(true)
            .args_conflicts_with_subcommands(true)
            .subcommand(json(paths(
                Command::new("check").about("Check all five tools without changing source files"),
            )))
            .subcommand(
                json(paths(Command::new("fix").about(
                    "Apply safe Ruff fixes, isort, and Black, then check all tools",
                )))
                .arg(
                    Arg::new("diff")
                        .long("diff")
                        .action(ArgAction::SetTrue)
                        .help("Show candidate changes without writing source files"),
                ),
            )
            .subcommand(
                json(paths(
                    Command::new("format").about("Sort imports and format source files"),
                ))
                .arg(
                    Arg::new("check")
                        .long("check")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("diff")
                        .help("Return a quality failure if formatting changes are needed"),
                )
                .arg(
                    Arg::new("diff")
                        .long("diff")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("check")
                        .help("Show changes without writing source files"),
                ),
            )
            .subcommand(json(paths(Command::new("info").about(
                "Show the fixed profile, target Python version, and dependency sources",
            )))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_does_not_require_an_interpreter() {
        let parsed = command_quality()
            .try_get_matches_from(["quality", "src", "main.py"])
            .expect("default quality check");
        assert_eq!(parsed.get_many::<String>("paths").unwrap().count(), 2);
    }

    #[test]
    fn format_check_and_diff_cannot_write_together() {
        assert!(command_quality()
            .try_get_matches_from(["quality", "format", "--check", "--diff"])
            .is_err());
        assert!(command_quality()
            .try_get_matches_from(["quality", "check", "--python", "python"])
            .is_err());
    }
}
