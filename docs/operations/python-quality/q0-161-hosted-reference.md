# Q0 issue 161: mypy reference locks and coverage

This gate concerns only [issue 161](https://github.com/faberline/mamba/issues/161).
Issue 160 was accepted, merged and closed before this work began.
The user approved fresh hosted evidence for issues 160 through 164.
Each issue still needs full independent review before the next issue starts.

## Fixed inputs and packages

The original mypy version is 2.4.0. The Pydantic plugin version is 2.13.5.
Python targets remain 3.12.11, 3.13.5 and 3.14.0.
All ten runtime packages use exact versions and artifact hashes from the
retained original oracle lock. Installation is offline from verified wheels.
Active requirements must satisfy those pins. Optional extras stay inactive.

The fixed corpus contains 22,898 distinct named inputs.
It contains 18,822 `.py` files and 4,076 `.pyi` files.
There are 21,652 unique source hashes. Duplicate source text remains present
under its original case name and filename.

Input construction reads only the already-published input fields.
It does not copy a protected helper or use expected results to create inputs.
The original field order matters. Twelve rows place extension before source.
The input-only JSON reproduces the original 6,335,487 bytes and SHA-256
`34a18754668b79baa71a16314d4eac07209c9a03111850546a78cc29ff711637`.
The gate also pins all 729 published catalog source files.

The pinned source archive contains upstream tests. Those tests are not this
custom corpus. Reading their bytes does not claim that they were executed.

## Required execution and comparison

Each Python target gets one fresh project and one complete mypy CLI batch.
All 22,898 files appear in their original order in the actual command.
The batch uses the retained strict flags and Pydantic plugin configuration.
It uses `--cache-dir=/dev/null`, `--no-incremental` and `--output=json`.
The original process timeout is 300 seconds. Actual exits 0 and 1 are allowed.
The runner records the actual PID, completed wait, exit and raw streams.
A batch exit is not assigned to individual cases.
All source files and configuration must remain unchanged after the call.

Every stdout line is parsed as JSON from the original bytes.
No diagnostic, note or status line is silently discarded.
The only projection removes the exact absolute project prefix from a path
and adds one to the start column. The original end column stays unchanged.
Messages, hints, severity, rule, end location and per-file order stay exact.
There is no message, address, path or diagnostic-order normalization.

All 68,694 case/target cells must compare against the existing expectations.
Empty diagnostic lists remain in that denominator.
The expected diagnostic counts are 25,951, 25,811 and 25,811.
Each target retains twelve negative and nine zero end columns.
Unknown files, missing rows, altered sources, incomplete waits, timeouts or
comparison differences fail the execution gate.

The retained recipe has no explicit platform option. The hosted Linux job
uses the same host-default behavior. A platform difference remains a failure;
the gate does not hide it by changing expected results or normalization.

## Evidence and acceptance boundary

The artifact retains exact source/host identity, downloads, environment
probes, full input bytes, source inventories, raw command records and every
case comparison. Fresh venv directories are excluded. Installed payload
identity uses complete probe hashes and pinned wheel bytes. Pip-rewritten
RECORD files stay separate from immutable wheel-payload comparisons.

New hosted records do not recover old corpus JSON, old venvs or old process
files. The user approved this evidence basis for the bounded lock/index audit.
The historical file-identity requirements of issues 16, 228 and 229 remain.

Original rows without category labels remain unknown.
Supplementary matrices and the separate project corpus have their own
contracts. They remain not_run unless those contracts actually execute.
The native product gate still declares incomplete behavior. This hosted job
does not build or run Mamba and does not accept those native obligations.
Parent 44, full-suite, platform, performance and release acceptance stay separate.

The exact candidate revision, run, artifact, actual results and independent
controller decision are recorded on issue 161 and its pull request.
This source document defines the gate. It does not claim a successful run.
