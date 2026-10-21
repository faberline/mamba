# Cclab Log Mamba

## Brief

Cclab Log Mamba is the Mamba native binding for the `mamba-log` structured
logging API.

It registers both `mambalibs.logging` and `cclab.log` through the shared Mamba
registry and exposes native-call entrypoints for `get_logger`, `info`, `error`,
`debug`, and `warning`. Both module names point to the same underlying FFI
functions and preserve current logging behavior without introducing a second
logger implementation or state store. The binding owns the Mamba value
conversion boundary, logger handle fallback behavior, and JSON-shaped log
records emitted by the native methods.

```python
from mambalibs.logging import get_logger, info, error, debug, warning
from cclab.log import get_logger, info, error, debug, warning
```

## Capabilities

A promise with no gate under it is not claimed.

### Capability Index

| Capability | Root WI | Notes |
|---|---:|---|
| Mamba Structured Logging Binding | - | exposes `mambalibs.logging` and `cclab.log` logger creation plus info/error/debug/warning native methods |

### Mamba Structured Logging Binding

Cclab Log Mamba exposes `mamba-log` structured logging to Mamba scripts through
the `mambalibs.logging` and `cclab.log` native modules, where the alias
preserves existing behavior across logger creation and info/error/debug/warning
methods that accept Mamba string values and emit JSON-structured log records.

- Root WI: none; this capability predates the tracker.
- Surfaces: Mamba modules: `mambalibs.logging`, `cclab.log`; Native ABI:
  `mb_log_get_logger`, `mb_log_info`, `mb_log_error`, `mb_log_debug`,
  `mb_log_warning`; Rust module registrars: `MambalibsLoggingModule`,
  `LogMambaModule`
- Gate — behavior: `cargo test -p log-mamba`
- Gate: `cargo test -p log-mamba`
- Source: `crates/log-mamba/src/lib.rs`,
  `crates/log-mamba/src/methods.rs`,
  `crates/log-mamba/tests/methods_test.rs`
- Evidence: `cargo test -p log-mamba`; crates/log-mamba/src/lib.rs;
  crates/log-mamba/src/methods.rs;
  crates/log-mamba/tests/methods_test.rs
