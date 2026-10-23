# Native Quality checkpoint（2026-10-23）

此版把 Rust 原生 Quality 程式接到現有 `mamba quality` 命令。這是可接續的版本。它不代表整個 Quality 計畫或所有 issues 已完成。

## 本版範圍

基線為 `6afecc56b0e28bbca3aa5aecfbf7a3dd2feb8203`。本版加入 `mamba-quality`、`mamba-language`、`mamba-shared-kernel` 與兩個必要的 layout vendor 套件。保留 vendor 與內嵌資料的授權檔及來源聲明。沒有導入本機其他 Runtime、Compiler、Test 或 Mambalibs 工作。

CLI 以薄接線呼叫 Quality 服務，並在 compiler runtime 初始化前處理 `quality` 命令。原命令定義、輸出程式與 11 個 CLI 測試保持原始位元組。公開退出碼維持 0／1／2。版本維持 0.4.13。

根 package 仍在工作區內。Cargo 的 `members` 移除多餘的 `"."`，避免與獨立 vendor 工作區衝突。原有成員保留，新增三個 Quality 必要 crate，兩個 vendor 仍排除於工作區。

## 本版驗證

驗證須在此提交的來源上執行。命令、來源身份、實際退出碼、案例數及證據摘要記錄在本版 PR。歷史候選的通過結果不能轉給此版。完整 Quality 驗收仍依原 issue 契約。

在倉庫根目錄使用 Rust 1.97。必要的 focused checkpoint 命令如下。這裡列命令，不預先宣告結果。

```sh
cargo test --offline --locked -j 1 -p mamba-quality --lib -- --test-threads=1
cargo clippy --offline --locked -j 1 -p mamba-quality --lib --tests -- -D warnings
cargo test --offline --locked -j 1 -p mamba-quality --test it -- --test-threads=1
cargo test --offline --locked -j 1 -p mamba-language --lib -- --test-threads=1
cargo test --offline --locked -j 1 -p mamba --test quality_command -- --test-threads=1 --nocapture
```

原 CLI11 包含 12 個有效 byte-path 控制、24 個 fatal exit=2 案例及 22 份外部 log 義務。APFS 上 6 個不能建立的無效位元組路徑須明列 `not_run`，不能算成通過。完整平台、全工作區、三 Python 五工具、SOM 與效能仍依各票驗收。

## 歷史結果

舊本機 rev5 的完整來源 SHA-256 為 `62dd06a7156137ec098427a2bf1c10c3ae5783315985f2b79811ceb3620e4e64`。作者與 Main 各自完成未過濾的 474 個 library tests，均通過。Main 的 Clippy 實際退出 101，該版未通過整體門檻。

舊本機 rev6 只修正測試中的 `thread_local` const 初始化。其來源 SHA-256 為 `1c84c8c09e400b892995534d6777a92591dbe4b99326b8759e9eab944bbcb3f2`。診斷性 Clippy 退出 0。這些歷史記錄不能代替此版乾淨 main 分支的獨立驗證。

[#129](https://github.com/faberline/mamba/issues/129) 與 [PR #227](https://github.com/faberline/mamba/pull/227) 只固定 412 項 incomplete 的歷史索引，沒有修復該 412 項行為。

## 未完成的工作

- [#29](https://github.com/faberline/mamba/issues/29)、[#30](https://github.com/faberline/mamba/issues/30)、[#31](https://github.com/faberline/mamba/issues/31)：完整 mypy 整合與 SAME217 原門檻。
- [#41](https://github.com/faberline/mamba/issues/41)、[#42](https://github.com/faberline/mamba/issues/42)、[#102](https://github.com/faberline/mamba/issues/102)、[#103](https://github.com/faberline/mamba/issues/103)：指定原始有序診斷列的差異。
- [#130](https://github.com/faberline/mamba/issues/130)–[#132](https://github.com/faberline/mamba/issues/132)：strict summary、generic alias、Pydantic 原始 incomplete inventory。
- [#47](https://github.com/faberline/mamba/issues/47)：以最終整合 binary 重驗 Black ASTSafety 174 案例。原 Black 退出 123，Mamba process 與 JSON 退出 2 分開記錄。原版位置未知時保留 unknown。
- [#159](https://github.com/faberline/mamba/issues/159)–[#164](https://github.com/faberline/mamba/issues/164) 及 [#16](https://github.com/faberline/mamba/issues/16)：六工具原始證據、初始 import map 和缺件。
- [#228](https://github.com/faberline/mamba/issues/228)：恢復 SAME217 原 Python 3.12.11 環境缺少的 52 個原檔，再核對完整 9335 entries。
- [#229](https://github.com/faberline/mamba/issues/229)：核對舊 editable som-core 的 32 個 src 檔加 pyproject.toml。恢復候選的 33/33 metadata 相符，原 absolute path 與當時查找的歷史連結仍缺。
- [#53](https://github.com/faberline/mamba/issues/53)、[#54](https://github.com/faberline/mamba/issues/54) 與其小票：完整回歸、SOM 三 Python 五工具、平台與效能。

SAME217 已保留 217 個有序輸入與 4034 筆九欄原版結果。兩份舊 reviewer inputs 仍缺。原始證據不能以新跑結果、新 venv 或同名不同 hash 的候選替代。

未執行為 `not_run`，身份或程序結果不明為 `unknown`。零案例、未 wait、未核實 exit 或局部通過，都不能當成完整驗收。此 checkpoint 不關閉上述未完成項目。
