# Code map after #239

This page maps the current main tree after #239 merged at `4a76f8bbf7de31776fb8bad1d18aa5794c54699f`. It describes the source layout. It does not claim that the planned product architecture is complete.

The root `mamba` package still contains several responsibilities. Its [library root](../src/lib.rs) declares compiler stages, the runtime, and `pkgmanage`. The [root manifest](../Cargo.toml) lists the root package and workspace members. This tree has no `ddd.toml`.

The compiler path runs from lexer and parser through name resolution, types, HIR, MIR, and code generation. The [source map](../llms.txt) names these stages. The root library declares their modules in [src/lib.rs](../src/lib.rs). The runtime also lives under that root library. The [runtime integration guide](integration/native-runtime-integration.md) identifies `mamba-registry` as the binding contract and `mamba` as the runtime that loads bindings.

The package manager code lives under [src/pkgmanage](../src/pkgmanage/mod.rs). The thin binary entry [src/main.rs](../src/main.rs) declares `cli`, `quality_command`, and `standard_cli`, keeps feature-gated native-binding links, and calls `cli::run()`. Command construction and top-level routing live in [src/cli.rs](../src/cli.rs). That module builds the Clap command tree and routes package-manager commands to `src/pkgmanage`, Quality to `src/quality_command.rs`, and standard commands to their handlers. The run, test, benchmark, and miscellaneous handlers live in [src/cli/run.rs](../src/cli/run.rs), [src/cli/test.rs](../src/cli/test.rs), [src/cli/bench.rs](../src/cli/bench.rs), and [src/cli/misc.rs](../src/cli/misc.rs). This source layout does not mean the product architecture or delivery plan is complete.

Quality has a separate Rust crate, [mamba-quality](../crates/mamba-quality/src/lib.rs). The root binary routes `mamba quality` through [src/quality_command.rs](../src/quality_command.rs) to that crate. Its command shape includes check, fix, format, and info in [command_quality.rs](../src/quality_command/command_quality.rs). The [Quality progress record](operations/python-quality/native-progress-20261023.md) says this integration does not complete the full Quality plan.

The root binary keeps feature-gated links to selected native binding crates when `native-modules` is enabled. The [integration guide](integration/native-runtime-integration.md) distinguishes the first-party mode from an external mode that is still a design.
