# Q0 issue 159: hosted Ruff reference evidence

The user requested GitHub-hosted execution on 2026-10-28.
This run concerns only [issue 159](https://github.com/faberline/mamba/issues/159).
The next Q0 issue must wait until this issue is accepted.

## Fixed scope

The workflow uses one Ubuntu 24.04 runner.
It runs Ruff 0.16.10 with Python 3.12.11, 3.13.5, and 3.14.0 in sequence.
It does not build or run Mamba.

The checked-in lock records the exact public source archive and Linux wheel.
The fixture archive is the `ruff_linter` crate pinned by Cargo.lock.
Each download must match its complete SHA-256 before use.
The current profile must match its existing SHA-256.

The fixture crate contains 590 upstream input files in the six original groups.
The preparation adds the six original baseline controls.
Fresh original diagnostics select the existing directive witnesses.
The preparation then adds the 15 original controls.
The result must contain exactly 784 files.

Input bytes come from the pinned upstream source and the fixed preparation rules.
They do not come from generated expected diagnostics or fixed output bytes.
The runner compares only the resulting file names and input SHA fields with
the published test inputs. All 784 input hashes must match.
A changed witness, missing file, extra file, or changed byte fails the job.

## Commands and raw evidence

For each target, the runner keeps the baseline reference command.
It then records `before`, `fix-1`, `fix-2`, `after`, and `clean`.
Safe fix commands use `--fix --no-unsafe-fixes`.
All checks use the original fixed select list and disable preview and cache.

Each command keeps argv, cwd, PID, wait result, actual exit, input hashes,
and the original stdout and stderr bytes.
The result keeps all 174 rule rows.
F621 stays dormant.
B911 and B912 keep their original target activation boundaries.
The clean control must exit 0.
Repeated fixes must preserve bytes and ordered diagnostics.
All three targets must finish before a successful execution result is possible.

The workflow uploads raw evidence on success and failure.
Artifact names include the exact source SHA, run ID, and attempt.
The workflow is triggered only by a push to `codex/q0-159-ruff-reference`.
It does not change the existing CI branches or release workflow.

## Evidence boundary

These are new Linux process and environment records.
They do not restore the old macOS Ruff binary, old virtual environments,
old corpus JSON, or old process outputs.
The old corpus digest is retained as a historical identity.
The new output must not be relabelled with that digest.

An execution pass is not issue acceptance.
The summary must retain `issue_accepted=false` and
`historical_original_evidence_recovered=false`.
The controller must inspect the real hosted results and each original
acceptance item before it can close issue 159.

## Current status

The hosted result is pending.
No product, platform, performance, parent issue, or release acceptance is claimed.
