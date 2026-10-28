# Q0 issue 162: Pylint reference locks and coverage

This gate concerns only [issue 162](https://github.com/faberline/mamba/issues/162).
Issue 161 was accepted, merged and closed before this work began.
The user approved fresh hosted evidence for issues 160 through 164.
Each issue still needs complete independent review before the next starts.

## Fixed sources and inputs

Pylint remains 4.1.2 and astroid remains 4.3.3.
Python targets remain 3.12.11, 3.13.5 and 3.14.0.
The current reconstruction uses standard macOS 15 ARM64 hosted runners.
Retained interpreter hashes and Mach-O headers identify the original platform
family. The original Mac ARM64 isort wheels are pinned for all three targets.
This does not recover the old venvs or assert their old OS release.
Exact CPython archives are pinned to official immutable
`astral-sh/python-build-standalone` releases and published checksums.
Python 3.12.11 and 3.13.5 use release 20250612 `install_only` archives.
These are new builds of the exact patches; their executables differ from
the historical pyenv builds. Python 3.14.0 uses release 20251031
`install_only_stripped`; its executable hash matches the retained uv build.
All three still need actual hosted execution and exact identity probes.
Eight core runtime packages and the fixture-import dependency
`typing_extensions 4.16.0` use the retained original oracle versions and hashes.
The fixture dependency is required by 63 fixed source files, including Final
decorator cases. It is not an active dependency of Pylint on these Python versions.
All nine packages are installed from verified wheels offline.
Active requirements must satisfy those pins. Optional extras stay inactive.

The public corpus has 19,747 distinct named cases and 20,651 fixture files.
It includes 20,500 Python entry files and all declared non-Python files.
Those extra files include 147 `.so` files, one `.pyd`, one `.dylib`, one
`.txt` and one `.pyi`. Their exact bytes remain part of the input.

Input-only `name` and `files` fields reproduce the original 5,911,082-byte
JSON and SHA-256 `56bd6674a569c788470395ecb2af55419d655f2a7906397a460d39c153990a17`.
Preparation follows the explicit GROUPS and CHUNKS reference order.
Module declarations can have a different lexical order at part 1000.
The gate pins all 1,314 public catalog source files.
It does not copy a protected helper or use expected results to create inputs.

## Required execution

The full reference denominator is 59,241 actual child processes.
Each case gets its own fresh Python/Pylint process on each target.
The original command uses isolated Python, `--rcfile=/dev/null`,
`--persistent=n`, `--max-line-length=88`, the fixed target version and
`--output-format=json2`, followed by sorted relative `.py` filenames.
All declared files are materialized. No extra pyproject file is added.

Twelve hosted jobs partition the existing inputs across three targets
and four execution shards. A shard owns global indices with index modulo
four equal to its shard number. Global case indices are not renumbered.
Each job uses the original four dispatch threads and 120-second case timeout.
Each thread launches a new process. No long-lived Pylint API replaces it.
This partition adds no source case, version, configuration or semantic axis.

Every child retains complete argv, cwd, input hashes, PID, completed wait,
actual exit and raw stdout/stderr. The actual exit must equal the frozen
case/target exit. Usage errors, timeouts, missing processes and altered
fixture bytes fail. Failed cases do not silently stop the other assigned cases.

The collector keeps the complete project inventory before and after each run.
Every declared fixture must remain present with its exact original bytes.
An added file is allowed only at the CPython cache path of a declared `.py`
source. Its cache tag and four-byte magic must match the probed target.
The collector records `fixture_files_unchanged`, `project_tree_unchanged`
and `generated_cache_files` separately. It rechecks files and cache metadata
after all assigned cases finish. Cache bytes remain in the artifact.
Other added files fail. No bytecode-suppression option is added to Pylint.

A shard pass is only a partition result. Final acceptance requires all
twelve artifacts from the same candidate/run and an independent full join.
The join must find every case on every target exactly once, with no missing,
duplicate, failed, unknown or not_run required process.

## Original comparison rules

JSON2 messages retain their received order. Rule, text, type, relative path
and exact locations are preserved. Columns remain zero based.
The canonical corpus has 43,803, 43,823 and 43,839 diagnostics.
Actual totals are reported separately because an already-approved whole-list
observation can differ. Each case still requires an exact allowed whole list.

Only the two existing message normalizers apply. F0010 permits the exact
encoding-error path template. F0002 permits the exact fatal template with
matching source paths and only cache-directory/timestamp replacement.
No diagnostic sorting, location change, address or OS normalization is added.

The unchanged variant fixture has 29 case/target rows and 87 approved whole
observations. Its name, target, source hashes and actual/frozen exit must match.
Complete ordered lists are compared. Individual observations are not combined.
The fixture and its original approval SHA are pinned.

Three F0002 crash cases and two F0010 cases per target remain fatal outcomes.
They are not relabeled as successful Pylint executions.
The original recipe has no platform adjustment. Differences remain failures
unless the existing precise comparator already permits them.

## First hosted Linux evidence

Run 40723996421 used source `f0cc2ac939dec78a60887aad6e60d92da9a95f5f`.
All twelve artifacts are retained with their GitHub ZIP hashes.
The run completed and waited for all 59,241 child processes.
The original shard gates reported 59,192 matches and 49 failed cells.
The run remains failed and is not reused as current acceptance evidence.

Observed differences include I0011 registration order, R0801 fragment text,
W1303 order and platform-specific extension behavior. The pinned Pylint source
uses unsorted directory enumeration and object-identity set iteration in the
relevant paths. Matching the original OS family does not guarantee a match.
The complete matrix must run again under the unchanged comparison rules.

Nine project inventories also gained interpreter-generated resource bytecode.
Their declared input files were neither removed nor changed. Generated cache
files are runtime output, distinct from the original fixture bytes. Their full
bytes and metadata remain visible; input immutability still needs verification.

## First hosted Mac setup failure

Run 40731426172 used source `e28141dabbc12ec52cc17baca06a79b64e85bfd5`.
The standard setup action could not supply Python 3.12.11 ARM64 on the host.
The controller requested cancellation because that run could not complete
the required three-target matrix. Cancelled and skipped work is not passed.
Its setup failure and available partial evidence remain retained.

The replacement bootstrap downloads each fixed official archive on the
hosted Mac, checks its size and SHA, and checks every archive member.
It extracts only into a fresh temporary prefix. It retains the archive,
full initial payload hashes, links and modes, and a real isolated identity
probe with PID, wait and exit. A separate manifest covers this evidence.
Signed redirect queries are omitted from receipts.
The existing runner receives the exact interpreter path through `--python`.
Its exact-patch probe, copied venv and offline nine-wheel install stay intact.
The bootstrap does not execute a fixture or change the reference recipe.

## Coverage and evidence boundary

The original profile has 383 default-enabled codes. Canonical results observe
359 of them and six additional inline-enabled information codes.
The 24 unobserved default codes remain unobserved/unknown. They are not
declared dormant. Other missing semantic categories remain unknown.
The 24 supplementary replay contracts remain not_run under their own gates.
The main variant index is comparator data, not another executed matrix.

Artifacts retain source/host identity, pinned downloads, package probes,
inputs, all case projects, raw commands/results and per-file hashes.
Only fresh venv directories are excluded. Installed payload identity uses
retained probe hashes and pinned wheel bytes. Pip-rewritten RECORD entries
and installed-only metadata/scripts remain separately visible.

New hosted evidence does not recover historical corpus JSON, venvs or raw
process files. Issues 16, 228 and 229 keep their original-file requirements.
This job does not build or run Mamba. Parent 44, native behavior, full-suite,
platform, performance and release acceptance remain separate.

Exact source revision, run, artifact identities, actual results and the final
independent controller decision are recorded on issue 162 and its pull request.
This document defines the gate. It does not claim a successful run.
