# 0001: Product delivery order

**Recorded decision:** 2026-09-03. **Record source:** [product requirements index](../product/README.md).

## Decision in the existing record

The package manager is the first deliverable and does not require the mamba runtime. Mambalibs kits reach CPython users as PyO3 wheels through that package manager. The compiler and CPython runtime replacement follow. A kit's `MambaModule` carrier follows its wheel and must match the wheel's Python API. The index says this order was decided on 2026-09-03 and that runtime tiers came first before that date.

The [project README](../../README.md) also states the package manager, wheels, and runtime replacement order. The [product index](../product/README.md) records the three horizons.

## Interpretation for this document set

This ADR records the order already written in those files. It does not add a new decision or change a release gate. The product index and [status](../../STATUS.md) remain the sources for delivered scope. The [roadmap](../../ROADMAP.md) remains the source for future work.

## Current code boundary

In the current main tree after #239 merged at `4a76f8bbf7de31776fb8bad1d18aa5794c54699f`, [src/lib.rs](../../src/lib.rs) still declares compiler, runtime, and package-manager modules in the root crate. The thin [src/main.rs](../../src/main.rs) declares modules, retains feature-gated native links, and calls `cli::run()`. Command construction and routing live in [src/cli.rs](../../src/cli.rs). Its run handler is in [src/cli/run.rs](../../src/cli/run.rs), test handler in [src/cli/test.rs](../../src/cli/test.rs), benchmark handler in [src/cli/bench.rs](../../src/cli/bench.rs), and miscellaneous handlers in [src/cli/misc.rs](../../src/cli/misc.rs). This entry-point source change does not complete a code split or prove runtime parity.
