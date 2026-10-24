# GitHub-hosted CI bootstrap and lifecycle handoff

This draft establishes cloud test receipts for the compiler/runtime. It does
not qualify a release or enable automatic issue completion. No GCP, GKE,
laptop runner, new secret, paid runner, or access change is required.

## First-stage checks

`mamba-ci.yml` runs on every PR (including drafts) and manual dispatch, on
`ubuntu-24.04`, with at most two jobs per run, one Cargo build thread, one
libtest thread and one Rayon thread. PR updates cancel that PR's superseded
run. Manual runs are independent; operators should avoid overlapping dispatches.
Jobs have a 55-minute ceiling. The receipt wrapper separately times out each
inventory/execution process group and kills its descendants.

Actions use immutable SHAs; Rust is 1.97.0 and CPython is 3.12.11. Only Cargo
downloads are cached by OS/architecture/toolchain/lock hash; binaries are
built for the checked-out PR head. Logs, tool identities, exact argv, exits,
counts and hashes are retained as artifacts for 14 days, including failures.
Cancelled or missing artifacts block acceptance. Fetch artifacts before expiry
when they are needed for a durable acceptance record.
The receipt requires the declared CI head to equal the checked-out commit.
Recorded PIDs/exits identify the Cargo driver; nested libtest PIDs and executable
hashes are not captured by this bootstrap and remain qualification follow-up.

The two suites are deliberately bounded:

| Suite | Actual command selection | Meaning |
| --- | --- | --- |
| library-baseline | `cargo test --locked -j 1 -p mamba --lib` | Full unfiltered library inventory and execution; preserves inherited failures |
| contracts | `--test perf_pin_gate_policy`, then `--test conformance_contract` | Existing perf baseline policy and fixture/harness contract assertions |

Each selection first executes `-- --list`, then `-- --test-threads=1`.
The receipt rejects zero/duplicate inventory, missing/substituted verdicts,
missing summary, filtered/measured results, failures, and ignored selected
cases. Ignored cases remain separately counted; they never count as passes.
A failing listing blocks execution. Crash/timeout results remain incomplete,
with unobserved cases counted as not run. A failed build is not a test failure
count: its cases remain unknown until inventory succeeds.

There are no failure exemptions, `continue-on-error`, retries or test assertion
changes. #244's historical unchanged-main result is 8513 passed, 45 failed,
19 ignored, exit 101 on a native Mac. A new pinned Linux run is new evidence;
its counts may differ and cannot replace that record. #246 concerns one gather
case and does not waive the other failures. No product source changes belong
in this bootstrap.

## Required qualification follow-up (currently NOT RUN)

`Cargo.toml` has `autotests = false`; its explicit targets are authoritative.
`docs/MVP_TEST_PROFILE.md` and `.cargo/config.toml` contain historical names;
do not copy their commands without checking the current manifest. The workflow
stores `cargo metadata --no-deps` so each run has its actual target inventory.

Before selecting affected PR checks, define a reviewed path-to-required-target
contract. Unknown paths or changes to the contract/harness require the full
applicable matrix; a small focused pass cannot stand in for that matrix.
This first draft always runs the two selections above and makes no affected
coverage or complete PR qualification claim.

The full contract still needs:

* Actual semantic/type-wall/runtime/real-oracle evidence from `conformance`,
  `conformance_pipeline`, `conformance_cpython_grammar`,
  `conformance_cpython_lib_test`, `conformance_runtime_shutdown`, and
  `conformance_real_world`. Preserve forced typing/free threading declared
  divergences and assertion-driven outcomes; parser success is not runtime
  or real-oracle success. Freeze fixtures, oracle packages and interpreter
  identity for each run. CPython 3.12.11 here is a deliberate new environment,
  not a replay of the historical oracle 3.14.6/build 3.12.11 record.
* `pkgmgr`, the applicable explicit `pkgmgr_*` E2E targets, `mambalibs`, and
  `mambalibs_array_wheel`, `mambalibs_sci_wheel`, `mambalibs_plot_wheel`.
  Wheel gates must actually build/install/import the native artifact.
  No-network and opt-in network contracts must remain distinct.
* Native matching runners for platform contracts. Apple Silicon and APFS
  evidence requires that actual host/filesystem; Linux or cross compilation
  proves neither. Runner availability/capacity must be checked using existing
  account capabilities before adding a job.
* A release-grade CLI build with `cargo build --release --locked -p mamba
  --bin mamba --features release`. The existing `mamba-release.yml` builds
  packages without tests and omits `--features release`; it is not a gate.
  No release workflow or publishing behavior changes in this draft.
  A completed milestone makes a release candidate eligible. The future release
  workflow must run the full applicable suite on matching GitHub-hosted runners
  and publish only after every required full gate passes. Preserve failed run
  links and repair follow-up; milestone completion never waives inherited
  failures. No tag, publishing, or automatic release is enabled here.
* Performance uses explicit opt-in targets `perf_pin` and `perf_gate_report`.
  Do not use `-- perf_pin`: generated cases are `run_pin::<filename>.toml`
  and that old filter may execute zero cases. Inventory must be nonzero,
  match required pins/fixture hashes, and prove each required metric graded.
  Use the ship release profile without LTO/codegen overrides, serialize heavy
  perf work, and measure a fresh CPython baseline in the same job/host.
  Never reuse a Mac baseline on Linux. Retain per-pin thresholds, samples,
  raw and fixed-floor-adjusted RSS, host/tool/source identities and raw logs.
* **Performance qualification is blocked:** `perf_pin.rs` currently prints
  skips for unavailable CPU/RSS even when baseline-required mode is enabled.
  Fix that in a focused reviewed change with missing-metric controls before
  accepting a performance pass. Existing process timeouts and process-group
  cleanup must be preserved. Negative controls for missing/wrong-host
  baseline, fixture hash mismatch, zero cases, timeout and assertion failure
  must all fail qualification. No skipped metric is a pass.
* Original historical Quality/native evidence requirements remain in force;
  CI receipts are supporting evidence, not a replacement or blanket waiver.

## Disabled integration contract

Reconcile the existing central AW lifecycle contracts and native-owner issue
handoff before activating Mamba agents; some existing tickets explicitly say
No AGY. Reuse applicable tracker/PR/head/evidence contracts, never app GKE
deployment policy. No schedule or old controller is started by this PR.

Read-only inspection of central `faberline/aw` at local source commit
`3e403aee31091eb8577db5a25afec0944daabff3` found reusable issue ownership,
per-issue PR and exact-reviewed-head checks in `issue-work/scripts/work.sh`
and `issue-review/scripts/review.sh`. Its `work.sh finish` currently relabels
`doing` to `done` immediately after pushing/opening a PR; `review.sh next`
selects `done` issues and `pass` merges. Those paths cannot run unchanged for
this handoff. A central adapter must separate PR publication from qualification
and disable automatic done/merge, rather than copying those scripts into
Mamba. Their default 10/15/15-minute loop is not the requested review cadence.

Onboarding found issue #240 without a milestone: it replaces the test-only
live-PyPI AC6 placeholder with the original measured resolver gate. Claude
planning should assess it against `L1 — Test and Mambalibs backlog`; that is a
candidate, not an assignment or an arbitrary release version. No milestone is
changed here.

The intended handoff is Claude plan → AGY todo-to-doing/PR → CI → Codex review.
Milestones are assigned only by Claude planning. CI failure returns work to
todo after ownership reconciliation. Codex review requires all required CI
for the **latest exact PR head** to pass, runs hourly/sequentially with at
most five Sol reviews, and rejects stale/cancelled/missing evidence. These
are deployment requirements, not active automation in this draft.

Success-to-done stays disabled until the complete required qualification
contract above is implemented and verified. Next deployment needs: a reviewed
affected/release/platform/performance evidence manifest, verified cloud
receipts, baseline blocker resolution, central AW/native-owner reconciliation,
and an explicit later activation decision. No merge, branch protection,
credential/permission, subscription/quota, or schedule changes are included.
