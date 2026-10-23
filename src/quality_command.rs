use anyhow::Result;
use clap::{Arg, ArgAction, ArgMatches, Command};

#[path = "quality_command/cmd_quality.rs"]
mod dispatch;
#[path = "quality_command/command_quality.rs"]
mod grammar;

pub(super) fn command_quality() -> Command {
    grammar::command_quality()
}

pub(super) fn cmd_quality(args: &ArgMatches) -> Result<()> {
    dispatch::cmd_quality(args)
}
