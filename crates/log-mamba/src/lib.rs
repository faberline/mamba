//! Mamba binding for `mamba-log`.
//!
//! Exposes structured logging capabilities to Mamba scripts via the
//! `mamba-registry` infrastructure.
//!
//! # Module names
//!
//! Import in Mamba as `mambalibs.logging` or `cclab.log`:
//! ```python
//! from mambalibs.logging import get_logger, info, error, debug, warning
//! from cclab.log import get_logger, info, error, debug, warning
//! ```
//! Both module names register the same native logging entrypoints and preserve
//! existing behavior.

pub mod methods;
pub mod types;

use linkme::distributed_slice;
use mamba_registry::{rt_sym, MambaModule, ModuleRegistrar, MAMBA_MODULES};

fn register_log_surface(r: &mut ModuleRegistrar) {
    use crate::methods::{
        mb_log_debug, mb_log_error, mb_log_get_logger, mb_log_info, mb_log_warning,
    };

    r.add_symbols([
        rt_sym!(
            "get_logger",
            mb_log_get_logger,
            "get_logger(name: str) -> logger"
        ),
        rt_sym!("info", mb_log_info, "info(logger, msg: str) -> None"),
        rt_sym!("error", mb_log_error, "error(logger, msg: str) -> None"),
        rt_sym!("debug", mb_log_debug, "debug(logger, msg: str) -> None"),
        rt_sym!(
            "warning",
            mb_log_warning,
            "warning(logger, msg: str) -> None"
        ),
    ]);
}

// ── LogMambaModule ────────────────────────────────────────────────────────────

/// The `cclab.log` native module descriptor.
pub struct LogMambaModule;

impl MambaModule for LogMambaModule {
    fn name(&self) -> &'static str {
        "cclab.log"
    }

    fn doc(&self) -> &'static str {
        "Mamba bindings for mamba-log — structured logging"
    }

    fn register(&self, r: &mut ModuleRegistrar) {
        register_log_surface(r);
    }
}

// ── MambalibsLoggingModule ────────────────────────────────────────────────────

/// The `mambalibs.logging` native module descriptor.
pub struct MambalibsLoggingModule;

impl MambaModule for MambalibsLoggingModule {
    fn name(&self) -> &'static str {
        "mambalibs.logging"
    }

    fn doc(&self) -> &'static str {
        "Mamba bindings for mamba-log — structured logging"
    }

    fn register(&self, r: &mut ModuleRegistrar) {
        register_log_surface(r);
    }
}

// ── Auto-registration ─────────────────────────────────────────────────────────

#[distributed_slice(MAMBA_MODULES)]
static LOG_MAMBA_MODULE: &dyn MambaModule = &LogMambaModule;

#[distributed_slice(MAMBA_MODULES)]
static MAMBALIBS_LOGGING_MODULE: &dyn MambaModule = &MambalibsLoggingModule;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::methods::{
        mb_log_debug, mb_log_error, mb_log_get_logger, mb_log_info, mb_log_warning,
    };
    use mamba_registry::{all_modules, find_module, RuntimeSymbol};
    use std::collections::BTreeMap;

    fn expected_exports() -> BTreeMap<&'static str, (&'static str, usize, &'static str)> {
        BTreeMap::from([
            (
                "get_logger",
                (
                    "mb_log_get_logger",
                    mb_log_get_logger as *const () as usize,
                    "get_logger(name: str) -> logger",
                ),
            ),
            (
                "info",
                (
                    "mb_log_info",
                    mb_log_info as *const () as usize,
                    "info(logger, msg: str) -> None",
                ),
            ),
            (
                "error",
                (
                    "mb_log_error",
                    mb_log_error as *const () as usize,
                    "error(logger, msg: str) -> None",
                ),
            ),
            (
                "debug",
                (
                    "mb_log_debug",
                    mb_log_debug as *const () as usize,
                    "debug(logger, msg: str) -> None",
                ),
            ),
            (
                "warning",
                (
                    "mb_log_warning",
                    mb_log_warning as *const () as usize,
                    "warning(logger, msg: str) -> None",
                ),
            ),
        ])
    }

    fn collect_symbols_by_name(module: &dyn MambaModule) -> BTreeMap<&'static str, RuntimeSymbol> {
        let mut registrar = ModuleRegistrar::new();
        module.register(&mut registrar);
        assert!(
            registrar.values().is_empty(),
            "module {} should not register non-function values",
            module.name()
        );
        let symbols = registrar.into_symbols();
        assert_eq!(
            symbols.len(),
            5,
            "module {} must register exactly 5 symbols",
            module.name()
        );

        let mut by_name = BTreeMap::new();
        for sym in symbols {
            let prev = by_name.insert(sym.name, sym);
            assert!(
                prev.is_none(),
                "module {} registered duplicate export name",
                module.name()
            );
        }
        by_name
    }

    #[test]
    fn both_module_names_resolve_once_in_registry_with_missing_name_control() {
        let cclab_matches: Vec<_> = all_modules().filter(|m| m.name() == "cclab.log").collect();
        let mambalibs_matches: Vec<_> = all_modules()
            .filter(|m| m.name() == "mambalibs.logging")
            .collect();

        assert_eq!(
            cclab_matches.len(),
            1,
            "cclab.log must appear exactly once in all_modules()"
        );
        assert_eq!(
            mambalibs_matches.len(),
            1,
            "mambalibs.logging must appear exactly once in all_modules()"
        );

        let cclab_mod = find_module("cclab.log").expect("cclab.log must resolve via find_module");
        let mambalibs_mod = find_module("mambalibs.logging")
            .expect("mambalibs.logging must resolve via find_module");
        assert_eq!(cclab_mod.name(), "cclab.log");
        assert_eq!(mambalibs_mod.name(), "mambalibs.logging");

        // Missing-name controls
        assert!(find_module("mambalibs.log").is_none());
        assert!(find_module("cclab.logging").is_none());
        assert_eq!(
            all_modules()
                .filter(|m| m.name() == "mambalibs.log" || m.name() == "cclab.logging")
                .count(),
            0
        );
    }

    #[test]
    fn descriptors_and_registry_modules_expose_identical_five_ffi_exports() {
        let expected = expected_exports();

        let direct_cclab = collect_symbols_by_name(&LogMambaModule);
        let direct_mambalibs = collect_symbols_by_name(&MambalibsLoggingModule);

        let registry_cclab =
            collect_symbols_by_name(find_module("cclab.log").expect("cclab.log registered"));
        let registry_mambalibs = collect_symbols_by_name(
            find_module("mambalibs.logging").expect("mambalibs.logging registered"),
        );

        for (label, table) in [
            ("LogMambaModule", &direct_cclab),
            ("MambalibsLoggingModule", &direct_mambalibs),
            ("registry cclab.log", &registry_cclab),
            ("registry mambalibs.logging", &registry_mambalibs),
        ] {
            let actual: BTreeMap<&'static str, (&'static str, usize, &'static str)> = table
                .iter()
                .map(|(&name, sym)| (name, (sym.ffi_name, sym.func_ptr, sym.signature)))
                .collect();
            assert_eq!(
                actual, expected,
                "{label} exports must match the 5 existing FFI functions by export name"
            );
        }

        for (&export_name, cclab_sym) in &registry_cclab {
            let mambalibs_sym = registry_mambalibs
                .get(export_name)
                .expect("matching export name in mambalibs.logging");
            assert_eq!(cclab_sym.name, mambalibs_sym.name);
            assert_eq!(cclab_sym.ffi_name, mambalibs_sym.ffi_name);
            assert_eq!(cclab_sym.func_ptr, mambalibs_sym.func_ptr);
            assert_eq!(cclab_sym.signature, mambalibs_sym.signature);
        }
    }
}
