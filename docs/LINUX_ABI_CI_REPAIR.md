# Linux libc compilation repair — staged validation

This is a separate source repair stacked on CI bootstrap PR #247, not a
baseline waiver or release qualification. It changes only the three native
ABI seams diagnosed by the first hosted run and adds four unit regressions.
All 32 original test-function bodies in those files remain byte-identical.

## First evidence

Run [40120611890](https://github.com/faberline/mamba/actions/runs/40120611890)
at `d81321bd899a766be1ffb537311c3a72996b43f3` failed before test enumeration.
The unfiltered library inventory exited 101 after about 28.5 minutes with seven
compiler errors. The contract inventory also exited 101 with the same errors.
There were no product test verdicts and no successful inventory; denominators
remain unknown. This is separate from the historical native 45-failure record.

| Cause | Narrow repair |
| --- | --- |
| `libc::POLLMSG` is not exported by libc 0.2.186 | Retain the Linux module attribute using Linux UAPI's `0x0400` poll mask; do not substitute the unrelated epoll constant |
| Linux `sockaddr_in` / `sockaddr_in6` omit BSD length fields | Skip only the two length assignments on Linux; retain the original statements on all other hosts |
| GNU/Linux resource selectors and `RLIM_NLIMITS` are unsigned, while the Python argument parser/macOS use C int | Compare bounds through lossless i64 widening, then infer the syscall selector's ABI type after validation |

Source references: [Linux UAPI poll.h](https://github.com/torvalds/linux/blob/v6.11/include/uapi/asm-generic/poll.h),
[libc 0.2.186 GNU resource ABI](https://github.com/rust-lang/libc/blob/0.2.186/src/unix/linux_like/linux/gnu/mod.rs),
and [CPython 3.12.11 select module](https://github.com/python/cpython/blob/v3.12.11/Modules/selectmodule.c).
Hosted C-header and pinned-CPython controls independently compare the constant
and numeric IPv6 address result. These ABI controls do not replace Mamba tests
or the plane-D real-oracle gate.

## Timeout diagnosis

The first perf-policy inventory was killed at its 1200-second deadline while
stderr still reported dependency compilation (`ruff_linter`); stdout was empty,
and product execution never started. The subsequent contract command reused
compiled dependencies in that same job, then reached the seven source errors.
This is cold compilation evidence, not a perf-policy test hang or a claim of
persistent warm-cache execution. The bootstrap caches downloads, not binaries.

This repair separates the contract inventory/compilation deadline (2400s) from
the unchanged execution deadline (1200s). Job ceiling remains 55 minutes and
matrix concurrency remains two. No retry or timeout-as-pass is introduced.
The unfiltered library still has its existing 2400s phase deadline.

## Regression and acceptance boundaries

The new library cases are:

* `runtime::stdlib::select_mod::tests::test_linux_pollmsg_matches_uapi_value`
* `runtime::stdlib::socket_mod::tests::test_sockaddr_v4_native_family_address_and_port`
* `runtime::stdlib::socket_mod::tests::test_getnameinfo_numeric_ipv6_native_sockaddr`
* `runtime::stdlib::resource_mod::tests::test_rlimit_selector_boundaries_rejected_before_abi_conversion`

The workflow requires all four exact names in the complete library inventory
before executing the unfiltered suite. Missing or duplicate cases block the
receipt; these requirements do not select a focused subset.

Local validation is limited to static checks, 29 receipt controls and a tiny
standalone native C-header probe. The Mac probe confirms the retained BSD path,
not Linux behavior. Rustfmt adds no findings: the two existing socket formatting
findings remain; the other two files pass. No Mamba library, integration, CLI,
E2E or performance command is run locally.

Actual Mamba compilation/inventory/execution must be validated by the hosted
workflow at the submitted head. Preserve failed and ignored cases. A passing
ABI probe, focused regression or default-feature compile does not qualify the
declared Linux release artifacts: ship-profile `--features release` builds and
the full applicable suite remain required, with publishing blocked until all
required evidence passes. No macOS/APFS or performance qualification follows
from Ubuntu checks. Existing metric and central AW activation gaps remain.

## Exact-case execution before the full baseline

The first repair run compiled and listed all 8,582 library cases, including
the four regressions, but the unfiltered process crashed before reaching them.
Run [40125433604](https://github.com/faberline/mamba/actions/runs/40125433604)
retains 146 passed / 5 ignored verdicts, a native SIGSEGV, and 8,431 cases
without a completed verdict. Its perf-policy target passed 12/12; its fixture
contract target passed 16 and failed 1. These records remain unchanged.

The follow-up workflow runs each of the four regressions with `--exact` in a
separate process before the full baseline. Each receipt first records the
complete catalogue, requires its exact case, then checks a one-case filtered
inventory and verdict. The filtered-out count must equal the complete catalogue
size minus one. Missing, duplicated, substituted, ignored, failed or incomplete
cases block the selection. Source identity is checked before and after.
Exact-case receipts explicitly cannot qualify the complete target. The four
commands share the same job target directory, so only the first catalogue needs
a cold build; later catalogues, cases and the full suite reuse those outputs.
The unchanged full baseline still runs after a focused failure unless the job
was cancelled. No ignore, retry, budget increase or full-suite waiver is added.

## Read-only failure context and issue mapping

The last-started generator case is unchanged from the bootstrap source. Its
helper in `src/conformance/tests/generators/mod.rs` compiles through the JIT,
passes an entry address to a spawned thread, calls it, then cleans runtime state.
The raw SIGSEGV record does not locate the fault within entry execution,
generator handling or cleanup. The helper also discards its `catch_unwind`
result; that cannot catch SIGSEGV and is a separate assertion-power concern.
No cause or fix is established by this static inspection, and no helper or
generator behavior is changed in this CI patch.

The fixture contract fails because the named AST fixture lacks a string `xfail`
entry in its `[tool.mamba]` record. This is a metadata-contract assertion, not a
CPython parity result. An oracle-backed policy decision is needed before any
fixture metadata edit; adding xfail merely to obtain green is not a remedy.

A read-only complete issue scan found no exact match for the crash case,
fixture basename or contract assertion. Existing context is:

* #26: Runtime tracking parent, with older generator-destruction crash evidence;
  that is a different case/source generation, not proof of this cause.
* #27: joint release acceptance; failed, missing and unrun evidence blocks it.
* #244: native 8,513/45/19 baseline; neither exact new failure name is in its
  recorded 45-case list. No unchanged Linux execution baseline is established.
* #246: only the published gather boundary, not this generator or metadata case.
* #5/#64: prepared-worker reuse/state isolation context, with no proven link to
  this native libtest JIT crash.

No duplicate issue, milestone assignment or runtime/xfail repair is created.
Planning must freeze the specific cause and original controls before broader
implementation. The old full-baseline and release requirements remain intact.
