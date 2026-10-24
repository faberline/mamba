mod cli;
mod quality_command;
mod standard_cli;

// Force-link Mamba native binding crates so their #[distributed_slice(MAMBA_MODULES)]
// entries are included in the binary.  Without these, `mamba run` cannot resolve
// imports like `from mambalibs.pg import connect`.
#[cfg(feature = "native-modules")]
use agentkit_binding as _;
#[cfg(feature = "native-modules")]
use log_mamba as _;
#[cfg(feature = "native-modules")]
use mambalibs_http_binding as _;
#[cfg(feature = "native-modules")]
use mcp_mamba as _;
#[cfg(feature = "native-modules")]
use pgkit_binding as _;
#[cfg(feature = "native-modules")]
use qc_mamba as _;

fn main() -> anyhow::Result<()> {
    cli::run()
}
