# Q0 #163 Black reference audit

This record belongs to [#163](https://github.com/faberline/mamba/issues/163).
The human approved fresh hosted evidence for #160–164.
Each ticket still needs its own full review.
This ticket starts from `8142685fc300533156009c043368a11e38d383d6`.
At this audit's integration gate, #159–162 were closed. #164 had not started.

## Fixed source and execution scope

The original source archive is Black 26.10.0.
Its SHA-256 is `b3476ce71b494fc6e39d77e75488e8339a87583029bbf608416e955c83582bd6`.
The original oracle lock SHA-256 is `3ede6b08cf4fb8a1a12ec148169667e6b22d484b380f901a1bb040c8a2669a35`.
The profile SHA-256 is `59cd5c03b0ab25ba7f994bcd81fece95dd84c613f1e0ca4dc947573b7bcae2e7`.
The retained corpus lock SHA-256 is `14806dbcaa1bc65b2d1344341f89069f50908f2589811d6c2967a63a17536e2c`.

There are 4,172 ordered inputs: 260 upstream files and 3,912 custom cases.
There are 131 published chunks and no excluded cases.
The 235 cases with a reference error on at least one target stay in this count.
There are 4,131 `py` and 41 `pyi` inputs.
The reader uses input fields only.
It checks all upstream raw file hashes and prepared input identities.
The custom input bytes have SHA-256 `d9918c69d50d04e4cc801911a92be1893139909001468a8621eb0cf4febcfbd4`.
The leading flag prefix is the exact nine-byte string `# flags: `.
The space after the colon is required.
The full batch input has 2,036,273 bytes and SHA-256 `7c293118b1f493b4244049116fe6308cdebc4b1a3c161b6988cae3a1bdc2246e`.
The published catalog source inventory SHA-256 is `80112297058fed872122e5019f18536c955195b29f36d539b49a500d5b5b57a5`.

A separate reconstruction checks the frozen output content identity.
Its 8,701,905 bytes have SHA-256 `9adbd99901efc776a87117d3546eab85b4d75ad56bbd72cc8868d97533fa60da`.
This content check does not recover an old raw process record.

The host is standard GitHub `macos-15`, ARM64.
The exact Python patches are 3.12.11, 3.13.5 and 3.14.0.
The lock binds each interpreter archive and executable.
The seven runtime pins are Black 26.10.0, click 8.5.0, mypy_extensions 1.1.0,
packaging 26.3, pathspec 1.1.1, platformdirs 4.12.3 and pytokens 0.4.1.
The 3.12 target uses the original pure Python pytokens wheel.
The 3.13 and 3.14 targets use the original compiled pytokens wheels.
All three use the original compiled Black wheels.
These are fresh environments. They are not recovered old environments.

The original recipe uses three sequential API batch children.
Each child reads all 4,172 inputs from standard input.
Each child has a 300-second timeout and must have actual process exit 0.
This gives 12,516 case and target observations.
The per-row `reference_check_exit` values 0, 1 and 123 are computed values.
They are not per-row process exits.
The main child has argv `[venv_python, "-c", observer_source]`.
It inherits the controller's current directory and environment.
The controller records that fresh context.
The main child does not add `-I`, `-B` or `-X` flags.

The new observer follows the verified original public API recipe.
It is authored independently. It does not publish protected old WIP.
The original captured ORACLE literal is 1,858 bytes.
Its SHA-256 is `bce0cf03729d471e558c639554ad75043f62dcfb537d9d2841e9d6e4e3fb0171`.
The recipe audit SHA-256 is `bcfef6119ab9707a64aff543f8969d7536621ed59fb556839f6ba8784c8d4f19`.
The saved preflight source audit SHA-256 is
`a19ae7044d2c98c5092748ee1260257b41083b5e325aae18c4e19c54048a12ed`.
This exact 31,446-byte version is
`/Users/chrischeng/.codex/artifacts/mamba-q0-163-hosted-20261029/preflight/black-corpus-source-audit.json`.
The package audit SHA-256 is `881202f1e50b8dfb200a823ce4812ea82546b8000fb31a3ad692d9e16db8033e`.
The exact versions named above are also in
`/Users/chrischeng/.codex/artifacts/mamba-q0-163-hosted-20261029/preflight/`.
Its eight-file custody manifest SHA-256 is
`3fe91b6ceed1bfdf389a92c9b1ccefd2c37e5e3487a4d3d8fa32c1bf1bbebec8`.
Later addenda have separate identities. They do not replace these saved bytes.
The 33,088-byte updated source report is
`/private/tmp/mamba-q0-163-20261029-ezqnkxd8/black-corpus-source-audit.json`.
Its separate SHA-256 is `0226a5b2ed2f256b5c89a6e4125dd1ab4ae5bc44e6038daec7411c65dc4c996e`.
The exact flag-prefix addendum SHA-256 is
`bb05127e33193641675aad63a073525a95c96235f2feb8bb06251aa973791f5f`.
The corrected reader passes 21 local pure controls with no skips.
Its full input digest is unchanged.
Root repeated these controls and retained its own wait/exit receipt at
`/private/tmp/mamba-q0-163-20261029-ezqnkxd8/root-input-controls/receipt.json`.
Actual exit was 0. No original tool ran in these controls.

The fixed Mode uses the actual Python minor, line length 88, string normalization,
magic trailing commas, `preview=false`, `unstable=false` and the case's `pyi` extension.
Other upstream flags remain source metadata.
The observer parses a syntax hint. This does not filter inputs.
It decodes and formats each source with `fast=false`.
It restores the decoded line ending.
On `NothingChanged`, it keeps the original source bytes as text.
It decodes and formats the result again to record convergence.
Any exception from either stage becomes the original error record schema.
Raw success and error fields keep their original order.
No CLI config discovery is added.

## Review rules

Successful output must match the frozen output exactly.
An error must have the exact class and message.
The existing `ASTSafetyError` rule permits only the suffix after
`This diff might be helpful: ` to use another valid `blk_*.log` path.
Its preceding message must match exactly.
No other path, whitespace, error text or expected output is normalized.
Raw log files and their current canonical temp parent are separate evidence.
This audit does not infer that the separate safety test passed.
The observer's hashes and computed values are checked separately from the original comparator.
Every failed comparison stays failed.

The original native test also checks native repeated formatting and strict AST validation.
This hosted original-tool audit does not execute those native gates.
The separate 18 JSON manifests, 21 width rows and 190 embedded type-comment rows keep their own contracts.
The 174-cell safety API worker recipe is still missing.
Those gates are `not_run` or `unknown` here.
They are not added to the 4,172-case denominator.
The published corpus has no typed positive, negative, interaction, dormant or unsupported fields.
These roles are `unknown` for all 4,172 cases.
Names do not supply missing roles.

## Evidence result

| Item | State | Evidence |
|---|---|---|
| Source/API from #61 on the actual base | Verified by static inspection | 213 source files; no build or native claim |
| Original archive and wheel bytes | Verified by static inspection | All 20 downloaded assets and 19 wheel RECORDs |
| Original ordered inputs | Verified by static inspection | 260 upstream plus 3,912 custom; exact input digest |
| Capture helper controls | 66 passed, no skips | Root retained stdout, stderr and terminal receipt |
| Fresh hosted main batches | Complete | Run 40758572839, three actual exit-0 children |
| Independent full raw/index review | Verified | 12,516 observations; zero comparison or integrity failures |
| Fresh environment and package review | Verified | Three interpreters; 21 installed RECORDs; all original payloads match |
| Separate ancillary and native gates | not_run / unknown | Original contracts remain open |
| Bounded audit evidence | Verified | Final record and integration are checked separately |
| Full reference/native/parent/release acceptance | false | No such acceptance is granted by this audit |

Root independently rehashed all 213 named sources against the base Git tree.
All 213 matched both the frozen inventory and current checkout.
The result SHA-256 is `99ced0fa45d99f19e58f2dd033b78f6a0096ffeed72d213b4e61056a5a83ae79`.
Root also checked all 20 downloaded artifacts against the retained original oracle lock.
All 19 wheel RECORDs matched all 737 hashed payload rows.
The separate result SHA-256 is `cbf4c62a0d4b19eeeac65071505852dcc700287dbd9589a133e9dd9a88fb3328`.
Both reports are in `/private/tmp/mamba-q0-163-20261029-ezqnkxd8/`.
These static checks do not execute Black or Mamba.

Root ran the combined capture controls from the repository root:

```sh
Q0_BLACK_TEST_ARCHIVE=/private/tmp/mamba-q0-163-20261029-ezqnkxd8/static-downloads/black-26.10.0.tar.gz python3 -B -m unittest discover -s tools/ci -p 'test_q0_black*.py' -v
git diff --check origin/main...HEAD
```

Both commands exited 0.
The range-check receipt is in
`/private/tmp/mamba-q0-163-20261029-ezqnkxd8/root-candidate-source-checks/summary.json`.
The control child completed its wait with the same recorded PID.
Its raw streams and receipt are in
`/private/tmp/mamba-q0-163-20261029-ezqnkxd8/root-all-helper-controls/`.
These tests cover exact input parsing, the original comparison rule, wrong hashes,
missing logs, repeated log paths, inherited process context, timeouts, process exits,
unknown rows, package RECORD payloads and target continuation after failure.

## Hosted execution and independent review

[Run 40758572839, attempt 1](https://github.com/faberline/mamba/actions/runs/40758572839)
has GitHub status `completed` and conclusion `success`.
GitHub records `updated_at=2026-10-28T19:56:44Z`.
The executed head is `f4fe69cbdb549080b4937fb53a510fd871c1512f`.
Its tree is `7bc8ca23e408be0a54e30b70b0fd58c528864604`.
The host was macOS 15.7.9, ARM64, image `20260907.0337.1`.
All three main children used cwd `/Users/runner/work/mamba/mamba`.
Each read the same exact 2,036,273-byte input array.
Each used a fresh symlink venv under `receipts/run/python312`, `python313` or `python314`.
Its full argv is in the corresponding `main/receipt.json`.
The complete `-c` argument is also in that target's `observer-source.py`.
The receipt binds that source text and every raw stream by SHA-256.
All three stderr streams are empty.

| Python | Observations | Output records | Error records | Primary matches | PID / waited PID | Actual exit |
|---|---:|---:|---:|---:|---|---:|
| 3.12.11 | 4,172 | 3,937 | 235 | 4,172 | 1367 / 1367 | 0 |
| 3.13.5 | 4,172 | 3,975 | 197 | 4,172 | 1434 / 1434 | 0 |
| 3.14.0 | 4,172 | 3,983 | 189 | 4,172 | 1672 / 1672 | 0 |

The computed per-row states remain separate:

| Python | State 0 | State 1 | State 123 |
|---|---:|---:|---:|
| 3.12.11 | 1,284 | 2,653 | 235 |
| 3.13.5 | 1,284 | 2,691 | 197 |
| 3.14.0 | 1,284 | 2,699 | 189 |

All 12,516 primary comparisons match the frozen original values.
This includes 174 uses of the existing narrow ASTSafetyError log-path rule.
There are zero secondary metadata differences or record-integrity errors.
All 174 referenced log files were retained.
They contain 220,629 bytes across 13 distinct SHA-256 values.
Every log joins to its raw error, case and current canonical temp parent.
These are main-batch log observations. The separate safety API test is still `not_run`.

The hosted helper suite ran 66 tests: 65 passed and one was skipped.
The skipped test requires a local `Q0_BLACK_TEST_ARCHIVE` variable.
The formal capture separately ran the complete input builder against the pinned archive.
The independent reviewer also rebuilt all 4,172 inputs from that archive and public input fields.
This verifies the required full input gate. It does not label the skipped unit check as passed.
The local root control run executed all 66 tests with no skips.

The environment reviewer checked all three retained interpreter archives.
Their 5,554 tar members and 5,994 initial payload entries match the recorded identities.
File modes follow the actual safe tar data-filter rules.
The safe extractor removes unsafe permission bits.
All 21 setup child receipts have a matching waited PID and actual exit 0.
All 21 whole installed RECORDs cover 687 installed file facts.
All 597 original immutable wheel payloads match their exact locked wheel bytes.
This includes 96 native extension files from the reference packages.
The 69 generated files and 21 regenerated RECORD files are listed separately.
Each target has six active satisfied requirements and 20 inactive requirements.
No extra runtime package is required or silently added.
Extracted venv directories are excluded from the upload.
Their full RECORD bytes, installed file facts, module hashes and exact source wheels are retained.
This is fresh environment evidence, not recovery of an old venv directory.

Root matched all 195 hosted source-manifest files to the executed Git commit.
The source closure report SHA-256 is
`31980806be58bd8ce008f81e72c07543b37e397d7b352f34dff3a96350f42e96`.
The raw/index review SHA-256 is
`ea7aba6b0995850d211491610c295e63b30362e3c99e0ba59cc8fb3aac9c836f`.
The environment/package review SHA-256 is
`80f811818c5471365465d583ad32ef7734e94d7ecb2ce20ddc53965afca80f80`.
Each reviewer used independent standard-library reads of the actual artifacts.
The raw reviewer did not call the producer's input or comparison functions.

## Durable raw pointers

The complete hosted backup is
`/Users/chrischeng/.codex/artifacts/mamba-q0-163-hosted-20261029/run-40758572839-full/`.
Its custody manifest SHA-256 is
`e528aa8e257eb1cf2b29c3b241d029e56e6a3e9252b861a9f05818b49a919026`.
It binds 381 saved files, including all 373 uploaded files and the original downloaded ZIP.
The uploaded files total 143,109,641 bytes.
GitHub artifact `12846453379` is a 64,896,228-byte ZIP.
Its SHA-256 is `4d54a52b0d5aa7e4eb3e5323ddf72716dc4a2bc703cf9c90bcc8754e766cf539`.
Four artifact manifests cover 366 entries.
The remaining seven files are those manifests and three explicit top-level files.
Every uploaded file was re-read and matched. No file was dropped.

Paths below are relative to the backup's `artifact/run/` directory.
The complete index is `observation-index.jsonl`: 12,516 rows and 19,151,794 bytes.
Its SHA-256 is `88330ca38dc5deac1117b11da9865da4c0e7a5c25d44d1bd9071924b0650ddb9`.
Each row joins an ordered input ID and source hash to the exact frozen and actual array positions.
It also points to the shared batch receipt and saved log files.

| Raw stdout | Bytes | SHA-256 |
|---|---:|---|
| `python312/main/stdout.raw` | 1,583,662 | `03c1a872442395b85ce2fe0123b615ae4c18079a9c50ae5183fec59081bec375` |
| `python313/main/stdout.raw` | 1,581,216 | `7ebbfb847e1348664d22352d932429a0654f44f01987bf7a3d8f99311bd256b8` |
| `python314/main/stdout.raw` | 1,588,926 | `ccdcbef406f7c4e8aa187dbd0624366e64ca68207040e2619d2d6a0bb547fdcd` |

The full static candidate backup is
`/Users/chrischeng/.codex/artifacts/mamba-q0-163-hosted-20261029/candidate-f4fe69cbd/`.
Its 62-file custody manifest SHA-256 is
`28bda2f13c9dc34a7812678e304c72c61b78c3290f636323b736b943b557838d`.
The independent full-review backup is
`/Users/chrischeng/.codex/artifacts/mamba-q0-163-hosted-20261029/run-40758572839-reviews/`.
Its custody manifest binds the two full reviews, their checkers, controls and source closure.
The manifest SHA-256 is
`39dae5402c9684669f6639b2b2d0ee7f6aaac9055d78b26d10118280174d3a94`.
It also preserves the first checker draft. The final checker applies the actual tar data-filter modes.

The post-run record changes only this document.
The PR description and final source-closure receipt bind the exact record head.
They must confirm that all runtime source, workflow, lock, input and comparator identities
still match the executed head. This document does not claim a new execution at its own record head.

The original bounded audit Acceptance permits truthful failed and missing findings.
It does not turn them into functional, full-denominator or platform passes.
Only audit capture helpers, their controls, the workflow and this record may change.
Product source, expected data, protected QA and accepted earlier-ticket helpers must not change.
The final record must bind the executed head, source closure, actual commands and exits,
complete raw/index review, durable custody, PR head and merged tree.
