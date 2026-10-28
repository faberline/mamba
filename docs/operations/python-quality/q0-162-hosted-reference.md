# Q0 issue 162: Pylint reference locks and coverage

This bounded lock and coverage audit concerns only [issue 162](https://github.com/faberline/mamba/issues/162).
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
actual exit and raw stdout/stderr. The actual exit is retained and compared with the frozen
case/target exit. A comparison passes only when those exits are equal. Usage errors, timeouts, missing processes and altered
fixture bytes fail. Failed cases do not silently stop the other assigned cases.

The collector keeps the complete project inventory before and after each run.
Every declared fixture must remain present with its exact original bytes.
An added file is allowed only at the CPython cache path of a declared `.py`
source. Its cache tag and four-byte magic must match the probed target.
The collector records `fixture_files_unchanged`, `project_tree_unchanged`
and `generated_cache_files` separately. It rechecks files and cache metadata
after all assigned cases finish. Cache bytes remain in the artifact.
Other added files fail. No bytecode-suppression option is added to Pylint.

A shard comparison pass is only a partition result. Complete audit evidence
requires all twelve artifacts from the same executed source/run and an
independent full join. Every case on every target must have complete verified
source, input, process and raw-output records exactly once. Missing, duplicate,
unknown or not_run required execution records block that evidence claim.
Comparison failures remain failed and are reported separately from evidence
completeness. They do not become full-reference, native or parent acceptance.

## Original comparison rules

JSON2 messages retain their received order. Rule, text, type, relative path
and exact locations are preserved. Columns remain zero based.
The canonical corpus has 43,803, 43,823 and 43,839 diagnostics.
Actual totals are reported separately because an already-approved whole-list
observation can differ. A case passes comparison only with an exact allowed whole list.

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
Its exact-patch probe and offline nine-wheel install stay intact.
The bootstrap does not execute a fixture or change the reference recipe.

Run 40733556126 then verified all three archive-bound interpreter identities.
Each target failed before any case ran: the copied venv's ensurepip child
aborted. Static Mach-O records load libpython relative to the executable.
Copying that executable into the venv loses the archive's library layout.
The retained child stderr does not include the inner loader message, so
this is a linkage inference pending successful hosted execution.
The replacement creates the fresh venv with `--symlinks` and requires its
interpreter link to resolve to the exact pinned base. Copies, wrong targets
and dangling links fail. Both resolved paths and executable hashes remain
recorded. The per-case Pylint argv, inputs and comparison rules do not change.

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
The original ticket accepts a truthful bounded lock/coverage audit. The
strict comparison rules above still decide each row. Audit completion does
not make failed comparisons pass or accept the parent/native/whole-suite gates.

## Completed hosted audit evidence

Executed source: `ee10c8ab97d0484b2f34a609e65fcfbe8a374e35`.
Executed tree: `c291ae82b33cfb0a42ee4bbb613639c64b93692b`.
[Run 40735095012](https://github.com/faberline/mamba/actions/runs/40735095012),
attempt 1, completed with a failed strict comparison result.
All twelve jobs and all 59,241 original-tool child processes completed.
Two controller-run audits independently read the retained raw evidence.
Every required input, source, process and raw-output record is complete.
There are no missing or duplicate case/target tuples, raw-evidence errors,
or inconsistent producer claims. All fixture bytes remain unchanged.

| Python | Complete raw cases | Exact allowed matches | Failed comparisons | Actual diagnostics | Canonical diagnostics |
| --- | ---: | ---: | ---: | ---: | ---: |
| 3.12.11 | 19,747 | 19,739 | 8 | 43,802 | 43,803 |
| 3.13.5 | 19,747 | 19,741 | 6 | 43,822 | 43,823 |
| 3.14.0 | 19,747 | 19,744 | 3 | 43,839 | 43,839 |

The 59,224 matches consist of 59,207 canonical matches and 17 matches to
existing approved whole-list observations. The other 17 cells remain failed.
The one-diagnostic deficit on each of Python 3.12 and 3.13 is in failed
comparison rows. It is not explained away as an approved variant.

All twelve bootstrap archives, initial payload inventories and isolated
identity probes were checked. All 108 installed distribution RECORDs
were reconstructed from exact wheels and the pinned pip implementation.
Each target has eight active dependency constraints and ten inactive
requirements. All nine mandatory package identities and the 383 default
rule IDs match. Missing packages, altered payloads and script exceptions
were not accepted.

The first auditor retains its failed strict result. Its matched-cell
histograms exclude the 17 failed cells. The separate raw-evidence join
covers every one of the 59,241 cells, including those failures.
Both audit processes completed and were waited for; each exited 1
because strict comparison did not fully pass. Their receipts and full
stdout/stderr remain retained. Wrapper completion is not a comparison pass.

## Preserved strict failures

- Six W1303 cases swap complete diagnostics in the outer ordered list.
  Their message text, locations and other fields stay exact.
- One E1304 case swaps complete missing-key diagnostics in that list.
- Four R0801 cases select a differently indented source fragment.
  Headers, all other fields, other ordered objects and actual exits stay exact.
- Six Python 3.12/3.13 cases resolve the PBS builtin `resource` before
  the unchanged local source module. E0611 replaces W0237/W0221, and
  the actual exit is 10 instead of the frozen 12.

Pinned Pylint source emits the format-key diagnostics directly from sets.
Its similarity checker hashes LineSet objects by identity and uses the
last visited member for the displayed fragment. Exact fragment slices
match the unchanged inputs. No object identities or set traversal trace
was captured, so the source supports this mechanism without claiming
a recovered historical traversal.

Each pinned PBS archive independently marks `resource` static.
Pinned astroid resolves a builtin before file-system source paths.
Current original-location pyenv configurations mark that module shared.
Those adjacent configuration reads are current corroboration, not
recovered historical process evidence. The retained Python 3.14 executable
matches the pinned PBS executable. Its current original-location
configuration is static. The frozen 3.14 signature rows already contain
E0611 and exit 10. The shared-to-static transition is therefore assigned
only to Python 3.12/3.13.

All 17 failures keep their complete actual/frozen objects, source hashes,
raw pointers, real waits and exits. No sorting, new normalizer, new
observation, combined variant or expected-result change makes them pass.

## Complete raw index and artifact identities

The controller index has 59,241 unique rows in target/global-index order.
Each row names the artifact, input, project, command, result, PID, wait,
actual exit, raw hashes and independent comparison result. Failed rows
also point to the independent mismatch ledger.
Index: 149,672,624 bytes; SHA-256 `8bd6ab89f80e1392499f56406933136b7a7b5adafc51eeb92e346f3953e5ff1c`.

Inside each artifact, raw command paths use
`run/python3MINOR/commands/case_GLOBALINDEX/`.
Global indices use a minimum width of four digits. Projects use
`run/python3MINOR/case_GLOBALINDEX/`; results use
`run/python3MINOR/results/case_GLOBALINDEX.json`.
The artifact is selected by the exact target and `GLOBALINDEX % 4`.

| Python / shard | Artifact | ZIP SHA-256 |
| --- | --- | --- |
| 3.12.11 / 0 | [12835934719](https://github.com/faberline/mamba/actions/runs/40735095012/artifacts/12835934719) | `3c8aa40e10dfc7b0929f8fb851845c9a9df4a454b0e5fe1a41dc370a1cfeeda9` |
| 3.12.11 / 1 | [12837173625](https://github.com/faberline/mamba/actions/runs/40735095012/artifacts/12837173625) | `c3e6a9807fa6582c3bc4a841f3fbf988ecc6e58cac9724fd6f30b635df533301` |
| 3.12.11 / 2 | [12839642673](https://github.com/faberline/mamba/actions/runs/40735095012/artifacts/12839642673) | `d988d97f6155423db0ea6eca6799e9aca33694de9908cab6192a6761f6009ea0` |
| 3.12.11 / 3 | [12839920014](https://github.com/faberline/mamba/actions/runs/40735095012/artifacts/12839920014) | `3995f45078f8b708ee2075541bf92cd7fad2f5d6f50d77a62dac93bc9b71985d` |
| 3.13.5 / 0 | [12839249407](https://github.com/faberline/mamba/actions/runs/40735095012/artifacts/12839249407) | `9904de4b89703945806d82d76b1838789b3c98a4033dbbde6a75e5fee916550b` |
| 3.13.5 / 1 | [12839853302](https://github.com/faberline/mamba/actions/runs/40735095012/artifacts/12839853302) | `fa438de6d308efd29cb3927770cf39c64bb378d3a0ab740b95d75826c5b1e27b` |
| 3.13.5 / 2 | [12838816098](https://github.com/faberline/mamba/actions/runs/40735095012/artifacts/12838816098) | `2bdc2184de8c82daf0814ffe14cda42ed779cc053c38505e7c05170e8ca5f5d4` |
| 3.13.5 / 3 | [12840355147](https://github.com/faberline/mamba/actions/runs/40735095012/artifacts/12840355147) | `4bd7cc6f9d8816061fde390ff85ba6a12d30ba510d2bf7aebcb3c97b6ffc9d3a` |
| 3.14.0 / 0 | [12837578263](https://github.com/faberline/mamba/actions/runs/40735095012/artifacts/12837578263) | `ea3583f4dcb35eb206a0990bed5ee906c1bf13c77093975820624ee1b55c17c5` |
| 3.14.0 / 1 | [12839616980](https://github.com/faberline/mamba/actions/runs/40735095012/artifacts/12839616980) | `4cd421ef95063eedd98c408e107c3c1e269dceba436cfabba772811d7cd39525` |
| 3.14.0 / 2 | [12839651348](https://github.com/faberline/mamba/actions/runs/40735095012/artifacts/12839651348) | `9e4ddb266fe30ca2b51361738bf2b6ef762e2cb0fb6a7491d578ffb41926d360` |
| 3.14.0 / 3 | [12837717641](https://github.com/faberline/mamba/actions/runs/40735095012/artifacts/12837717641) | `f4e07cc0f5d91a7ad1cf36837e7ba15fb359fa059752f5183060dcad3ac19868` |

The twelve ZIPs total 519,750,515 bytes.
Every GitHub ZIP digest, retained manifest entry and required file was checked.
The controller retains durable verified copies outside Git.

| Controller record | SHA-256 |
| --- | --- |
| Official metadata | `6603dbee97f579a0076ee5509c34389f38f82fafe24d2fc38ab6f2e55a5b32f8` |
| Complete custody and strict audit | `3eb5a48efcb32bf9c64129584bfe6b9cecad182feaafde52a60366afe282869a` |
| Independent raw and diagnostic audit | `ddb808816591008ae8ad57d73661952b15c51d75a3f83cab62fc6d222204c8ae` |
| Complete index custody | `5f9d8126971a27d629ee37feb45463d53df14b42237db00ad7865f67a320b242` |
| Failure source audit | `ab0bfee2f89643d164cbe18b7b70e60e15132b55bd890b6aa233fb5e7cba8268` |

The bounded lock/coverage audit is complete. Strict reference comparison
remains failed. Native, full-suite, parent and release gates are not
accepted by this record. The unobserved/unknown and supplementary
not_run rows above keep their original status. Historical missing
files and issues 16, 228 and 229 retain their original requirements.

## Failed-cell index

| Python | Global index / case | Actual / frozen exit | Preserved difference |
| --- | --- | --- | --- |
| 3.12.11 | 1186 / `strings_percent_22_08` | 22 / 22 | E1304 list order |
| 3.12.11 | 1476 / `strings_format_27_02` | 20 / 20 | W1303 list order |
| 3.12.11 | 3899 / `signature1_project_0` | 10 / 12 | Resource resolution |
| 3.12.11 | 3900 / `signature1_project_1` | 10 / 12 | Resource resolution |
| 3.12.11 | 3901 / `signature1_project_2` | 10 / 12 | Resource resolution |
| 3.12.11 | 14402 / `nesting2_multifile_0_1_6` | 8 / 8 | R0801 fragment indentation |
| 3.12.11 | 14405 / `nesting2_multifile_1_0_6` | 8 / 8 | R0801 fragment indentation |
| 3.12.11 | 14408 / `nesting2_multifile_1_1_6` | 8 / 8 | R0801 fragment indentation |
| 3.13.5 | 1476 / `strings_format_27_02` | 20 / 20 | W1303 list order |
| 3.13.5 | 1477 / `strings_format_27_03` | 20 / 20 | W1303 list order |
| 3.13.5 | 3899 / `signature1_project_0` | 10 / 12 | Resource resolution |
| 3.13.5 | 3900 / `signature1_project_1` | 10 / 12 | Resource resolution |
| 3.13.5 | 3901 / `signature1_project_2` | 10 / 12 | Resource resolution |
| 3.13.5 | 14458 / `similarity1_padded_indentation` | 12 / 12 | R0801 fragment indentation |
| 3.14.0 | 1474 / `strings_format_27_00` | 20 / 20 | W1303 list order |
| 3.14.0 | 1475 / `strings_format_27_01` | 20 / 20 | W1303 list order |
| 3.14.0 | 1477 / `strings_format_27_03` | 20 / 20 | W1303 list order |

## Recorded audit limits

The 120-second case timeout is bound by the executed runner source and
run identity. The common child receipt does not store that controller
argument. Every required child has a completed wait and no timeout.

Directory and symlink permission bits are retained but depend on the
host permission default. Regular-file payloads and modes, and symlink
targets, are checked against the exact base archives.

Live venv directories are excluded. Link evidence is the executed
fail-closed guard, recorded resolved paths and base/venv executable
hashes. A separate post-run file-system inspection of that live venv
is not available. No old venv, historical process file or full old
environment is claimed recovered.

The independent final evidence review found no blocking defect for
this bounded audit. Its SHA-256 is
`21d5def51d334b2a48e5733bcb2ebe7dded0b74118a5733cb89db215b3bb75ff`.
It retains all 17 failures and all separate parent/native gates.
