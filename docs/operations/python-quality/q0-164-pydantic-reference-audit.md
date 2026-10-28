# Q0 #164 Pydantic reference audit

This record belongs to [#164](https://github.com/faberline/mamba/issues/164).
The human approved fresh hosted evidence for #160–164.
This audit starts from `6cecd91a45865561f6d4f29ce9e81f16655f3fe3`.
#163 was accepted, integrated and closed before this work started.
The original Acceptance, Never, comparisons and parent gates still apply.
Original-file identity requirements for #16/#228/#229 remain unchanged.

## Fixed scope

The reference material has four asset families.
They let mypy 2.4.0 inspect Pydantic 2.13.5 symbols and types.
They do not declare a separate runtime-validation corpus or original case IDs.
The fixed index has four families and three targets: 12 asset-target sections.
Each section needs its own new process.
The full 22,898-row mypy catalog remains context.
It is not this ticket's acceptance denominator.

| Asset | Bytes | Original/current SHA-256 |
|---|---:|---|
| `field-types.json` | 344,006 | `3feeb55e6b8abf976d00d42bd7c5357a06264d171b49ec4f30528f4fe1c0c5ca` |
| `model-types.json` | 79,871 | `745e7173fbed74acf3924ff92d36c67a6ba271dae63951e6b666facd78cc71a9` |
| `model-methods.json` | 1,013,691 | `4d8b9edc282ed6e064aaf613b74adce81bed8f18b18cd9556bbe08f2c8db7fa9` |
| `model-config-field.json` | 713 | `f4f4a01379dba139751a74465618a158ab80952e7a680e5bc8c0ffedf0853a96` |

These files are under `crates/mamba-quality/assets/pydantic/`.
All four byte-match their retained original assets.
Their total size is 1,438,281 bytes.
Their original indent-2, default-ASCII, final-LF serialization also reconstructs exactly.
This proves frozen content identity. It does not recover an old process record.

The profile SHA-256 is `59cd5c03b0ab25ba7f994bcd81fece95dd84c613f1e0ca4dc947573b7bcae2e7`.
The original oracle lock SHA-256 is `3ede6b08cf4fb8a1a12ec148169667e6b22d484b380f901a1bb040c8a2669a35`.
The Pydantic source archive has 845,750 bytes and 298 regular members.
Its SHA-256 is `51a9c5f7b2f8e636f04c6cada605d9b6a3bf1348fdf945a3d8869b19bba0ee08`.
Its `pydantic/fields.py` SHA-256 is
`6bc66125f23c143e934030fbf9c58c9b5657950bdad3031d4f6132e55bd57be3`.
This matches all three original field-types source identities.

The 41-file owning-source inventory SHA-256 is
`d3d98eb06d8cdae1a658b0392755a2d517ea0e39f70906509d638ab81be4a278`.
This is a digest of path/SHA rows. It is separate from each file's content digest.
Rows use path-component ordering, as `PurePosixPath` does.
They do not use raw path-string ordering.
The source-order clarification SHA-256 is
`112cb0fcb0da82a896c24e899ac2e2948172583f6aa11397675adcb954916e39`.

## Original process and input rules

The host is standard GitHub `macos-15`, ARM64.
The exact Python patches are 3.12.11, 3.13.5 and 3.14.0.
Their archive and executable pins match the accepted prior bootstrap pins.
The ten replay packages are mypy 2.4.0, mypy_extensions 1.1.0, pathspec 1.1.1,
librt 0.16.0, ast_serialize 0.12.1, typing_extensions 4.16.0,
Pydantic 2.13.5, pydantic_core 2.46.5, annotated-types 0.8.0 and typing-inspection 0.4.4.
The five default Pydantic packages and broader original 22-package inventory remain distinct.
The old 3.13 restoration used 3.13.9. It cannot stand in for 3.13.5.
Fresh captures do not recover old venv directories or editable-source identities.

The capture uses a fixed order for the four families.
Each family runs the original target order: 3.12, 3.13 and 3.14.
All 12 processes are independent. No collector shares interpreter state with another.
Each has a 120-second timeout, actual exit 0 and empty stderr.
The parent supplies no cwd, environment or stdin override.
The receipt records the inherited context and the absence of supplied stdin bytes.
It does not invent captured inherited stdin contents.

| Family | Interpreter flags | Original stub | Stub bytes | Complete result |
|---|---|---|---:|---|
| field-types | `-I -c` | `field_types.pyi` | 36 | stdout projection |
| model-types | `-I -c` | `field_types.pyi` | 86 | stdout projection |
| model-methods | `-I -c` | `field_types.pyi` | 86 | stdout projection |
| model-config-field | `-I -B -c` | `config_field.pyi` | 70 | `field.json` |

The first three argv tails are `[minor, work, profile]`.
The fourth tail is `[work, minor, profile]`.
The fourth child creates its own fresh work root.
It also writes the original `identity.json` file.
Its stdout contains only the Python target and class-var receipt.
The exact 127-byte `mypy.ini` enables the original three plugin flags.
Its SHA-256 is `82ca0b8a00280cb3546ec816852c81ec1b68a667c863bdff6d8c22119f1d893e`.
The fixed stubs come from verified import texts, independent of expected type trees.
Their fingerprints are newly verified reconstructions, not recovered old input manifests.

The development API checks `define_options()[1]` against the exact profile flags.
It calls `process_options`, sets `preserve_asts=True`, then calls `build`.
Any build error fails the child.
The new collectors are authored independently from the verified API recipe.
They retain the exact selectors, alias traversal, serialized IDs, unpacked kwargs and sorted slots.
Six Field variants are six ordered overload signatures. All six remain required.
New source/import evidence goes into separate capture files after the original values are derived.

The original comparison is ordinary Python whole-object equality.
Object key order does not change equality. List order and values remain exact.
There is no added type-ID, alias, path, signature, message or variant normalizer.
Python's bool/int equality is preserved by the primary predicate.
Separate JSON type-integrity findings do not redefine that predicate.

## Separate contracts and current evidence

The protected config QA contains 257 projects and 341 source files.
It has 771 target cells and seven explicit entrypoint overrides.
The construct QA selects three exact existing catalog rows, for nine target cells.
These contracts remain separate and `not_run` here.
The assets do not declare typed positive, negative, interaction, dormant or unsupported roles.
Those roles stay `unknown`. No role is inferred from a name.
No absent case-ID, reference-error, exclusion or whole-suite field is reclassified as zero or pass.

The four collector bodies contain no explicit SAME217/SOM import or path.
The complete historical editable environment remains unknown.
This observation does not waive the original identities required by #16/#228/#229.
Native, full-reference, parent #44 and release acceptance remain false or unrun.

| Item | Current state |
|---|---|
| Original recipe for all four families | Recovered and statically checked |
| Asset/source/profile/version identities | Statically checked |
| Locked package metadata and source payloads | Statically checked |
| Combined capture helper controls after repair | 71 passed; no skips; root retained actual wait/exit 0 |
| First hosted preparation attempt | Failed before any original collector launched |
| Formal hosted reference processes | 0 launched; not_run |
| Independent full raw/index/environment review | not_run |
| Bounded audit acceptance and integration | Pending |

Static reports are under `/private/tmp/mamba-q0-164-20261029-16ddkt6_/`.
Recipe SHA-256: `2f806cfcf910e365ed7f3a4db08dea36c745e87a94735db2ae8156c9149e31ce`.
Assertion addendum SHA-256: `4822fd1da7802cc01dee079fcf6adc58f91f1f435857948ec20b2db7322f6d73`.
Source audit SHA-256: `43ee8a2e3f9a3551a3bd7bf462f271961c35713d3cee2add75ec59dd1651cbef`.
Package audit SHA-256: `416955a404ae017bd7b3a95636336bf68dcd92ed737d4d029e536806312a2344`.
The static reports, source events, candidate files and controls have durable custody at
`/Users/chrischeng/.codex/artifacts/mamba-q0-164-hosted-20261029/preflight/`.
Its 126-file custody manifest SHA-256 is
`15957b2d7377f28d01fbd7c1d9c6a327d72d5e1debf8816dd2e50950bacda1e7`.
Root independently repeated all 64 helper controls from the repository root:

```sh
python3 -B -m unittest discover -s tools/ci -p 'test_q0_pydantic*.py' -v
```

The actual control child completed its wait and exited 0.
Its stdout, stderr, source hashes and receipt are in
`/private/tmp/mamba-q0-164-20261029-16ddkt6_/root-all-helper-controls/`.
The controls include the real Python 3.12 typing aliases, with deprecation warnings treated as errors.
Each fileless alias must resolve to the exact same object in an allowed file-backed owner's namespace.
Unknown objects and forged owners still fail. This changes new capture custody only.
The original API values, stdout, input files, argv and comparison rule remain fixed.
Root also matched all 41 named sources to the actual base Git tree.

## First hosted preparation failure

[Run 40773532370](https://github.com/faberline/mamba/actions/runs/40773532370)
used head `202e26f3df1861da0e310da650c74c2caa39b2fd`.
Its source tree is `f86a8715964a48edc22d9030adbe01f523c9e28d`.
The 64 hosted helper controls and all three exact-Python bootstrap steps passed.
The three package-identity probes then exited 1.
Their actual PIDs were 1327, 1353 and 1362. Each child completed its wait.
The probe treated every imported package as one file.
The raw trace shows `Path(module.__file__)` raised `TypeError` on `None`.
Static wheel and pinned importlib checks identify `librt` as the first namespace in probe order.
That attribution is a static inference; the failed stdout did not print the module name.
This is a new evidence-probe failure, before the original API calls.
None of the 12 original collector processes launched.
No asset comparison or formal reference result passed in this attempt.

The complete artifact has 148 files and 104,302,296 uncompressed bytes.
The 99,782,095-byte ZIP SHA-256 is
`906e01437e446c9c0130ffa2ca217c5c46735c10256de9406fbd9295a555b075`.
The failed run, ZIP, metadata and download receipt have durable custody at
`/Users/chrischeng/.codex/artifacts/mamba-q0-164-hosted-20261029/run-40773532370-failed-package-probe/`.
Its 155-file custody manifest SHA-256 is
`0b96f868fd7d073a848c526ceea5fcd315028cbb65d0d658abf8d7ef721e3590`.
Root re-read and matched every saved file's size and SHA-256.
The repair is limited to namespace module evidence and its validation.
The original pins, inputs, API calls, commands and comparisons remain fixed.

The repaired probe retains the namespace's module spec, exact directory and complete own-package subtree.
The parent requires that directory inside the exact target venv's `site-packages`.
Whole installed RECORD and native payload checks still apply.
External, empty, changed, unowned, initialized, symlinked or forged directories fail.
Independent review also found a mixed-format bypass in the first repair.
The final repair requires exactly `path`, `bytes` and `sha256` for ordinary module records.
A namespace record cannot enter that branch with extra fields.
The original failure and both repair reports remain saved.

Root repeated all 71 pure helper controls after that fix.
Child PID 25204 completed its wait and exited 0, without skips.
The before/after source manifests match.
These controls include a real standard-library-created test namespace and a mixed-format forgery.
They import no reference package and execute no original or native tool.
Raw controls are under `root-namespace-helper-controls-r2/` in the static report directory.
The repair addendum SHA-256 is
`d877e6767b4d7a418b5b8cc97c9a4579111b960bfac48602dd93e42eb2e78a0f`.
The independent first-failure review SHA-256 is
`07d634ddcc1b3c5c3b45474dd6eae1283d713f1447eba37e362b8c5a3cb652c1`.
Independent review confirmed the mixed-format counterexample fails after the repair.
Its final source/schema review has no remaining probe blocker.
The review SHA-256 is
`29c170e6ba95792e8c4b02af2e7ae7f72c7ac4b193ed86baecda066ec5231607`.
The next hosted reference run and its full independent review remain pending.

The final record must bind the executed head, final PR source closure, actual command/exit receipts,
all 12 target sections, immutable full evidence, independent review and exact merged tree.
