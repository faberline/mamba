# Quality cache identity evidence for issue #183

Date: 2026-10-24. This document records existing source and test evidence. It does not change product code or close #183.

## Contract and scope

Issue [#183](https://github.com/faberline/mamba/issues/183) covers cache identity for source bytes, dependency data, Python target, the fixed profile, and executable identity. A change to a required identity must cause a miss. Only complete reports may be stored. The required author and reviewer gates are an unfiltered library test list and a full unfiltered library test run at the final candidate HEAD. The issue text names no separate historical cache case list, command, binary, raw artifact, or approval variant. If a protected handoff names additional original variants, bind and run those exact variants before acceptance; this document does not replace them.

The retained #182 candidate and its merged-main commit `464476a9cd2676e72d9be802a770a027ec51036d` have the same whole Git tree, as independently checked in the retained #182 Main audit. These historical product-source identities are separate from this document candidate. The source tree SHA-256 is `692c7d875d1c231b0a02fe698af4586d39c491b346a9896b3d47cc73e32e6fdb`; its source-map SHA-256 is `20388c856794b169d085e06e72cb3386ce44041130294057b8c2dea2ae92bdab`. The audit rehashed 57,286 paths and found matching bytes, modes, and paths.

## Five identity axes

| Axis | Existing assertion | Evidence limit |
| --- | --- | --- |
| Source bytes | `never_reuses_a_diagnostic_after_its_source_changes` edits a source, loads a fresh workspace, and observes that Native Ruff no longer returns the old diagnostic. `explicit_and_scanned_source_origins_invalidate_the_key` checks that different source request origins produce different keys. | A fresh load changes the snapshot SHA. The key uses the SHA of that immutable snapshot. |
| Dependency data | `dependency_target_profile_and_executable_changes_invalidate_the_key` changes `.pyi` bytes. `namespace_directories_and_native_module_data_invalidate_the_key` changes namespace-directory and `.so` inputs. `dependency_change_during_check_does_not_publish_stale_result` checks that a synthetic engine does not publish a report after dependency change during the check. | The first two are key assertions. The last is adapter write-admission behavior, not Native Ruff output. |
| Python target | `dependency_target_profile_and_executable_changes_invalidate_the_key` changes the project Python version. | Key assertion. |
| Fixed profile | The same test changes the locked Ruff rule set. | Key assertion. |
| Executable | The same test changes the supplied executable identity from `binary-a` to `binary-b`. Production cache wiring hashes current executable bytes. | The test does not build or compare two real executable files. |

`Filesystem.load()` builds an immutable `SourceSnapshot` from source bytes, decoded text, and their SHA-256. Native Ruff checks those stored bytes. If the disk file changes after load, a check using that workspace still checks the old snapshot, both with and without a cache hit. A later fresh load gets the new SHA and a different key. Hashing newer disk bytes into the old snapshot key could store the old report under a key for new bytes. The separate source publication guard checks the current file before replacing it. The independent snapshot review found no demonstrated disk-after-load cache defect and recommends no patch or new test for that scenario.

## Exact retained tests and raw rows

The following 13 cache-module tests are present exactly once in the retained full Main output. Each row ended `ok`. The paths and file hashes identify the test source on the tested tree.

| Full-output row | Test | Source file (SHA-256) | What it supports |
| ---: | --- | --- | --- |
| 44 | `admission::concurrent_checks_share_one_writer_and_warm_check_reuses_complete_report` | `cache/tests/admission.rs` (`0b27fb4fc456acade4411ff45820a9172747e3f8b6ce70ba2a5e4abde10155f7`) | Warm reuse through `ProbeEngine`; not Native Ruff warm reuse. |
| 45 | `admission::dependency_change_during_check_does_not_publish_stale_result` | `cache/tests/admission.rs` (`0b27fb4fc456acade4411ff45820a9172747e3f8b6ce70ba2a5e4abde10155f7`) | Synthetic-engine write-admission guard; zero report entries after changed dependency. |
| 46 | `admission::incomplete_report_never_becomes_a_warm_cache_hit` | `cache/tests/admission.rs` (`0b27fb4fc456acade4411ff45820a9172747e3f8b6ce70ba2a5e4abde10155f7`) | Incomplete reports do not become warm hits. |
| 47 | `admission::subprocess_entry_claim_respects_live_writer_and_claims_after_release` | `cache/tests/admission.rs` (`0b27fb4fc456acade4411ff45820a9172747e3f8b6ce70ba2a5e4abde10155f7`) | Single-writer claims; primarily #184. |
| 48 | `dependency_target_profile_and_executable_changes_invalidate_the_key` | `cache/tests.rs` (`5f02296b9c2f4ac8b642572ca57679b2dd49153cae225aa8bea7a7389f14ddbd`) | Dependency, Python target, fixed profile, and supplied executable identity key changes. |
| 49 | `explicit_and_scanned_source_origins_invalidate_the_key` | `cache/tests.rs` (`5f02296b9c2f4ac8b642572ca57679b2dd49153cae225aa8bea7a7389f14ddbd`) | Source request origin changes the key. |
| 50 | `namespace_directories_and_native_module_data_invalidate_the_key` | `cache/tests.rs` (`5f02296b9c2f4ac8b642572ca57679b2dd49153cae225aa8bea7a7389f14ddbd`) | Namespace directory and native module dependency data affect the key. |
| 51 | `negative::cache_symlinks_do_not_write_external_data` | `cache/tests/negative.rs` (`01349b42c5e3ff0f403b28eae18e5ca0d6a356f4c0bd0ff912c7061ab75c49d3`) | Symlink safety; primarily #184. |
| 52 | `negative::report_path_symlink_is_a_miss_and_external_bytes_survive` | `cache/tests/negative.rs` (`01349b42c5e3ff0f403b28eae18e5ca0d6a356f4c0bd0ff912c7061ab75c49d3`) | Symlink miss and external-byte preservation; primarily #184. |
| 53 | `negative::wrong_identity_tool_version_count_and_completion_are_cache_misses` | `cache/tests/negative.rs` (`01349b42c5e3ff0f403b28eae18e5ca0d6a356f4c0bd0ff912c7061ab75c49d3`) | Cache envelope identity and completion rejection. |
| 54 | `never_reuses_a_diagnostic_after_its_source_changes` | `cache/tests.rs` (`5f02296b9c2f4ac8b642572ca57679b2dd49153cae225aa8bea7a7389f14ddbd`) | Native Ruff invalidation after a fresh source load. |
| 55 | `one_writer_owns_an_entry_and_a_corrupt_report_is_a_miss` | `cache/tests.rs` (`5f02296b9c2f4ac8b642572ca57679b2dd49153cae225aa8bea7a7389f14ddbd`) | Writer and corrupt-entry behavior; primarily #184. |
| 56 | `provenance::mypy_cache_keeps_request_order_aliases_and_package_anchors` | `cache/tests/provenance.rs` (`ad27709bc72c140d801caf078a24760719c9a84d0dcf48d3024fc67d4b5303d5`) | Mypy request provenance changes keys; it does not prove a Mypy warm report. |

One separate app test also passed at full-output row 4: `app::tests::writing_fix_keeps_the_existing_complete_report_cache`, source `app/tests.rs` SHA-256 `5df4fd1a8104fbdb39525b21eed9f825e96479ff64d75960dabadf9913971754`. It supports public writing-Fix cache creation. It is not one of the 13 cache-module tests and does not prove Native Ruff warm reuse.

Four cache tests primarily belong to #184: the writer/corrupt-report case, subprocess claim case, cache-symlink case, and report-path symlink case. #184 separately covers missing dependency data and explicit failure. Native engine code currently marks only Ruff complete. Other engine results can remain incomplete and bypass warm-cache admission. These receipts therefore do not prove five-tool warm cache, full Mypy caching, SOM217, or the broader parent #52 contract.

## Retained Main run and limits

The retained Main run is prior coverage on source HEAD `61a6996dccea229ee7fd6caa38ad1fbb10e416ff`, whose whole Git tree matches the #182 merged-main commit `464476a9cd2676e72d9be802a770a027ec51036d`. Its exact full command was:

```text
/private/tmp/mamba-controller-sol-luna-20261023/rust-1.97-complete/bin/cargo test --offline --locked -j 1 -p mamba-quality --lib -- --test-threads=1
```

The command ran in `/private/tmp/mamba-controller-sol-luna-20261023/quality182-dryrun-cache-repair-1`. Its recorded overrides set `CARGO` and `RUSTC` to the pinned Rust 1.97 toolchain, `CARGO_HOME=/private/tmp/mamba-controller-sol-luna-20261023/cargo-home`, `CARGO_TARGET_DIR=/private/tmp/mamba-controller-sol-luna-20261023/quality182-author-target-1`, `CARGO_BUILD_JOBS=1`, `RAYON_NUM_THREADS=1`, `RUST_TEST_THREADS=1`, and a PATH beginning with the pinned toolchain. The receipt states that inherited environment values were not recorded; it is not a complete environment dump.

The list stage ran as PID 10578 and the full stage as PID 10765. Both have actual waits, exit 0, and raw stdout/stderr hashes. The full stage listed and executed 476 tests with zero failed, ignored, measured, or filtered. The supervisor (PID 9942; driver PID 9945) was waited and exited 0. The same unit binary was captured for both stages: `/private/tmp/mamba-controller-sol-luna-20261023/quality182-author-target-1/debug/deps/mamba_quality-8bc6104e3772d555`, SHA-256 `32ce1318b95d29f2ac5737835dfe5295226ea6f4463214b60a6cf8d98f3dc838`.

The raw list SHA-256 is `45e487d1acc3539eb003e0099c6361aab9eb1cf39d88113b992cc98780b20cb6`. The full stdout SHA-256 is `96ab8ebc260860dc12c647ddbafc4688d9bb696fdd100efb4215a949afc9443f`; stderr SHA-256 is `81746d972da70d02c9f8fd85d9775605804e7f18f2debc37cd864d5846dbe515`. The full-source before and after maps match at SHA-256 `692c7d875d1c231b0a02fe698af4586d39c491b346a9896b3d47cc73e32e6fdb` across 57,286 rehashed files.

These retained results show that the listed cases passed on the matching source tree. They are not the final #183 candidate author/reviewer runs. The final candidate HEAD is not frozen here, and the final author and reviewer gates remain **NOT RUN**. Keep #183 and parent #52 open until those exact receipts and any separately named protected variants are independently accepted. No native command was run for this document.
