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

Local validation is limited to static checks, 19 receipt controls and a tiny
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
