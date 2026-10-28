# Q0 #163 Black reference audit

This record belongs to [#163](https://github.com/faberline/mamba/issues/163).
The human approved fresh hosted evidence for #160–164.
Each ticket still needs its own full review.
This ticket starts from `8142685fc300533156009c043368a11e38d383d6`.
#159–162 are closed. #164 has not started.

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

## Current evidence state

| Item | State | Evidence |
|---|---|---|
| Source/API from #61 on the actual base | Verified by static inspection | 213 source files; no build or native claim |
| Original archive and wheel bytes | Verified by static inspection | All 20 downloaded assets and 19 wheel RECORDs |
| Original ordered inputs | Verified by static inspection | 260 upstream plus 3,912 custom; exact input digest |
| Capture helper controls | 66 passed, no skips | Root retained stdout, stderr and terminal receipt |
| Fresh hosted main batches | not_run | Run and commit pointers pending |
| Independent full raw/index review | not_run | Pending hosted artifacts |
| Separate ancillary and native gates | not_run / unknown | Original contracts remain open |
| Bounded audit acceptance | Pending | Root reviewer owns acceptance |
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
git diff --check
```

Both commands exited 0.
The control child completed its wait with the same recorded PID.
Its raw streams and receipt are in
`/private/tmp/mamba-q0-163-20261029-ezqnkxd8/root-all-helper-controls/`.
These tests cover exact input parsing, the original comparison rule, wrong hashes,
missing logs, repeated log paths, inherited process context, timeouts, process exits,
unknown rows, package RECORD payloads and target continuation after failure.

The original bounded audit Acceptance permits truthful failed and missing findings.
It does not turn them into functional, full-denominator or platform passes.
Only audit capture helpers, their controls, the workflow and this record may change.
Product source, expected data, protected QA and accepted earlier-ticket helpers must not change.
The final record must bind the executed head, source closure, actual commands and exits,
complete raw/index review, durable custody, PR head and merged tree.
