# Historical mypy remaining ledger: 412 unresolved obligations

This document records the retained native output for issue #129. It does not report a current product pass. Every row in `remaining-412-ledger.json` stays `unresolved`.

## Source and result

- Original source HEAD: `fdcb0aa08ec0778ce63ec75d8b0b66f71083834a`.
- Full source content SHA-256: `fdc25f5b7fb135cbfcb15f5e24219db6b9e4e6c73b964b612e7668e5855c82f6`.
- Source map SHA-256: `e12a7e465e9ae6088be8e7a208cd158702dbfb9098b0add1a40cb6fbda62eac5`.
- CLI binary SHA-256: `9d2eb190cef74856e1816bdddea69009c5ddd199c595e988877907d12cffe255`.
- Original native stdout SHA-256: `0e0fbba56f0eac0872390dbb52dd463188c10bdf20dfbafc790f97a9b9bc4cf5` (6,659,522 bytes).
- Original command: the pinned CLI ran `quality check` on 217 exact Python paths with `--json`. Its command SHA-256 is `39effcb1874f89a6031b96bf72438fcf40da885ba2e44a2bb5ac3a4343ff5ed1`.
- Original process: `actual_wait_known=true`, exit `2`. The mypy result checked 217 files and retained 412 remaining strings. Completion was `incomplete`.

The original comparator recorded 4,034 reference rows and 3,969 native rows. Its unordered cause-only view found 72 missing and 7 extra rows. That view is for diagnosis; it does not replace the ordered nine-field comparison. `whole_som_accepted=false`.

## Ledger rule

Indices `0..411` follow the raw order. The `text` field keeps each original string. The raw schema has no separate reason field, so `reason` repeats that string and `reason_source` explains this limit. The count of each exact string in this raw is its `multiplicity`; all 412 strings occur once.

Index 0 is one global summary. Indices 1–3 and 411 are four capability rows. Indices 4–410 are 407 diagnosis markers. These groups do not mean 412 distinct bugs. The summary and capability rows remain until their full denominators pass. No row is marked fixed.

Each row names one bounded child issue from #130 through #158. The counts are:

| Child issue | Rows |
| --- | ---: |
| #130 | 1 |
| #131 | 1 |
| #132 | 1 |
| #133 | 1 |
| #134 | 24 |
| #135 | 4 |
| #136 | 121 |
| #137 | 29 |
| #138 | 16 |
| #139 | 2 |
| #140 | 2 |
| #141 | 32 |
| #142 | 22 |
| #143 | 5 |
| #144 | 3 |
| #145 | 3 |
| #146 | 12 |
| #147 | 21 |
| #148 | 20 |
| #149 | 9 |
| #150 | 1 |
| #151 | 3 |
| #152 | 6 |
| #153 | 1 |
| #154 | 1 |
| #155 | 58 |
| #156 | 7 |
| #157 | 5 |
| #158 | 1 |

## Local evidence and limits

The local native-review handoff has 24 retained files, including the exact stdout, stderr, command, process, source map, source proof, comparison, and prior reviews. Its manifest is:

`/Users/chrischeng/.codex/worktrees/4e1a/mamba/target/native-quality-controller-20261023/quality129-retained-handoff-1/manifest.json`

Manifest SHA-256: `4e241c368827ca8fe0f330370353089aa26e2220abfa42ead345f306fcf7973a`. The handoff is local. The raw bytes have not been uploaded for remote review.

This change is a document record only. Product unit and build tests are not applicable to this two-file document change. No new native, reference, Cargo, build, or CLI run supports it. The original native exit 2 remains a failure. This child record does not accept issue #43 or current-source Quality behavior.

## Document checks

The author ran pinned Python 3.14.6 with `-B` to compare all 412 rows with the retained raw and to check all 24 local handoff files by bytes, mode, and SHA-256. The command exited 0. The author also ran `python3.14 -B -m json.tool` on the ledger JSON. It exited 0. The exact commands and raw self-check output are in the task-local author report. These document checks do not run or accept the product.
