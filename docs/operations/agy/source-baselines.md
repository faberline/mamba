# Source Baselines Audit: Test, Runtime, Quality, Mambalibs, and Joint Release Before AGY Handoff

- **Issue**: `#61` (`source: map the Test, Runtime and Quality baselines before AGY handoff`, Parent: `#27`)
- **Audit Observation Timestamp (UTC)**: `2026-10-21T11:06:22Z` (`2026-10-21T19:06:22+08:00`)
- **Audit Scope**: Read-only reconciliation of `origin/main`, local `main`, the three protected local working trees, and live GitHub issues `#1`–`#223`. No source files, branches, staged indices, or untracked files in `/Users/chrischeng/faberlines/mamba`, `/Users/chrischeng/.codex/worktrees/9725/mamba`, or `/Users/chrischeng/.codex/worktrees/2a2a/mamba` were modified, stashed, cleaned, or committed during this audit.

---

## 1. Reference Commits and Protected Worktree Baselines

### 1.1 Commit Graph Relationship

All active tracks descend from `origin/main`'s initial readiness baseline (`6e2d1ae10dffd45d5c9244f3ad259e7bc4cc99ce`):

```text
6e2d1ae10dffd45d5c9244f3ad259e7bc4cc99ce  (initial readiness origin/main baseline; 52,335 tracked files)
├── c016c085dd00de23415033764cc239222cde331d  (current origin/main after PR #224 / #59; 52,335 tracked files)
└── fdcb0aa08ec0778ce63ec75d8b0b66f71083834a  (Quality worktree 9725 HEAD; +10 commits over 6e2d1ae10d; 74,404 tracked files)
    ├── 6881195835c85ba7ac9d52f578f4009ad8d8ee1c  (feat(test): land native mamba test product and runner contracts)
    │   └── 611f2c86abe4ec27d8a74c9eda7473eeb8c6519a  (docs(test-landing): record test landing evidence and preservation; local main & Runtime 2a2a HEAD; 74,531 tracked files)
```

| Commit SHA | Reference Role | Tracked File Count (`git ls-tree -r --name-only`) | Commit Title / Summary |
| :--- | :--- | ---: | :--- |
| `6e2d1ae10dffd45d5c9244f3ad259e7bc4cc99ce` | Initial readiness `origin/main` baseline (`#27` / `#61` baseline reference) | `52,335` | `Merge pull request #4 from faberline/feat/mambalibs-wheel-acceptance` |
| `c016c085dd00de23415033764cc239222cde331d` | Current `origin/main` (`1` commit ahead of `6e2d1ae10d`) | `52,335` | `feat(log-mamba): register mambalibs.logging alias alongside cclab.log (#224)` (closes `#59`) |
| `fdcb0aa08ec0778ce63ec75d8b0b66f71083834a` | Quality worktree `9725` `HEAD` (`10` commits ahead of `6e2d1ae10d`, unpushed to `origin/main`) | `74,404` | `test(mamba): split file over 300-line ceiling` (includes `021537c21c` `chore(mambalibs): relocate kits into crates/ and drop external path deps`) |
| `6881195835c85ba7ac9d52f578f4009ad8d8ee1c` | Test product landing commit (`11` commits ahead of `6e2d1ae10d`, unpushed to `origin/main`) | `74,517` | `feat(test): land native mamba test product and runner contracts` (adds `crates/mamba-test` and `crates/mambalibs-unittest`) |
| `611f2c86abe4ec27d8a74c9eda7473eeb8c6519a` | Local `main` and Runtime worktree `2a2a` `HEAD` (`12` commits ahead of `6e2d1ae10d`, unpushed to `origin/main`) | `74,531` | `docs(test-landing): record test landing evidence and preservation` (adds `14` files under `docs/migrations/test-tool/landing/`; `221` total files changed vs `fdcb0aa08e`) |

### 1.2 Commits Between `6e2d1ae10d` and `611f2c86ab` (Local `main`, Unpushed to `origin/main`)

1. `021537c21ce2ab48dc261d4026b4b3159b321ab3` — `chore(mambalibs): relocate kits into crates/ and drop external path deps`
2. `a2b5dcde70147f9a882b13a7de7f0b17f7d3a9cf` — `test(mambalibs): relocate wheel integration tests into mamba-cli`
3. `de8bb1ef7665257df8bf7b1b306e9fa25e04d786` — `chore(workspace): register relocated kit crates and tighten ddd check`
4. `f2f823618d6a3fc87639a79d152754cf6e8a623f` — `refactor(mambalibs): split files over 300-line ceiling`
5. `9fd9e7810c659d516ef8ec53b25a3d7db5b6b007` — `refactor(mamba-core): split files over 300-line ceiling`
6. `e41bb03d7123225750300373c0fd7b8f50f685a2` — `refactor(mamba-stdlib): split files over 300-line ceiling`
7. `ca9a4e687893040fb66f2cb92b07ec14ef05fa93` — `refactor(mamba-compiler): split files over 300-line ceiling`
8. `0c5cf8ec2628b16a6a5182297236f8be79dc761c` — `refactor(mamba-runtime): split files over 300-line ceiling`
9. `b1575b9bf499e1b66c55ac03824d8a3282ba8240` — `refactor(mamba): split core and driver files over 300-line ceiling`
10. `fdcb0aa08ec0778ce63ec75d8b0b66f71083834a` — `test(mamba): split file over 300-line ceiling` (**Quality `9725` `HEAD`**)
11. `6881195835c85ba7ac9d52f578f4009ad8d8ee1c` — `feat(test): land native mamba test product and runner contracts`
12. `611f2c86abe4ec27d8a74c9eda7473eeb8c6519a` — `docs(test-landing): record test landing evidence and preservation` (**Local `main` and Runtime `2a2a` `HEAD`**)

---

## 2. Reconciled Snapshot Boundaries: Historical Manifests vs Live Porcelain Status

### 2.1 Why Full-Source Manifest Counts Differ from `git status --porcelain=v1 -uall` Counts

Two distinct counting boundaries exist across historical handoff artifacts and live git queries:

1. **Full-Source File Inventory (`full-source-map.json` / full-tree snapshot)**:
   Counts **every file present in the working tree** = `(tracked files at HEAD − deleted tracked files) + untracked non-ignored files`.
   - In Quality `9725`: `74,404` tracked files at `fdcb0aa08e` (`74,341` unmodified + `63` modified ` M`) + `4,441` untracked `??` files = **`78,845` total files**.
   - In Runtime `2a2a` (initial `#26` baseline): `74,531` tracked files at `611f2c86ab` − `1` deleted tracked file (` D`) + `373` initial untracked `??` files = **`74,903` total files** (and with current `601` untracked files: `74,531 − 1 + 601 = 75,131` total files).
   - In Primary `/Users/chrischeng/faberlines/mamba`: `74,531` tracked files at `611f2c86ab` (`74,505` unmodified + `26` modified ` M`) + `83` untracked `??` files = **`74,614` total files**.
2. **Dirty / Untracked Porcelain Status (`git --no-optional-locks -C <path> -c core.fsmonitor=false status --porcelain=v1 -uall`)**:
   Counts **only modified/deleted tracked files plus untracked files** (`modified/deleted tracked + untracked`). Unmodified tracked files are omitted by `git status`.
   - In Quality `9725`: `63` modified tracked (` M`) + `4,441` untracked (`??`) = **`4,504` status lines**.
   - In Runtime `2a2a`: `390` modified/deleted tracked (`389` ` M` + `1` ` D`) + `601` untracked (`??`) = **`991` status lines**.
   - In Primary `/Users/chrischeng/faberlines/mamba`: `26` modified tracked (` M`) + `83` untracked (`??`) = **`109` status lines**.

> **Sandbox Measurement Note**: Running `git status` inside an AGY workspace sandbox that restricts read/stat access to `/Users/chrischeng/faberlines/mamba` (`BypassSandbox: false`) fails `lstat()` with `Operation not permitted` on working-tree files inside `/Users/chrischeng/.codex/worktrees/*`, causing `git` to falsely report tracked files as deleted (` D`). Authoritative read-only inspection of `/Users/chrischeng/.codex/worktrees/*` must run with host read access outside the workspace path filter.

### 2.2 Side-by-Side Protected Worktree Inventory (Observed `2026-10-21T11:06:22Z`)

| Worktree / Checkout | Branch & `HEAD` SHA | Historical / Retained Snapshot Identity | Live `status --porcelain=v1 -uall` Count (`2026-10-21T11:06:22Z`) | Live Raw `status --porcelain=v1 -uall` SHA-256 | Drift Between Retained Manifest and Live Tree |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`origin/main` (Initial Readiness Baseline)** | `6e2d1ae10dffd45d5c9244f3ad259e7bc4cc99ce` | `52,335` tracked files | `0` (clean commit) | N/A | Superseded on `origin/main` by `c016c085dd00de23415033764cc239222cde331d` (`#59` / PR `#224`, also `52,335` tracked files) |
| **Primary Checkout** (`/Users/chrischeng/faberlines/mamba`) | `main` @ `611f2c86abe4ec27d8a74c9eda7473eeb8c6519a` (`index_empty: true`) | `docs/migrations/test-tool/landing/evidence/primary-preservation.json`: `109` dirty entries (`26` tracked + `83` untracked); `protected_diff_before_sha256`: `dfc67b56b5578a93c980612f756f196172c3578c45604b352a938152963f8018`; `protected_diff_after_sha256`: `816b4d6e8fc19b5118679580a46db9302648bec84f1d6227077d86ed74687566` | **`109` entries** (`26` tracked ` M`, `0` staged, `83` untracked `??`; `74,614` total files in tree) | `55547aa972941783ae46804bcce2911720a5fbcd0e3a17ba3da4c9a29b2a8956` | **Zero count drift (`109` = `109`)**. All `109` paths are earlier Quality WIP (`103` byte-identical to `9725`, `6` further updated in `9725`). |
| **Quality Worktree (`9725`)** (`/Users/chrischeng/.codex/worktrees/9725/mamba`) | detached `HEAD` @ `fdcb0aa08ec0778ce63ec75d8b0b66f71083834a` (`index_empty: true`) | Full-source manifest `/Users/chrischeng/.codex/worktrees/9725/mamba/target/quality-evidence-20261018-01a13406-fresh-1/existing-differences-batch-integration-1/full-source-map.json`:<br>• **Manifest file SHA-256**: `e12a7e465e9ae6088be8e7a208cd158702dbfb9098b0add1a40cb6fbda62eac5`<br>• **`full_source_sha256` (content identity)**: `fdc25f5b7fb135cbfcb15f5e24219db6b9e4e6c73b964b612e7668e5855c82f6`<br>• **Full-source file count (`source_files`)**: `78,845` (`74,404` tracked + `4,441` untracked)<br>• **Controller state**: `/private/tmp/mamba-quality-current-state-main-20261021-1.json` (`2a1a423f8ddde05b61a667d72270aa54c24665cf3c591877674cff245e630f55`) | **`4,504` entries** (`63` tracked ` M`, `0` staged, `4,441` untracked `??`; `74,404 + 4,441 = 78,845` total files in tree) | `371b7234d6e0d7fed3d53044b8d9a74942efc388659db757f07e1d2c234f554f` | **Zero file drift (`missing = 0`, `mismatches = 0`)**: all `78,845` files in live `9725` match the per-file SHA-256 entries in `full-source-map.json` (`fdc25f5b...`). |
| **Runtime Worktree (`2a2a`)** (`/Users/chrischeng/.codex/worktrees/2a2a/mamba`) | detached `HEAD` @ `611f2c86abe4ec27d8a74c9eda7473eeb8c6519a` (`index_empty: true`) | Initial `#26` protected full-tree snapshot: `74,903` total files (`74,531` tracked − `1` deleted ` D` + `373` initial untracked `??`), plus subsequent `#26` batch overlays (`+ 193` Batch 42 overlay paths and Batch 45 `annotation-source-provenance-coherent-v1-installed/manifest.json` SHA-256 `c5a69857d3420a18691e565e1033a1e165460ffdfca63776c74f1312bd3092a3` & `traceframe-panic-QA-v3-installed/manifest.json` SHA-256 `51e88d63e0c25015debf5b37829ef18007e94066eb91b64234ab19c347bbdb42`) | **`991` entries** (`390` tracked [`389` ` M`, `1` ` D`], `0` staged, `601` untracked `??`; `74,531 - 1 + 601 = 75,131` total files in tree) | `c679d43d6992970b2a6bc175740e9cfd2b5b8cec6465393256e1bab02c521528` | `+228` untracked files over the initial `373`-untracked (`74,903` full-tree) snapshot (`373 -> 601` untracked, `74,903 -> 75,131` total files), accounted for by `#26` Batches 42–46 Runtime overlays and tests. |

### 2.3 Exact Shared-File Overlaps Across Tracks

#### A. Test Product Commits (`fdcb0aa08e..611f2c86ab`, `221` changed files) vs Quality Worktree (`9725`, `4,504` dirty/untracked paths)

Exactly **`6` shared files** are modified by both `fdcb0aa08e..611f2c86ab` and `9725`:

1. `Cargo.lock`
2. `Cargo.toml`
3. `crates/mamba-cli/Cargo.toml`
4. `crates/mamba-cli/src/entry.rs`
5. `crates/mamba-cli/tests/it/main.rs`
6. `ddd.toml`

*(Note: In the Primary checkout `/Users/chrischeng/faberlines/mamba`, `docs/migrations/test-tool/landing/evidence/primary-preservation.json` already reconciled those `6` files on top of `611f2c86ab`, leaving `103` of Primary's `109` dirty paths byte-identical to `9725` and only these `6` files differing between Primary's dirty tree and `9725`.)*

#### B. Test Product Commits (`fdcb0aa08e..611f2c86ab`, `221` changed files) vs Runtime Worktree (`2a2a`, `991` dirty/untracked paths)

`2a2a` is already checked out at `HEAD = 611f2c86abe4ec27d8a74c9eda7473eeb8c6519a`. Only **`1` file** modified in `fdcb0aa08e..611f2c86ab` also has uncommitted edits in `2a2a`:

1. `crates/mamba-compiler/Cargo.toml`

#### C. Runtime Worktree (`2a2a`, `991` dirty/untracked paths) vs Quality Worktree (`9725`, `4,504` dirty/untracked paths)

Exactly **`12` dirty paths** overlap between `2a2a` and `9725` (zero untracked paths overlap):

- **`5` `crates/mamba-runtime` files** where `9725` holds the older Primary dirty copy and `2a2a` holds newer Runtime edits:
  1. `crates/mamba-runtime/src/infrastructure/vm/module.rs`
  2. `crates/mamba-runtime/src/infrastructure/vm/module/compile_and_exec_module.rs`
  3. `crates/mamba-runtime/src/infrastructure/vm/module/mb_import.rs`
  4. `crates/mamba-runtime/src/infrastructure/vm/module/tests.rs`
  5. `crates/mamba-runtime/src/infrastructure/vm/module_compilation/tests.rs`
- **`7` `crates/mamba-language` files** where both `2a2a` and `9725` have independent modifications relative to `fdcb0aa08e`:
  6. `crates/mamba-language/src/application/lexing/tests_files/core.rs`
  7. `crates/mamba-language/src/domain/literal/tests/case_display_literals.rs`
  8. `crates/mamba-language/src/domain/literal/tests/case_lex_complex_float.rs`
  9. `crates/mamba-language/src/domain/literal/tests/case_lex_float.rs`
  10. `crates/mamba-language/src/domain/parser.rs`
  11. `crates/mamba-language/src/domain/parser/expr/tests/case_float_literal.rs`
  12. `crates/mamba-language/src/domain/parser/type_expr.rs`

*(Historical `#26` Phase 23 cross-check: when `2a2a` replayed `4,227` proposed/shared/mixed Quality paths against `2a2a`, `4,215` paths were absent in `2a2a`, `6` matched the peer original, and `6` required three-way reconciliation.)*

#### D. Mambalibs Path Divergence Between `origin/main` (`c016c085d`) and Local `main` (`fdcb0aa08e..611f2c86ab`)

Commit `021537c21ce2ab48dc261d4026b4b3159b321ab3` (`chore(mambalibs): relocate kits into crates/ and drop external path deps`) and `a2b5dcde70147f9a882b13a7de7f0b17f7d3a9cf` (`test(mambalibs): relocate wheel integration tests into mamba-cli`) exist on local `main` (`fdcb0aa08e..611f2c86ab`), `9725`, and `2a2a`, but are **not** on `origin/main`:

| Kit / Suite | Path on `origin/main` (`6e2d1ae10d` / `c016c085d`) | Path on Local `main` (`fdcb0aa08e..611f2c86ab`), `9725`, and `2a2a` | Target Path Written in Active Issues (`#60`, `#70`–`#95`) |
| :--- | :--- | :--- | :--- |
| `arraykit` | `mambalibs/arraykit/{src,binding,pyo3}` | `crates/arraykit`, `crates/arraykit-binding`, `crates/arraykit-pyo3` | `mambalibs/arraykit/binding/src/...`, `e2e/mambalibs_array_wheel.rs` |
| `scikit` | `mambalibs/scikit/{src,binding,pyo3}` | `crates/scikit`, `crates/scikit-binding`, `crates/scikit-pyo3` | `mambalibs/scikit/binding/src/...`, `e2e/mambalibs_sci_wheel.rs` |
| `plotkit` | `mambalibs/plotkit/{src,binding,pyo3}` | `crates/plotkit`, `crates/plotkit-binding`, `crates/plotkit-pyo3` | `mambalibs/plotkit/binding/src/...`, `e2e/mambalibs_plot_wheel.rs` |
| `cryptokit` | `mambalibs/cryptokit/src` | `crates/cryptokit/src` | `mambalibs/cryptokit/pyo3/...`, `e2e/mambalibs_crypto_wheel.rs` |
| `mediakit` | `mambalibs/mediakit/src` | `crates/mediakit/src` | `mambalibs/mediakit/pyo3/...`, `e2e/mambalibs_media_wheel.rs` |
| `mongokit` | `mambalibs/mongokit/src` | `crates/mongokit/src` | `mambalibs/mongokit/pyo3/...`, `e2e/mambalibs_mongo_wheel.rs` |
| `log-mamba` | `crates/log-mamba/src` | `crates/log-mamba/src` | `crates/log-mamba/src/...` (same in both) |

Because AGY issue worktrees branch from `origin/main` (`c016c085d`), Mambalibs issues `#60` and `#70`–`#96` match `origin/main` directly, whereas any future merge of `021537c21c..fdcb0aa08e` must reconcile those path moves.

---

## 3. Exact Ticket-to-Product and Child Mapping (`#1`–`#223`)

### 3.1 Summary of All Repository Issues (`#1`–`#223`)

*(Note: `#4` is a merged Pull Request; all other numbers in `1..223` are issues.)*

| Track | Parent / Tracker Issues | Intermediate / Group Issues | Atomic / Leaf Issues | Target Product / Subsystems | Required Source Baseline |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Legacy Runtime / Concurrency** | `#26` (reconciled under Runtime release) | — | `#1`, `#2`, `#3` | `crates/mamba-runtime` (`Value::clone_val` / refcount race `#1`, `MbObject` atomic header `#2`, `TOKIO_RT` multi-thread pool `#3`; originally filed with legacy `apps/mamba/...` paths) | Runtime `2a2a` (`#26`) or `origin/main` `crates/mamba-runtime` |
| **Test Runner & Fixtures** | `#5`, `#6` | — | `#62`–`#66` (under `#5`), `#67`–`#69` (under `#6`) | `crates/mamba-test`, `crates/mambalibs-unittest` (`mambalibs.unittest` / `cclab.qc` compatibility), `crates/mamba-cli` (`mamba test`) | Local `main` `6881195835..611f2c86ab` (**missing on `origin/main`**; requires unauthored Test baseline handoff first) |
| **Mambalibs Native & Wheels** | `#7` (umbrella), `#8`, `#9`, `#10`, `#11`, `#12`, `#13`, `#14` (CLOSED) | — | `#59` (CLOSED), `#60`, `#70`–`#96` | `mambalibs.array` (`#8`: `#70`–`#73`), `mambalibs.sci` (`#9`: `#74`–`#82`), `mambalibs.plot` (`#10`: `#83`–`#85`), `mambalibs.crypto` (`#11`: `#60`, `#86`–`#88`), `mambalibs.media` (`#12`: `#89`–`#92`), `mambalibs.mongo` (`#13`: `#93`–`#95`), `mambalibs.logging` (`#14`: `#59` CLOSED, `#96` OPEN) | `origin/main` (`c016c085d`) directly has `mambalibs/<kit>/` and `crates/log-mamba/` |
| **Quality (`mamba quality`)** | `#15` (umbrella), `#16`–`#25` | `#29`–`#58` | `#102`–`#203` | Five native Rust quality tools in one `mamba` binary for Python `3.12`/`3.13`/`3.14` (**Ruff `0.16.10`**, **isort `9.0.2`**, **mypy `2.4.0` strict + built-in Pydantic `2.13.5`**, **Pylint `4.1.2`**, **Black `26.10.0`** — no Pyright) in `crates/mamba-quality`, `crates/mamba-cli`, `crates/mamba-language`, `docs/operations/python-quality/` | Docs/ledger issues (`#129`, `#159`–`#165`, `#201`): `origin/main` + read-only `9725`/`/private/tmp` evidence.<br>Engine/gate issues (`#29`–`#42`, `#45`–`#58`, `#102`–`#128`, `#130`–`#158`, `#166`–`#200`, `#202`–`#203`): Quality `9725` baseline (`fdcb0aa08e` + `fdc25f5b...`, **missing on `origin/main`**) |
| **Runtime (`mamba` execution)** | `#26` | — | `#204`–`#218` | `crates/mamba-runtime`, `crates/mamba-compiler`, `crates/mamba-core`, `crates/mamba-stdlib`, `crates/mamba-cli`, `docs/operations/runtime/` | Audit doc `#204`: `origin/main` + read-only `2a2a` evidence.<br>Code/gate issues `#205`–`#218`: Runtime `2a2a` baseline (`611f2c86ab` + `991` dirty paths, **missing on `origin/main`**) |
| **Joint Release (`0.1.0-alpha` / `0.4.13`)** | `#27` | — | `#61`, `#219`–`#223` | `docs/operations/agy/` (`#61`, `#219`, `#220`) and joint 3-platform candidate verification (`#221` `macos-arm64`, `#222` `linux-x86_64`, `#223` `linux-arm64`) | `#61`, `#219`, `#220`: `origin/main` (`docs/operations/agy/`).<br>`#221`–`#223`: reconciled joint candidate from `#219` |
| **Parked Axiom WIP (`ac3d7d9c74`)** | `#28` | — | `#97`–`#101` | 15 parked files from `/Users/chrischeng/projects/axiom` stash `ac3d7d9c74`: audit (`#97`), compiler/typeck (`#98`), runtime call/class/executor (`#99`), 8 stdlib wrappers (`#100`), 2 conformance/perf tests (`#101`) | `#97`: `origin/main` + read-only `/Users/chrischeng/projects/axiom`.<br>`#98`–`#101`: `origin/main` after `#97` |

---

### 3.2 Test Track Mapping (`#5`, `#6`, Children `#62`–`#69`)

| Parent | Child Issue | Title & Scope | Declared `Depends on` | Target Paths | Baseline Availability on `origin/main` (`c016c085d`) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`#5`** (`perf(test): reuse worker processes and compiled test modules`) | **`#62`** | `test(runner): expose prepared-module execution through the runner port` | `#61` | `crates/mamba-test/src/application/ports.rs`, `crates/mamba-test/src/infrastructure/mamba_runner/mod.rs`, `crates/mamba-test/src/infrastructure/mamba_runner/execute.rs` | **BLOCKED on `origin/main`**: `crates/mamba-test` only exists in unpushed commit `6881195835c85ba7ac9d52f578f4009ad8d8ee1c` (`611f2c86ab`). `#62` is scoped to prepared-module execution inside `crates/mamba-test`, **not** publishing `fdcb0aa08e..611f2c86ab`. |
| **`#5`** | **`#63`** | `test(worker): reuse supervised workers for prepared test modules` | `#62` | `crates/mamba-test/src/domain/worker.rs`, `crates/mamba-test/src/infrastructure/worker_pool/{mod,process}.rs`, `crates/mamba-test/src/application/execute_suite.rs` | Requires `#62` + `crates/mamba-test` baseline |
| **`#5`** | **`#64`** | `test(isolation): reset case state between reused worker executions` | `#63` | `crates/mamba-test/src/infrastructure/mamba_runner/execute.rs`, `crates/mambalibs-unittest/src/registry.rs`, `crates/mamba-test/tests/suite_isolation.rs` | Requires `#63` + `crates/mamba-test` & `crates/mambalibs-unittest` baseline |
| **`#5`** | **`#65`** | `test(supervision): replace timed-out workers and clean their child process groups` | `#63, #64` | `crates/mamba-test/src/infrastructure/worker_pool/{process,mod}.rs`, `crates/mamba-test/src/domain/worker.rs`, `crates/mamba-test/tests/worker_supervision.rs` | Requires `#63, #64` |
| **`#5`** | **`#66`** | `test(bench): record paired 20-case and 200-case worker-reuse measurements` | `#64, #65` | `crates/mamba-test/tests/worker_reuse_bench.rs`, `docs/operations/test/worker-reuse-benchmark.md` | Requires `#64, #65` |
| **`#6`** (`feat(test): add per-test fixture setup and cleanup`) | **`#67`** | `test(fixtures): define the per-case fixture graph and reverse cleanup model` | `#61` | `crates/mamba-test/src/domain/{fixture,case,report}.rs`, `crates/mambalibs-unittest/src/{fixture,registry}.rs` | **BLOCKED on `origin/main`**: requires `crates/mamba-test` and `crates/mambalibs-unittest` from `6881195835c85ba7ac9d52f578f4009ad8d8ee1c` |
| **`#6`** | **`#68`** | `test(fixtures): inject per-case fixtures through the native runner` (preserving `mambalibs.unittest` and `cclab.qc` compatibility) | `#67` | `crates/mamba-test/src/infrastructure/mamba_runner/execute.rs`, `crates/mambalibs-unittest/src/{binding,fixture}.rs`, `crates/mamba-test/tests/fixtures_lifecycle.rs` | Requires `#67` |
| **`#6`** | **`#69`** | `test(fixtures): clean runner-owned fixture resources after worker death` | `#68, #65` | `crates/mamba-test/src/domain/fixture.rs`, `crates/mamba-test/src/infrastructure/worker_pool/mod.rs`, `crates/mamba-test/tests/fixtures_worker_crash.rs` | Requires `#68, #65` |

---

### 3.3 Mambalibs Track Mapping (`#7`, `#8`–`#14`, Children `#59`, `#60`, `#70`–`#96`)

Umbrella tracker: **`#7`** (`mambalibs: track remaining wheel and native interface delivery`).

| Product / Parent Issue | Interface Mode | Child Issues & Exact Scope | Declared `Depends on` | Target Paths on `origin/main` (`c016c085d`) |
| :--- | :--- | :--- | :--- | :--- |
| **`#8` — `mambalibs.array`** (`implement the native carrier for the existing wheel API`, reusing `mambalibs/arraykit/pyo3/` semantics) | Native `mamba` carrier (`mambalibs/arraykit/binding/`) | • **`#70`**: define and verify one native array ownership handle (`mambalibs/arraykit/binding/src/{handle,lib,methods}.rs`)<br>• **`#71`**: register `DType`, `NdArray` shape metadata, and `zeros`/`ones`/`full` constructors<br>• **`#72`**: implement native array `get` and `set` with wheel errors (`IndexError`/`TypeError`/`ValueError`)<br>• **`#73`**: implement `reshape`, `flatten`, and `transpose` | `#70` -> `#61`<br>`#71` -> `#70`<br>`#72` -> `#71`<br>`#73` -> `#72` | `mambalibs/arraykit/binding/src/` (present on `origin/main`) |
| **`#9` — `mambalibs.sci`** (`implement native science exports with wheel API parity` against `mambalibs/scikit/pyo3/src/lib.rs`) | Native `mamba` carrier (`mambalibs/scikit/binding/`) | • **`#74`**: `stats` (`describe`, `skew`, `kurtosis`, `zscore`, `pearsonr`, `spearmanr`, `ttest_1samp`, `ttest_ind`, `mannwhitneyu`, `chisq_test`, `Normal`, `Uniform`, `Exponential`, `Poisson`, `StudentT`, `ChiSquared`, `Binomial`, `Beta`, `Gamma`)<br>• **`#75`**: `fft` (`fft`, `ifft`, `rfft`, `irfft`, `fftfreq`, `rfftfreq`, `hanning`, `hamming`, `blackman`, `bartlett`, `next_fast_len`, `fft2`, `ifft2`)<br>• **`#76`**: `signal` (`convolve`, `correlate`, `FirFilter`, `moving_average`, `ewma`, `gaussian_smooth`, `savgol_filter`, `find_peaks`, `welch`, `stft`)<br>• **`#77`**: `interpolate` (`LinearInterpolator`, `CubicSpline`, `AkimaInterpolator`, `PchipInterpolator`, `BilinearInterpolator`, `interp1d`)<br>• **`#78`**: `optimize` (`bisect`, `brentq`, `newton`, `secant`, `minimize_1d`, `nelder_mead`, `gradient_descent`, `bfgs`, `lbfgs`, `curve_fit`, `polyfit`, `polyval`)<br>• **`#79`**: `ts` (`TimeSeries`, `adf_test`, `kpss_test`, `pp_test`, `ArimaModel`, `SimpleExpSmoothing`, `HoltLinear`, `HoltWinters`, `stl_decompose`, ` classical_decompose`, `ewma`, `expanding_mean`, `expanding_std`, `autocorrelation`, `partial_autocorrelation`, `detect_outliers_iqr`, `detect_outliers_zscore`, `fill_forward`, `fill_backward`, `fill_linear`)<br>• **`#80`**: `spatial` (`euclidean`, `manhattan`, `cosine`, `chebyshev`, `minkowski`, `cdist`, `pdist`, `KdTree`, `convex_hull_2d`, `point_in_polygon`, `polygon_area`, `Delaunay2D`, `voronoi_2d`)<br>• **`#81`**: `sparse` (`CsrMatrix`, `CscMatrix`, `eye`, `diags`, `spsolve`, `cg`, `bicgstab`, `gmres`)<br>• **`#82`**: `integrate` (`trapezoid`, `simpson`, `romberg`, `gauss_legendre`, `quad`, `dblquad`, `solve_ivp`, `Rk45Solver`, `EulerSolver`) | `#74`–`#82` all depend on **`#70`** (`mambalibs.array` native handle contract) | `mambalibs/scikit/binding/src/` (present on `origin/main`) |
| **`#10` — `mambalibs.plot`** (`implement the native plotting carrier with wheel API parity` against `mambalibs/plotkit/pyo3/`) | Native `mamba` carrier (`mambalibs/plotkit/binding/`) | • **`#83`**: expose `ThemeName` and `DataSeries` constructors (`line`, `bar`, `scatter`, `histogram`, `box_plot`, `heatmap`, `pie`, `annotated_line`, `candlestick`, `violin`, `area`, `radar`, `stacked_bar`)<br>• **`#84`**: expose `Chart` builder (`Chart::new`, `title`, `x_label`, `y_label`, `size`, `theme`, `show_legend`, `show_grid`, `add_series`, `add_annotation`, `set_subplots`) and `to_svg` / `save_svg`<br>• **`#85`**: expose HTML rendering (`to_html`, `save_html`) and `nice_ticks` | `#83` -> `#70`<br>`#84` -> `#83`<br>`#85` -> `#84` | `mambalibs/plotkit/binding/src/` (present on `origin/main`) |
| **`#11` — `mambalibs.crypto`** (`ship the first hashing and authenticated encryption CPython wheel`) | CPython `cp312-abi3` wheel (`mambalibs/cryptokit/pyo3/`) | • **`#60`**: `cp312-abi3` wheel for four SHA-2 functions (`sha256`, `sha256_hex`, `sha512`, `sha512_hex`)<br>• **`#86`**: add `blake3_hash`, `blake3_hex`, `hmac_sha256`, `hmac_sha256_hex`, `hmac_sha256_verify`<br>• **`#87`**: add `random_bytes`, `generate_key_256`, `generate_nonce_96`, `generate_nonce_192`, `aes256_gcm_encrypt`, `aes256_gcm_decrypt`<br>• **`#88`**: add `chacha20_poly1305_encrypt` and `chacha20_poly1305_decrypt` | `#60` -> `None`<br>`#86` -> `#60`<br>`#87` -> `#60`<br>`#88` -> `#87` | `mambalibs/cryptokit/pyo3/` & `e2e/mambalibs_crypto_wheel.rs` (ready on `origin/main`) |
| **`#12` — `mambalibs.media`** (`ship the first image processing CPython wheel`) | CPython `cp312-abi3` wheel (`mambalibs/mediakit/pyo3/`) | • **`#89`**: ship `Image` metadata, `PixelFormat`, `imdecode`, and `imencode` (PNG/JPEG)<br>• **`#90`**: add `imread` and `imwrite` with OS error mapping<br>• **`#91`**: expose `resize` (`Nearest`/`Bilinear`) and `crop` with checked bounds<br>• **`#92`**: expose `flip_horizontal`, `flip_vertical`, and `rotate90` | `#89` -> `None`<br>`#90` -> `#89`<br>`#91` -> `#89`<br>`#92` -> `#89` | `mambalibs/mediakit/pyo3/` & `e2e/mambalibs_media_wheel.rs` (ready on `origin/main`) |
| **`#13` — `mambalibs.mongo`** (`ship the first MongoDB connection and CRUD CPython wheel`) | CPython `cp312-abi3` wheel (`mambalibs/mongokit/pyo3/`) | • **`#93`**: first client wheel with connection (`MongoClient.connect`, `from_uri`, `ping`, `database_name`) and `close` semantics<br>• **`#94`**: BSON document boundary conversion (`bson_to_py` / `py_to_bson`, `ObjectId`)<br>• **`#95`**: single-collection CRUD (`insert_one`, `insert_many`, `find_one`, `find`, `update_one`, `delete_one`, `count_documents`) | `#93` -> `None`<br>`#94` -> `#93`<br>`#95` -> `#94` | `mambalibs/mongokit/pyo3/` & `e2e/mambalibs_mongo_wheel.rs` (ready on `origin/main`) |
| **`#14` — `mambalibs.logging`** (CLOSED: `expose the planned logging name and retain cclab.log`) | Native `mamba` carrier (`crates/log-mamba/`) | • **`#59`** (**CLOSED** via PR `#224` @ `c016c085d`): register `debug`, `info`, `warning`, `error`, `critical` under both `mambalibs.logging` and `cclab.log`<br>• **`#96`** (**OPEN**): enforce invalid-argument errors in `crates/log-mamba/src/methods.rs` without changing valid logs | `#59` -> `None` (merged)<br>`#96` -> `#59` | `crates/log-mamba/src/` (ready on `origin/main` right now) |

---

### 3.4 Quality Track Mapping (`#15`, Parents `#16`–`#25`, Intermediate `#29`–`#58`, Leaves `#102`–`#203`)

Umbrella tracker: **`#15`** (`quality: deliver and verify five native Rust quality tools in one mamba binary` — **Ruff `0.16.10`**, **isort `9.0.2`**, **mypy `2.4.0` strict + built-in Pydantic `2.13.5`**, **Pylint `4.1.2`**, **Black `26.10.0`** across Python `3.12`, `3.13`, and `3.14`; **no Pyright**).

| Milestone & Parent (`#16`–`#25`) | Intermediate Issue (`#29`–`#58`) | Leaf Issues (`#102`–`#203`) | Exact Scope & Local Scratch / Evidence Artifacts | Declared `Depends on` & True Prerequisite Boundary |
| :--- | :--- | :--- | :--- | :--- |
| **`#16` (Milestone Q0)** — `recover the import audit and lock the full reference inventory` | **`#44`** (`完成五工具鎖定清單與逐行驗收分母`) | • **`#159`**: Ruff `0.16.10` rule & coverage denominator (`ruff-0.16.10.json`)<br>• **`#160`**: isort `9.0.2` option/behavior denominator (`isort-9.0.2.json`)<br>• **`#161`**: mypy `2.4.0` strict/flag/error-code denominator (`mypy-2.4.0.json`)<br>• **`#162`**: Pylint `4.1.2` default-checker/message denominator (`pylint-4.1.2.json`)<br>• **`#163`**: Black `26.10.0` AST/comment/stability denominator (`black-26.10.0.json`)<br>• **`#164`**: Pydantic `2.13.5` model/field/validator denominator (`pydantic-2.13.5.json`)<br>• **`#165`** (direct child of `#16`): audit unresolved original `76,831`-path import evidence (`import-audit-76831.md`) | Writes JSON/Markdown lock manifests under `docs/operations/python-quality/` from retained `9725` and `/private/tmp` evidence. Keeps missing original `76,831` path map / ignored-failure inventory explicitly marked `missing` / unaccepted (`#165`). | `#159`–`#165` all declare `Depends on: #61`.<br>**Ready on `origin/main` once `#61` lands** (read-only access to `9725` and `/private/tmp`; no Runtime `#26` dependency). |
| **`#17` (Milestone Q2)** — `finish syntax, project discovery, and public API acceptance` | **`#50`** (`完成既有語法、專案發現與公共命令契約驗收`) | • **`#177`**: project discovery (`pyproject.toml`, `ruff.toml`, `.ruff.toml`, `.isort.cfg`, `mypy.ini`, `.pylintrc`, `setup.cfg`) & Python `3.12`/`3.13`/`3.14` target selection<br>• **`#178`**: public request/report & command behavior (`mamba quality check/format`, `API17`, `CLI11`)<br>• **`#179`**: source bytes, BOM, newline, invalid UTF-8, and 1-based UTF-8 location behavior (`6` macOS invalid-byte CLI cases currently `NOT_RUN`) | Verifies `crates/mamba-quality`, `crates/mamba-cli`, `crates/mamba-language` (`language505`, `API17`, `CLI11`) and records receipts under `docs/operations/python-quality/api-cli/`. | `#177`–`#179` declare `Depends on: #61` (and `#159`–`#163`), and require the **`9725` Quality source tree (`fdc25f5b...`)** for live `crates/mamba-quality` / `mamba quality` execution. |
| **`#18` (Milestone Q2)** — `close the pinned Ruff rule and safe-fix acceptance matrix` | **`#45`** (`核對既有 unknown 與編碼證據缺口`) | • **`#166`**: verify Ruff `26` unknown-rule & error-object evidence (`ruff-unknown-26.json`)<br>• **`#167`**: verify Ruff `12` encoding & safe-fix convergence evidence (`ruff-encoding-safefix-12.json`) | Closes Ruff `0.16.10` evidence gaps in `crates/mamba-quality/src/infrastructure/ruff/` and `docs/operations/python-quality/ruff/`. | Declare `Depends on: #159`. Requires `9725` Quality source / evidence tree. |
| **`#19` (Milestone Q2)** — `complete native isort 9.0.2 byte and error parity` | **`#46`** (`補齊既有 CLI、skip exception 與原版退出證據`) | • **`#168`**: verify isort original CLI wait and exit evidence (`isort-cli-exit-evidence.json`)<br>• **`#169`**: verify isort exception and `FileSkipComment` parity (`isort-skip-exceptions.json`)<br>• **`#170`**: verify isort fixed `108`-row stable-output chain (`isort-108-stable-chain.json`) | Closes isort `9.0.2` parity and process receipts in `crates/mamba-quality/src/infrastructure/isort/` and `docs/operations/python-quality/isort/`. | Declare `Depends on: #160` (`#170` also `#168, #169`). Requires `9725` Quality source / evidence tree. |
| **`#20` (Milestone Q2)** — `complete native Black 26.10.0 byte and structure parity` | **`#47`** (`將已核准 174 筆 ASTSafety 原版證據接到最終候選`)<br>**`#48`** (`完成既有錯誤分類、changed-candidate 結構與完整格式對照`) | • **`#47`** (no leaf sub-issues): join verified `174` original Black ASTSafety receipts (`/private/tmp/mamba-quality-black174-original-controller-main-20261019-1/proof.json`, SHA `4f7d2426f68e586d8ccc7d315ecd0d5e9e3eff96695f30e05a9f9d43620816cb`) to the final native candidate<br>• **`#171`** (under `#48`): Black unchanged-input `313` rows and `396`/`51` error classification<br>• **`#172`** (under `#48`): Black changed-candidate AST/comment structure validation (joins `#47` `174` rows)<br>• **`#173`** (under `#48`): Black exact stable-layout comparison | Closes Black `26.10.0` parity in `crates/mamba-quality/src/infrastructure/black/` and `docs/operations/python-quality/black/`. Note: the `174` original Black ASTSafety receipts already exist and are verified; `#47`/`#172` join them to final native outputs. | `#171`–`#173` declare `Depends on: #163` (`#172` -> `#171`, `#173` -> `#171, #172`). Require `9725` Quality source / evidence tree. |
| **`#21` (Milestone Q1)** — `complete mypy strict and Pydantic 2 inference parity` | **`#29`** (`整合已審查的 untyped-call recovery，補回 finalize 生成式診斷`)<br>**`#30`** (`整合 canonical typing import 的 method-root 證明`)<br>**`#31`** (`完成並審查 DictComp contextual overload 草稿`) | *(No leaf sub-issues under `#29`, `#30`, `#31`; they are the 3 prerequisite mypy proposal tickets for `#32`–`#43`)* | • **`#29`**: Integrates the **sealed 6-file mypy untyped-call recovery proposal** (`/private/tmp/mamba-quality-existing-untyped-candidate-recovery-sol-20261021-1/pins.json`, SHA `2ff56e6408cfb47f2ec15f7f39545fc4fb2b49dbb23b93a45be2bf8d4d1b324d`, base `fdc25f5b...`) and restores the missing generator diagnostic at `scripts/finalize_m1.py:39` (`#29` is **not** a whole-tree `9725` branch publication ticket).<br>• **`#30`**: Integrates the **sealed 3-file mypy method typing-root proposal** (`scopes/decorators/methods.rs`, `scopes.rs`, `types/annotations.rs`; `/private/tmp/mamba-quality-existing-method-typing-root-repair-sol-20261021-1/pins.json`, SHA `1c576181ae0feb6a314de231df839c67dba858c81d80be8dd375fce5ca7a97fd`).<br>• **`#31`**: Completes `REPORT.md`, `pins.json`, and independent peer review for the **interrupted 12-file DictComp contextual overload draft** (`/private/tmp/mamba-quality-existing-dictcomp-candidate-repair-sol-20261021-1`, `canonical.diff` SHA `afdae509dca778a03a4abe87150e7e6e6782a8fe96dbb7ca8036d6c544c1faa4`, base counterfactual `815f2d0770cf6e7344f3ff59dcf63f0f7b7e0a15b031f6aeba2a4a37fea5b700`). | All three (`#29`, `#30`, `#31`) target `crates/mamba-quality/src/infrastructure/mypy/` on top of Quality source `fdc25f5b...` (`9725` at `fdcb0aa08e`). **Blocked on `origin/main` until the missing Quality source-owner handoff publishes `fdcb0aa08e` + `fdc25f5b...` (or an explicit Quality integration branch).** |
| **`#21` (Milestone Q1)** — SOM217 `79` diagnostic diffs (`72` missing, `7` extra across `69` ordered edit groups) | **`#32`**–**`#42`** | • **`#32`** (`no-untyped-call`): `#102` (module/helper calls), `#103` (`main()` entry calls), `#104` (returned helper/subscript calls), `#105` (`scripts/run_m1_7_3.py` extra false positives)<br>• **`#33`** (`var-annotated`): `#106` (missing dict/set annotations + preserve `scripts/audit_m3_flow_data.py:23`), `#107` (extra `var-annotated` false positives)<br>• **`#34`** (`call-overload`): `#108` (`__getitem__`), `#109` (`__setitem__`)<br>• **`#35`** (`assignment`): `#110` (loop/optional narrowing), `#111` (subscript/mapping target), `#112` (extra `assignment` false positives)<br>• **`#36`** (`index`): `#113` (`object` subscript), `#114` (`Collection[str]`), `#115` (`list` slice/index), `#116` (`Union[..., None]` index), `#117` (nested union mapping/sequence), `#118` (`Sequence[object]` target)<br>• **`#37`** (`union-attr`): `#119` (`object` in union), `#120` (`None`/scalar in union)<br>• **`#38`** (`arg-type`): `#121` (`ast.parse` & `tokenize.generate_tokens`), `#122` (`list.append`/`extend` & constructor), `#123` (callable/path/mapping args)<br>• **`#39`** (`attr-defined`): `#124` (`module` & `object` attrs), `#125` (`AST`/`stmt` attrs), `#126` (comprehension/iterator target attrs)<br>• **`#40`** (`operator`): `#127` (`Path`/`object` binary ops), `#128` (numeric/string/collection comparisons)<br>• **`#41`** (`dict-item` — no leaf children): unpacked dictionary entry diagnostics<br>• **`#42`** (`misc` unpack — no leaf children): three tuple unpack arity diagnostics | Repairs the exact `79` SOM217 diagnostic differences (`Gold = 4034`, `native = 3969`, `missing_cause_only = 72`, `extra_cause_only = 7`, `ordered_edit_groups = 69`) in `crates/mamba-quality/src/infrastructure/mypy/`. | `#102`–`#128` declare `Depends on: #29, #30, #31`. Require `9725` Quality source + `#29, #30, #31`. |
| **`#21` (Milestone Q1)** — `412` incomplete capability obligations | **`#43`** (`關閉既有 412 項 incomplete capability 義務`) | • **`#129`**: reconcile the exact `412` completion-obligation ledger (`docs/operations/python-quality/remaining-412-ledger.json`)<br>• **`#130`**–**`#158`**: `29` bounded mypy/Pydantic producer contracts and repairs closing all `412` `incomplete` capability items so `mypy_completion == "complete"` | `#129` reconciles the `412` ledger in `docs/operations/python-quality/remaining-412-ledger.json`. `#130`–`#158` implement the `29` producer slices in `crates/mamba-quality/src/infrastructure/mypy/`. | `#129` declares `Depends on: #61` (ready on `origin/main` after `#61` using read-only `9725`/`/private/tmp` evidence).<br>`#130`–`#158` declare `Depends on: #129, #29, #30, #31` (require `9725` Quality source). |
| **`#22` (Milestone Q2)** — `complete Pylint default rules and cross-file inference parity` | **`#49`** (`補齊既有原版程序與公共完成欄位證據`) | • **`#174`**: Pylint original process and completion-field receipts (`pylint-process-completion.json`)<br>• **`#175`**: Pylint `383`-message evidence ledger (`pylint-383-ledger.json`)<br>• **`#176`**: Pylint current-source cross-file inference & duplicate-code behavior (`pylint-cross-file-inference.json`) | Closes Pylint `4.1.2` parity and process evidence in `crates/mamba-quality/src/infrastructure/pylint/` and `docs/operations/python-quality/pylint/`. | Declare `Depends on: #162` (`#175` -> `#162, #174`; `#176` -> `#162, #174`). Require `9725` Quality source / evidence tree. |
| **`#23` (Milestone Q2)** — `close safe publication, convergence, and cache acceptance` | **`#51`** (`修正收斂、來源競爭與原子發布驗收`)<br>**`#52`** (`cache 單一寫入者、失效與依賴資料驗收`) | • **`#180`** (under `#51`): stable formatting & safe-fix sequence (`fix-convergence.json`)<br>• **`#181`** (under `#51`): source-conflict detection & per-file atomic replacement (`atomic-publication.json`)<br>• **`#182`** (under `#51`): `--check`/`--diff` read-only & failure-log behavior (`check-diff-readonly.json`)<br>• **`#183`** (under `#52`): cache identity & dependency invalidation (`cache-invalidation.json`)<br>• **`#184`** (under `#52`): cache single-writer & explicit failure behavior (`cache-single-writer.json`) | Verifies safe write/fix convergence and cache isolation in `crates/mamba-quality` and records receipts under `docs/operations/python-quality/{publication,cache}/`. | `#180` -> `#167, #170, #173`<br>`#181` -> `#180`<br>`#182` -> `#178`<br>`#183` -> `#177, #176`<br>`#184` -> `#183`. Require `9725` Quality source. |
| **`#24` (Milestone Q3)** — `execute the current release on three platforms and close regression gates` | **`#53`** (`完整 461／311／API17／CLI11／language505 回歸`)<br>**`#54`** (`原有 SOM 複本與三 Python 五工具整體對照`)<br>**`#55`** (`驗證最終安裝候選不依賴 Python 或外部品質工具`)<br>**`#56`** (`macOS Apple Silicon 候選`)<br>**`#57`** (`Linux x86_64 候選`)<br>**`#58`** (`Linux ARM64 候選`) | • **`#185`** (under `#53`): `full461` (`Mypy311` + `27` semantic arrays)<br>• **`#186`** (under `#53`): `API17`, `CLI11`, `SAME24`<br>• **`#187`** (under `#53`): `language505` inventory<br>• **`#188`** (under `#53`): format, Clippy, strict, workspace regressions<br>• **`#189`–`#191`** (under `#54`): full SOM copy & 5-tool comparison for Python `3.12` (`#189`), `3.13` (`#190`), `3.14` (`#191`)<br>• **`#192`, `#194`** (under `#56`): build/identify (`#192`) & functional acceptance (`#194`) on `macos-arm64`<br>• **`#195`, `#197`** (under `#57`): build/identify (`#195`) & functional acceptance (`#197`) on `linux-x86_64`<br>• **`#198`, `#200`** (under `#58`): build/identify (`#198`) & functional acceptance (`#200`) on `linux-arm64`<br>• **`#193`, `#196`, `#199`** (under `#55`): hermetic no-Python / no-external-tool verification on `macos-arm64` (`#193`), `linux-x86_64` (`#196`), `linux-arm64` (`#199`) | Runs full final-source Quality regressions and 3-platform release candidate gates under `docs/operations/python-quality/{regression,som,platforms}/`. (Current `9725` binary `9d2eb190cef74856e1816bdddea69009c5ddd199c595e988877907d12cffe255` is `opt1/debug2`, **not** a release candidate.) | `#185`–`#188` depend on Q1/Q2 completion (`#102`–`#184`); `#189`–`#191` depend on `#185`–`#188`; `#192`–`#200` depend on `#189`–`#191`. **Independent of Runtime `#26` until the `#27` joint binary (`#221`–`#223`).** |
| **`#25` (Milestone Q4)** — `measure and meet the agreed speed and memory limits` | — | • **`#201`**: freeze performance workload and sampling record (`docs/operations/python-quality/performance/measurement-protocol.md`)<br>• **`#202`**: measure small-project quality latency and peak RSS (`small-workload.json`)<br>• **`#203`**: measure large-project quality latency and peak RSS (`large-workload.json`) | `#201` freezes the measurement protocol (`1s` target latency, cold/warm cache, CPU, raw peak RSS; RAM ceiling pending human confirmation after measurement). `#202`/`#203` execute quiet-host measurements on the 3 platform candidates. | `#201` declares `Depends on: #61` (ready on `origin/main` after `#61`).<br>`#202, #203` declare `Depends on: #201, #194, #197, #200`. |

---

### 3.5 Runtime Track Mapping (`#26`, Children `#204`–`#218`, Legacy `#1`–`#3`)

Parent tracker: **`#26`** (`Release readiness: native Mamba runtime verification and performance sign-off`).

| Issue | Title & Scope | Declared `Depends on` | Target Paths | Baseline Availability on `origin/main` (`c016c085d`) |
| :--- | :--- | :--- | :--- | :--- |
| **`#204`** | `runtime(a01): freeze the A01 support and case denominator from the Batch46 checkpoint` | `#61` | `docs/operations/runtime/a01-denominator.md` | **Ready on `origin/main` after `#61`** (audit/manifest doc only, reading retained Batch46 evidence in `/Users/chrischeng/.codex/worktrees/2a2a/mamba`; `#204` does **not** publish the `2a2a` Runtime source tree). |
| **`#205`** | `runtime(annotations): repair the two N1 annotation provenance regressions` | `#61, #204` | `crates/mamba-compiler/src/`, `crates/mamba-runtime/src/infrastructure/vm/`, `crates/mamba-runtime/tests/annotation_provenance.rs` | **BLOCKED on `origin/main`**: requires the unauthored Runtime `2a2a` source-owner handoff (Batch 45 `annotation-source-provenance-coherent-v1-installed/manifest.json` SHA `c5a69857...`). |
| **`#206`** | `runtime(annotations): implement the N2 typed annotation transport boundary` | `#61, #205` | `crates/mamba-compiler/src/`, `crates/mamba-runtime/src/infrastructure/vm/`, `crates/mamba-runtime/tests/typed_annotation_transport.rs` | Requires `#205` + Runtime `2a2a` source baseline |
| **`#207`** | `runtime(memory): release extra decorated native String owners after execution` | `#61, #204` | `crates/mamba-runtime/src/infrastructure/vm/`, `crates/mamba-runtime/tests/memory_decorated_strings.rs` | Requires Runtime `2a2a` source baseline |
| **`#208`** | `runtime(memory): retire borrowed-list native-handle and closure owners` | `#61, #204` | `crates/mamba-runtime/src/infrastructure/vm/`, `crates/mamba-runtime/tests/memory_borrowed_list_closures.rs` | Requires Runtime `2a2a` source baseline |
| **`#209`** | `runtime(memory): close the Range Filter(None) lifetime slice` | `#61, #208` | `crates/mamba-runtime/src/infrastructure/vm/`, `crates/mamba-stdlib/src/`, `crates/mamba-runtime/tests/memory_range_filter.rs` | Requires `#208` + Runtime `2a2a` source baseline |
| **`#210`** | `runtime(cleanup): close the exception-tree cleanup boundary` | `#61, #204` | `crates/mamba-runtime/src/infrastructure/vm/`, `crates/mamba-runtime/tests/exception_cleanup.rs` | Requires Runtime `2a2a` source baseline (Batch 45 `traceframe-panic-QA-v3-installed/manifest.json` SHA `51e88d63...`) |
| **`#211`** | `runtime(cleanup): order root collection and code-lease retirement at shutdown` | `#61, #210, #208` | `crates/mamba-runtime/src/infrastructure/vm/`, `crates/mamba-runtime/tests/shutdown_cleanup_order.rs` | Requires `#210, #208` + Runtime `2a2a` source baseline |
| **`#212`** | `runtime(a04): verify the force-typed boundary matrix` | `#61, #204` | `crates/mamba-compiler/src/`, `crates/mamba-runtime/src/`, `crates/mamba-runtime/tests/a04_force_typed.rs`, `docs/operations/runtime/a04-force-typed.md` | Requires Runtime `2a2a` source baseline |
| **`#213`** | `runtime(concurrency): verify task cancellation and worker shutdown` | `#61, #211, #212` | `crates/mamba-runtime/src/`, `crates/mamba-runtime/tests/concurrency_cancellation.rs`, `docs/operations/runtime/a05-concurrency.md` | Requires `#211, #212` + Runtime `2a2a` source baseline |
| **`#214`** | `runtime(a06): verify multi-file import and error behavior` | `#61, #204` | `crates/mamba-runtime/src/infrastructure/vm/module/`, `crates/mamba-runtime/tests/a06_imports.rs`, `docs/operations/runtime/a06-imports.md` | Requires Runtime `2a2a` source baseline |
| **`#215`** | `runtime(a07): execute the four public user journeys on the candidate binary` | `#61, #213, #214` | `crates/mamba-cli/tests/`, `docs/operations/runtime/a07-user-journeys.md` | Requires `#213, #214` |
| **`#216`** | `runtime(soak): verify 1,000 same-process execution and cleanup cycles` | `#61, #211` | `crates/mamba-runtime/tests/soak_1000_cycles.rs`, `docs/operations/runtime/soak-1000-cycles.md` | Requires `#211` |
| **`#217`** | `runtime(stress): verify 2, 4 and 8-worker safety stress without races or leaks` | `#61, #213` | `crates/mamba-runtime/tests/worker_stress.rs`, `docs/operations/runtime/worker-stress.md` | Requires `#213` |
| **`#218`** | `runtime(perf): record fixed native CPU and raw peak-RSS comparisons` | `#61, #215, #216, #217` | `docs/operations/runtime/performance-report.md` | Requires `#215, #216, #217` |
| **`#1`, `#2`, `#3`** | Legacy monorepo (`apps/mamba/...`) concurrency & refcount issues: `#1` (`Value::clone_val` refcount race), `#2` (`MbObject` atomic refcount header), `#3` (`TOKIO_RT` multi-thread pool) | Reconciled with `#26` (`#207`–`#213`, `#217`) | Current workspace equivalent: `crates/mamba-runtime/src/` | Must be verified/closed alongside `#26` memory/concurrency gates (`#207`–`#213`, `#217`) |

---

### 3.6 Joint Release (`#27`, Children `#61`, `#219`–`#223`) and Parked Axiom WIP (`#28`, Children `#97`–`#101`)

| Parent | Child Issue | Title & Scope | Declared `Depends on` | Target Paths & Prerequisite Boundary |
| :--- | :--- | :--- | :--- | :--- |
| **`#27`** (`release: coordinate Runtime and Quality readiness for the 0.1.0-alpha cut`) | **`#61`** | `source: map the Test, Runtime and Quality baselines before AGY handoff` | `None` | `docs/operations/agy/source-baselines.md` (this document) |
| **`#27`** | **`#219`** | `release(source): reconcile one shared-file source plan for the joint candidate` | `#61` | `docs/operations/agy/joint-source-plan.md` (ready on `origin/main` once `#61` lands; plans reconciliation of the exact shared files in §2.3 without mutating protected worktrees) |
| **`#27`** | **`#220`** | `release(version): record the human version, tag and artifact naming decision` (`0.1.0-alpha` milestone vs `0.4.13` `Cargo.toml`) | `#219` | `docs/operations/agy/release-version-decision.md` (records human version/tag/artifact naming decision; does not mutate `Cargo.toml` without explicit authorization) |
| **`#27`** | **`#221`** | `release(macos-arm64): verify the joint Runtime and Quality candidate` | `#219, #220, #194, #215, #216, #217, #218` | `docs/operations/release/joint-candidate-macos-arm64.md` |
| **`#27`** | **`#222`** | `release(linux-x86_64): verify the joint Runtime and Quality candidate` | `#219, #220, #197, #215, #216, #217, #218` | `docs/operations/release/joint-candidate-linux-x86_64.md` |
| **`#27`** | **`#223`** | `release(linux-arm64): verify the joint Runtime and Quality candidate` | `#219, #220, #200, #215, #216, #217, #218` | `docs/operations/release/joint-candidate-linux-arm64.md` |
| **`#28`** (`Re-home parked mamba WIP from axiom app/tape (ac3d7d9c74)`) | **`#97`** | `audit(axiom-wip): classify the 15 parked ac3d7d9c74 files against current Mamba crates` | `None` | `docs/operations/agy/parked-mamba-wip.md` (ready on `origin/main` right now; read-only inspection of `/Users/chrischeng/projects/axiom` commit `ac3d7d9c74`) |
| **`#28`** | **`#98`** | `compiler(axiom-wip): port approved compiler and type-checker hunks from ac3d7d9c74` | `#97` | `crates/mamba-compiler/`, `crates/mamba-core/` |
| **`#28`** | **`#99`** | `runtime(axiom-wip): port approved runtime call, class and executor hunks from ac3d7d9c74` | `#97, #98` | `crates/mamba-runtime/` |
| **`#28`** | **`#100`** | `stdlib(axiom-wip): port approved stdlib wrapper hunks from ac3d7d9c74` | `#97` | `crates/mamba-stdlib/` (`8` stdlib wrapper files) |
| **`#28`** | **`#101`** | `tests(axiom-wip): port approved conformance and performance harness hunks from ac3d7d9c74` | `#98, #99, #100` | `crates/mamba-cli/tests/` or `crates/mamba-runtime/tests/` |

---

## 4. Missing Source-Owner Handoffs and Quality-First Execution Order

### 4.1 Explicit Missing / Unauthored Source-Owner Handoffs (`unknown` / Not Yet Authorized)

Because every AGY issue worker (`issue.sh next`) branches a fresh worktree from `origin/main` (`c016c085dd00de23415033764cc239222cde331d`), **three source baselines that exist only in local commits or protected local worktrees are currently missing on `origin/main`**:

| Missing Baseline Handoff | Source Location & Identity | Currently Owned by Any Open Issue (`#1`–`#223`)? | Downstream Tickets Blocked Until This Handoff Lands on `origin/main` (or an Authorized Target Branch) |
| :--- | :--- | :--- | :--- |
| **1. Unpushed Local `main` Commits (`fdcb0aa08e` and `6881195835..611f2c86ab`)** | `/Users/chrischeng/faberlines/mamba` (`main` @ `611f2c86abe4ec27d8a74c9eda7473eeb8c6519a`, `12` commits ahead of `6e2d1ae10d`):<br>• `021537c21c..fdcb0aa08e` (`10` commits: kit relocation to `crates/*` and 300-line splits)<br>• `6881195835..611f2c86ab` (`2` commits: `crates/mamba-test` and `crates/mambalibs-unittest` landing) | **`unknown` / None** — No open issue is authorized to push or PR `fdcb0aa08e..611f2c86ab` onto `origin/main`. (`#62` only owns prepared-module execution inside `crates/mamba-test`.) | Test track **`#62`–`#69`** (`#5`, `#6`), plus any ticket expecting `crates/mamba-test`, `crates/mambalibs-unittest`, or relocated `crates/*kit*` paths on `origin/main`. |
| **2. Protected Quality Worktree (`9725`) Source Baseline (`fdc25f5b...`)** | `/Users/chrischeng/.codex/worktrees/9725/mamba` (`HEAD = fdcb0aa08ec0778ce63ec75d8b0b66f71083834a` + `4,504` dirty/untracked paths = `78,845` total files, `full_source_sha256 = fdc25f5b7fb135cbfcb15f5e24219db6b9e4e6c73b964b612e7668e5855c82f6`) | **`unknown` / None** — Explicitly paused per `/private/tmp/mamba-quality-paused-gh-todos-main-20261021-1/HANDOFF.md` (`不得commit/push/merge/發布`). Issue **`#29`** is a bounded 6-file mypy untyped-call recovery + `finalize_m1.py` generator diagnostic ticket, **not** a whole-tree `9725` publication ticket. | Quality engine/code and live execution tickets: **`#29`–`#42`**, **`#45`–`#58`**, **`#102`–`#128`**, **`#130`–`#158`**, **`#166`–`#200`**, **`#202`–`#203`**. |
| **3. Protected Runtime Worktree (`2a2a`) Source Baseline** | `/Users/chrischeng/.codex/worktrees/2a2a/mamba` (`HEAD = 611f2c86abe4ec27d8a74c9eda7473eeb8c6519a` + `991` dirty/untracked paths = `75,131` total files, status SHA `c679d43d6992970b2a6bc175740e9cfd2b5b8cec6465393256e1bab02c521528`) | **`unknown` / None** — Issue **`#204`** is a documentation-only A01 denominator freeze (`docs/operations/runtime/a01-denominator.md`), **not** a whole-tree `2a2a` publication ticket. | Runtime code, memory/cleanup, and gate execution tickets: **`#205`–`#218`** (and legacy `#1`–`#3`). |

### 4.2 Quality-First Execution Order (`Q0 -> Q1 -> Q2 -> Q3 -> Q4`) Without Blanket Runtime Gating

Per the Quality roadmap (`Q0 -> Q1 -> Q2 -> Q3 -> Q4` in `#15` and `#16`–`#25`), **Quality does not wait on the Runtime track (`#26` / `#204`–`#218`)**:

1. **Immediately Executable on `origin/main` (`c016c085d`) Without Any Protected-Worktree Source Import**:
   - **Mambalibs Wheel & Native Slices on `origin/main`**:
     - `#60` (`mambalibs.crypto` SHA-2 wheel; unblocks `#86`–`#88`)
     - `#89` (`mambalibs.media` image metadata/codecs wheel; unblocks `#90`–`#92`)
     - `#93` (`mambalibs.mongo` client connection/close wheel; unblocks `#94`–`#95`)
     - `#96` (`mambalibs.logging` invalid-argument error enforcement in `crates/log-mamba/src/methods.rs`; `#59` is already merged)
     - `#70` (`mambalibs.array` native ownership handle in `mambalibs/arraykit/binding/src/`, once `#61` lands; unblocks `#71`–`#85`)
   - **Parked Axiom WIP Audit (`#28`)**:
     - `#97` (`docs/operations/agy/parked-mamba-wip.md`; unblocks `#98`–`#101`)
   - **Quality Q0 / Q1 / Q4 Documentation & Ledger Freezes (once `#61` lands)**:
     - **Q0**: `#159`–`#164` (`#44` 5-tool + Pydantic denominator lock JSONs under `docs/operations/python-quality/`) and `#165` (`#16` `76,831`-path import provenance audit report under `docs/operations/python-quality/import-audit-76831.md`)
     - **Q1 Ledger**: `#129` (`#43` `412` incomplete-capability ledger in `docs/operations/python-quality/remaining-412-ledger.json`)
     - **Q4 Protocol**: `#201` (`#25` performance measurement protocol in `docs/operations/python-quality/performance/measurement-protocol.md`)
   - **Runtime A01 Denominator Doc & Joint Source Plan Doc (once `#61` lands)**:
     - `#204` (`docs/operations/runtime/a01-denominator.md` from read-only `2a2a` Batch46 evidence)
     - `#219` (`docs/operations/agy/joint-source-plan.md`) and `#220` (`docs/operations/agy/release-version-decision.md`)

2. **Quality Engine & Gate Execution (`Q1 -> Q2 -> Q3 -> Q4`, Gated Only on Quality Source Handoff `fdc25f5b...`, NOT on Runtime `#26`)**:
   - **Prerequisite**: Explicit human/source-owner handoff of the Quality `9725` baseline (`fdcb0aa08e` + `fdc25f5b7fb135cbfcb15f5e24219db6b9e4e6c73b964b612e7668e5855c82f6`).
   - **Step Q1a (Mypy Proposals)**: `#29` (`untyped6` + `finalize_m1.py:39`), `#30` (`methodtyping3`), and `#31` (finish & review `DictComp12` draft against counterfactual `815f2d07...`), verified via `Clippy -> CLI11 -> SOM217`.
   - **Step Q1b (Mypy SOM217 Diffs & 412 Obligations)**: `#32`–`#42` (`#102`–`#128` closing the `79` SOM217 diffs) and `#43` (`#130`–`#158` closing the `412` incomplete capability obligations after `#129`).
   - **Step Q2 (Per-Tool & Public Contract Parity)**:
     - `#18` / `#45` (`#166`, `#167` Ruff unknown-rule & encoding/safe-fix evidence)
     - `#19` / `#46` (`#168`–`#170` isort CLI/skip/108-row chain)
     - `#20` / `#47`, `#48` (`#47` + `#171`–`#173` Black 174 ASTSafety join, 313/396/51 classification, structure & layout parity)
     - `#22` / `#49` (`#174`–`#176` Pylint process/completion, 383-message ledger, cross-file inference)
     - `#17` / `#50` (`#177`–`#179` discovery, `API17`/`CLI11`, source-byte/location contracts)
     - `#23` / `#51`, `#52` (`#180`–`#184` convergence, atomic publication, `--check`/`--diff` read-only, cache single-writer & invalidation)
   - **Step Q3 (Full Quality Regressions & 3-Platform Standalone Quality Candidates)**:
     - `#24` / `#53` (`#185`–`#188`: `full461`/`Mypy311`, `API17`/`CLI11`/`SAME24`, `language505`, format/Clippy/strict/workspace)
     - `#24` / `#54` (`#189`–`#191`: full SOM copy & 5-tool parity across Python `3.12`, `3.13`, `3.14`)
     - `#24` / `#55`–`#58` (`#192`–`#200`: standalone Quality candidates and no-Python hermetic checks on `macos-arm64`, `linux-x86_64`, `linux-arm64`)
   - **Step Q4 (Quality Speed & Memory Acceptance)**:
     - `#25` (`#202`, `#203` small and large project latency/RSS measurements after `#201` and `#194, #197, #200`).

3. **Suggested Non-Blocking Cross-Track Source Reconciliation Order (`#219`)**:
   - *Verified overlap boundary*: As proven in §2.3, Quality (`9725`), Test (`fdcb0aa08e..611f2c86ab`), and Runtime (`2a2a`) only overlap on `6` files (`Test vs Quality`), `1` file (`Test vs Runtime`), and `12` files (`Runtime vs Quality`: `5` in `crates/mamba-runtime` where `9725` has the older Primary copy and `2a2a` is authoritative, plus `7` in `crates/mamba-language` requiring a 3-way merge).
   - *Suggested non-blocking reconciliation sequence for `#219`* (for the final `#27` joint binary `#221`–`#223`, without blocking independent Q0–Q4 Quality work):
     1. Reconcile the `10` shared kit-relocation/split commits (`021537c21c..fdcb0aa08e`) with any `origin/main` Mambalibs commits (`#59` `c016c085d`, `#60`, `#70`–`#96`).
     2. Land or replay the `2` Test product commits (`6881195835..611f2c86ab`) to unblock `#62`–`#69`, using the already-proven `6`-file reconciliation in `docs/migrations/test-tool/landing/evidence/primary-preservation.json` (`Cargo.lock`, `Cargo.toml`, `crates/mamba-cli/Cargo.toml`, `crates/mamba-cli/src/entry.rs`, `crates/mamba-cli/tests/it/main.rs`, `ddd.toml`).
     3. When assembling the joint Runtime + Quality candidate (`#219` -> `#221`–`#223`), take `2a2a`'s version for the `5` shared `crates/mamba-runtime/src/infrastructure/vm/module*` files and perform a 3-way merge on the `7` shared `crates/mamba-language` float/literal/type_expr files.
