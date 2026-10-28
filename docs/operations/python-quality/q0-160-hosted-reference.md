# Q0 issue 160: isort reference locks and coverage

This work concerns only [issue 160](https://github.com/faberline/mamba/issues/160).
Issue 159 was accepted and closed before this work began.

## Fixed source and input scope

The source is isort 9.0.2 with its original source-archive SHA.
The three ABI-specific Linux wheels are pinned by the retained original lock.
Their declared runtime dependency is pinned to the original
`mypy_extensions 1.1.0` wheel. Inactive optional extras are not installed.
Python targets are 3.12.11, 3.13.5 and 3.14.0.
The profile remains Black with line length 88.

The preparation reads 59 upstream test files as AST data.
It does not execute the upstream tests or fixture programs.
The 288 synthetic input definitions contain only input fields already
published in the existing test corpus. No protected helper source or
expected result was copied to create inputs.

All 1,355 constructed input identities must match the existing published
inputs: 1,155 main cases and 200 separate excluded cases.
Every upstream test-source hash must match the original inventory.

## Required reference outcomes

For each target, the main cases retain 2 reference errors, 3 skipped cases,
36 nonconverging cases and 1,117 converging cases.
The three skip rows are included in the original 1,117 convergence flags.
Their actual skip exception is recorded; no second API call is invented.
An expected error or nonconvergence remains visible as that outcome.
A batch exit 0 proves completion of the batch, not success of each case.

The 200 excluded cases have separate API and CLI evidence.
Their exact original API and checker recipes were traced from retained
same-session records and their directly named predecessor.
The API uses the case root as its only source path, explicit settings path
and file path, and generic exception reporting.
The independent checker runs `python -I -B -m isort --profile black
--line-length 88 --python-version 312|313|314 --check-only <absolute main.py>`.
It preserves actual process exits: 136 zero and 64 one per target.

The three main API batches, 600 excluded API processes and 600 independent
checker processes must complete. Each process retains real argv, cwd,
input bytes/hash, PID, wait, exit, stdout and stderr.
The input files must remain unchanged after excluded API and checker calls.
Missing, skipped, timed-out or unrun required processes cannot pass.

## Evidence boundary

These are new hosted Linux results. They do not restore missing historical
corpus JSON, old environments or old process files.
The existing corpus, API outcomes, exceptions, syntax hints, bytes and
checker exits are compared without widening normalization.
The runner does not build or run Mamba or alter published expectations.

Issue 159's human-approved replacement basis applied only to issue 159.
Issue 160 keeps its own acceptance conditions until its current evidence
has been independently reviewed and any required decision is made.
No parent, whole-suite, platform, performance or release acceptance is claimed.

## Verification record

The exact run ID, source revision, artifact identity, actual checks and
controller decision are recorded on issue 160 and its associated pull request.
This source document describes the gate; it does not claim that a run or an
issue passed. Each result keeps the source revision that actually produced it.
