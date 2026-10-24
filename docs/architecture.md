# Code map at baseline 8975

This page describes the committed 8975 baseline. It is a map of current code, not a claim that the planned architecture is complete.

The root `mamba` package still contains several responsibilities. Its [library root](../src/lib.rs) declares compiler stages, the runtime, and `pkgmanage`. Its [binary entry](../src/main.rs) still builds command arguments and routes compiler, package manager, and Quality commands. The [root manifest](../Cargo.toml) lists the root package and workspace members. This snapshot has no `ddd.toml`.

The compiler path runs from lexer and parser through name resolution, types, HIR, MIR, and code generation. The [source map](../llms.txt) names these stages. The root library declares their modules in [src/lib.rs](../src/lib.rs). The runtime also lives under that root library. The [runtime integration guide](integration/native-runtime-integration.md) identifies `mamba-registry` as the binding contract and `mamba` as the runtime that loads bindings.

The package manager code lives under [src/pkgmanage](../src/pkgmanage/mod.rs). The root binary imports its commands and dispatches them in [src/main.rs](../src/main.rs). The [product requirements](product/package-manager.md) describe the user workflow; they do not establish a completed source split.

Quality has a separate Rust crate, [mamba-quality](../crates/mamba-quality/src/lib.rs). The root binary routes `mamba quality` through [src/quality_command.rs](../src/quality_command.rs) to that crate. Its command shape includes check, fix, format, and info in [command_quality.rs](../src/quality_command/command_quality.rs). The [Quality progress record](operations/python-quality/native-progress-20261023.md) says this integration does not complete the full Quality plan.

The root binary force-links selected native binding crates when the `native-modules` feature is on. The [integration guide](integration/native-runtime-integration.md) distinguishes the first-party mode from an external mode that is still a design.

This map must be checked again against the actual main commit after the separate CLI entry change lands. The 8975 baseline does not contain that change.
