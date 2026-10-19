# TreeDock — Master Plan

**Status:** Plan v3. Licensing and CLA decisions finalised; ASPEC CLAs v1.0 final; PactDock integrated, awaiting activation (§11.4); implementation in progress (see §12).
**Owner:** ASPEC RACING PTY LTD (Australia)
**Date:** 2026-10-20
**Scope:** This document covers research, architecture and the roadmap. Implementation starts only after the plan is approved.

---

## 0. How to read this document

Claims are tagged by how sure we are of them:

| Tag | Meaning |
|---|---|
| **[C]** | Confirmed against the cited source (official documentation, the product's own site, a source repository, crates.io) during research on 2026-10-20. |
| **[K]** | Well-established technical knowledge that was not re-verified against a source during this research pass. Verify before relying on it for anything critical. |
| **[A]** | Assumption or inference. It is a reasonable working hypothesis, not a fact. |
| **[P]** | A decision or claim that needs a **prototype or benchmark** before it is final. These are collected in §12 Phase 0 and §13. |

The document makes no performance promises. Every number in §9 is either a measurement target to validate or a cited third-party figure.

---

## 1. Vision, objectives and non-goals

### 1.1 Vision

TreeDock is a fast, accurate, modern Windows disk-space analyser. It is free of charge and **source-available under the PolyForm Noncommercial License 1.0.0** (see §11.1; note this is not an OSI-approved open-source licence). It explains *where space went* and *why the numbers add up (or don't)*. It also lets users act on what they find safely, without paywalls, accounts, telemetry or ads.

### 1.2 Objectives

1. **Speed.**
   - Elevated, local NTFS: read the Master File Table (MFT) directly.
   - Everywhere else: a highly parallel directory walker tuned to the storage type.
2. **Accuracy and honesty.**
   - Report logical, allocated and unique (hard-link-deduplicated) sizes.
   - Reconcile the scan against volume used space and explain the gap ("unaccounted space").
   - State filesystem limitations explicitly, e.g. ReFS block cloning or OneDrive placeholders.
3. **Responsiveness.**
   - Results appear progressively.
   - The UI stays fluid with tens of millions of entries.
   - Scans can be cancelled at any time.
4. **No paywalled features.** Snapshots, history, duplicates, CLI, CSV/JSON/HTML/PDF export and scheduled scans (via the user's own Task Scheduler entry) are all free for every permitted user. The licence (PolyForm Noncommercial 1.0.0) permits personal and noncommercial-organisation use; **commercial use is not granted by the licence** (§11.1).
5. **Native Windows 11 experience.** Fluent design, Mica, dark/light/high-contrast themes, per-monitor DPI, full keyboard operation and screen-reader support.
6. **Safety.** Nothing is ever deleted automatically. Recycle Bin is the default. Every destructive action is re-validated and confirmed.
7. **Privacy and offline operation.** No network access at all in normal operation, and no telemetry.

### 1.3 Non-goals (for 1.0)

- **Cross-platform.** macOS and Linux are out of scope. The core engine stays platform-neutral where it costs nothing, so a port remains possible.
- **Cloud storage APIs.** SharePoint, S3, Google Drive and Azure scanning (TreeSize Pro features [C]) are out of scope. Locally synced cloud folders *are* in scope and are handled as placeholders.
- **Deleted-file recovery and forensics.** WinDirStat 2.9 added this [C]. It is outside the core mission and carries data-safety risk.
- **A permanently running background service or indexer.** Monitoring happens only while the app is open. A scheduled scan is a user-created Task Scheduler job that runs the CLI.
- **Built-in "cleaner" heuristics that act on their own.** Suggestions are allowed. Actions always require explicit approval.
- **Auto-update over the network.** TreeDock may open the releases page in the browser when the user asks, but the app itself makes no network calls.
- **Plugin or scripting system.** This is deferred. The design should allow it later (see §3.8).

---

## 2. Competitor research and feature comparison

### 2.1 Summary of findings

**TreeSize Free 4.8.1 / Personal & Professional 9.9.1 (JAM Software)** [C]

Sources: https://www.jam-software.com/treesize_free/editions.shtml · https://www.jam-software.com/treesize/prices.shtml · https://www.jam-software.com/treesize/changes.shtml · https://www.jam-software.com/treesize_free/changes.shtml

- **Free edition:** private, non-commercial use only. It has the treemap, regex filtering, long paths, PDF export and faster MFT scanning when elevated (since 4.2).
- **Paid editions** (Personal €20.40/user/year; Professional from €41/user/year) are where these features live:
  - CSV/Excel/HTML/SQLite export
  - duplicate search
  - snapshot comparison and scan history
  - hard-link deduplication
  - the NTFS permissions view
  - the command line, scheduled scans and the portable install (Pro only)
- **Recent changes:**
  - 4.8 added *opt-out anonymous telemetry*.
  - 9.9 added an AI help chatbot.
  - Windows 8/8.1 support was dropped.
- **Takeaway:** the most valuable features sit behind a subscription. That is TreeDock's clearest opening.

**WizTree 4.33 (Antibody Software, closed source)** [C]

Sources: https://diskanalyzer.com/ · https://diskanalyzer.com/guide · https://diskanalyzer.com/whats-new · https://diskanalyzer.com/donate

- **Scanning:**
  - It reads the NTFS MFT directly. The fast mode is "only available when running as administrator".
  - Non-admin, non-NTFS, network and folder scans use a slower path.
  - Hard-link detection needs admin rights.
- **Features:** CSV export/import, a CLI export, treemap and file-type view.
- **Duplicates** are matched by **name+size(+date) only, with no content hashing** [C].
- **Licence:** free for personal use. Commercial use needs a paid "Supporter Code" ($25 to $1,800 depending on staff count).
- **Platforms:** a macOS edition was introduced in 4.33. The Windows on Arm build runs under emulation.
- **Speed claim:** "46x faster than WinDirStat" is a vendor claim with no published method [C that the claim exists; method unknown].

**WinDirStat 2.9.3 (GPL-3.0-or-later, C++, very active)** [C]

Sources: https://github.com/windirstat/windirstat · https://raw.githubusercontent.com/windirstat/windirstat/master/CHANGELOG.md

- **Feature timeline:**
  - 2.0: hash-based duplicates
  - 2.5: elevated direct MFT scanning, dark mode, CLI
  - 2.6: JSON save/load and age filters
  - 2.7: XXH3 hashing, permissions scan, removal of empty folders
  - 2.8: sunburst, flame graph and Hilbert treemap views, plus a file watcher
  - 2.9: sparse "probable duplicate" hashing, MTP scanning and deleted-file recovery; also a UI move from MFC to Win32/GDI
- **Release cadence:** roughly monthly.
- **Open issues show what users want:**
  - path tooltips
  - created/accessed columns
  - a file-types panel that follows the current selection (#228, #719)
  - cancellable mass delete (#663)
  - a duplicates tree (#656)
  - hard-link size splitting (#448)
  - reading offline MFT dumps (#406)
  - less settings clutter (#697)
- **Takeaway:** this is **the strongest free/OSS feature competitor.** Its UI technology is classic Win32/GDI.

**FolderSize v0.1.0 (TalionOfRivia, MIT, Rust/egui)** [C]

Source: https://github.com/TalionOfRivia/FolderSize

- Created 2026-10-15. It has 4 commits.
- **Stack:** egui/eframe, rayon fork-join over `std::fs::read_dir`, windows-sys and the `trash` crate.
- **Data model:** an arena `Vec<Node>` with u32 parent indices.
- **Limitations:**
  - No MFT path.
  - On-disk size is estimated by rounding up to **4096 bytes, not the real cluster size**.
  - Hard links are counted once per link.
  - No duplicates, snapshots or history.
- **Value to TreeDock:** a useful *reference point* for a minimal Rust design. TreeDock does **not** fork or reuse it.

**Other relevant tools** [C unless noted]

| Tool | Licence | Notes relevant to TreeDock |
|---|---|---|
| Squirrel Disk 2.6.1 (adileo/squirreldisk) | AGPL-3.0 | Cross-platform Rust (egui) rewrite with live sunburst/treemap during scan, CLI JSON and SSH/rclone. **Closest OSS GUI rival.** |
| Everything 1.4.1 / 1.5 beta (voidtools) | Freeware | MFT + USN index kept in memory, with optional folder-size indexing. Its SDK is an IPC client, and the SDK licence was not confirmed. |
| SpaceSniffer 2.2 | Freeware | Treemap with tags and filters. [A] Effectively unmaintained. |
| dua-cli 2.46 (MIT), dust 1.2.6 (Apache-2.0), gdu 5.38 (MIT), pdu 0.24 (Apache-2.0), diskus 0.9 (Apache-2.0) | OSS | Parallel CLI walkers. gdu has `--sequential` for HDDs, JSON import/export and SQLite persistence. pdu has optional hard-link dedupe. |
| Czkawka 12 → Krokiet (Slint) | MIT / GPL-3.0 | Duplicate finder using size → hash. Similar images. Krokiet is GPL because of Slint. |
| dupeGuru 4.3.1 | GPL-3.0 | Last release July 2022. |
| Filelight (KDE) | GPL-2.0+ | Radial map. On the Microsoft Store. |
| UltraSearch (JAM) | Free/Pro | MFT search without a background index. |

**Published benchmarks.**
- No controlled, current, reproducible Windows benchmark of WizTree vs TreeSize vs WinDirStat 2.9 vs Everything was found.
- gdu's README has Linux numbers only (https://github.com/dundee/gdu). They do not transfer to NTFS.
- **Opportunity:** publish a reproducible benchmark harness and results (§9).

### 2.2 Feature comparison

✓ = available free · $ = paid tier only · ~ = partial · ✗ = absent. TreeDock column = planned (target phase).

| Capability | TreeSize Free | TreeSize Pro | WizTree | WinDirStat 2.9 | Squirrel Disk | **TreeDock (plan)** |
|---|---|---|---|---|---|---|
| Free for commercial use | ✗ | $ | $ | ✓ | ✓ (AGPL) | ✗ (noncommercial licence) |
| Source available | ✗ | ✗ | ✗ | ✓ GPL (OSI) | ✓ AGPL (OSI) | ✓ PolyForm NC (not OSI) |
| MFT fast scan (admin) | ✓ | ✓ | ✓ | ✓ | ✗ [A] | ✓ (P2) |
| Tuned non-admin scan | ~ | ~ | ~ | ~ | ✓ | ✓ (P1) |
| Logical vs allocated, real cluster size | ✓ | ✓ | ✓ | ✓ | ? | ✓ (P1–2) |
| Hard-link dedupe | ✗ | $ | admin | ~ (#448) | ? | ✓ (P2) |
| Cloud placeholder awareness | ? | ? | ? | ? | ? | ✓ (P2) |
| "Unaccounted space" reconciliation | ✗ | ✗ | ~ | ✗ | ✗ | ✓ (P2) |
| Treemap | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ (P3) |
| Sunburst | ✗ | ✗ | ✗ | ✓ | ✓ | ✓ (P4) |
| Content-hash duplicates | ✗ | $ | ✗ | ✓ | ✗ [A] | ✓ (P4) |
| Snapshots / comparison / history | ✗ | $ | ✗ | ~ JSON | ✗ | ✓ (P5) |
| USN incremental refresh | ✗ | ? | ✗ | ✗ [A] | ✗ | ✓ (P5) |
| CLI | ✗ | $ | ✓ | ✓ | ✓ | ✓ (P1) |
| CSV / JSON / HTML / PDF | PDF only | $ | CSV | CSV/JSON | JSON | ✓ (P1/P5) |
| AI-ready summary export | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ (P5) |
| Telemetry | opt-out | opt-out | ? | ✗ [A] | ✗ [A] | **never** |
| Native ARM64 build | ? | ? | emulated | ? | ? | ✓ (P6) |

`?` = not determined during research. These cells must not be used in marketing.

### 2.3 Differentiation opportunities (ranked)

1. **Every feature free for personal and noncommercial use, with the source published.** TreeSize Free and WizTree are also free for personal use but closed source and feature-limited (TreeSize). Commercial use is *not* a differentiator under the chosen licence: WinDirStat (GPL) and Squirrel Disk (AGPL) allow it, TreeDock does not (§11.1).
2. **Honest accounting.** A "Why doesn't this add up?" panel reconciles the scan with volume used space and lists hard links, VSS, metadata, inaccessible folders, ReFS clones and placeholders.
3. **Incremental refresh via the USN journal** combined with **snapshot diffs** ("what grew since last week?"). [A] No free GUI tool appears to own this.
4. **Selection-linked views.** The treemap, sunburst, extension stats and top files all follow the current selection. This directly answers WinDirStat #228/#719.
5. **Native Windows 11 UI** with real accessibility, unlike the egui/GDI competitors.
6. **A reproducible public benchmark** and a correctness test suite.
7. **Safety-first file management:**
   - re-validation before delete
   - Restart Manager "who is locking this?"
   - cancellable bulk operations (answers WinDirStat #663)
   - a staging list ("Collector")

---

## 3. Recommended technology stack and architecture

### 3.1 Options evaluated

Scores are 1–5, weighted, and are judgement [A] based on the cited facts below.

| Criterion (weight) | Rust + WinUI 3 (C#, AOT) | Rust + WPF | Rust + Tauri 2 | Pure Rust (Slint) | Pure Rust (egui) | Avalonia | Qt |
|---|---|---|---|---|---|---|---|
| Win11 look / Mica (3) | 5 | 4 | 3 | 3 | 2 | 3 | 2 |
| Million-row virtualised list (3) | 3* | 5 | 3 | 3 | 4 | 3 | 5 |
| Treemap/sunburst rendering (2) | 5 | 3 | 4 | 4 | 4 | 4 | 4 |
| Accessibility (3) | 4 | 5 | 4 | 3 | 2 | 3 | 4 |
| Portable + small (2) | 3 | 2 | 4 | 5 | 5 | 3 | 2 |
| Windows 10 support (1) | 5 | 5 | 4 | 5 | 5 | 5 | 5 |
| Ecosystem / risk (2) | 3 | 5 | 4 | 3 | 4 | 3 | 4 |
| **Weighted total /80** | **64** | **67** | **58** | **57** | **55** | **51** | **58** |

\* Only when a flattened virtual list is used instead of `TreeView` (see below).

**Key evidence behind the scores:**

- **WinUI 3 / Windows App SDK.**
  - Current stable release is 2.5.1 (Sep 2026). Minimum OS is Windows 10 1809.
  - Unpackaged self-contained (xcopy) deployment is supported.
  - Native AOT has been supported since 1.6, with Microsoft reporting about 50% faster startup and about 2× smaller packages in a sample [C].
  - Sources: https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/release-notes/windows-app-sdk-2-0 · https://learn.microsoft.com/windows/apps/windows-app-sdk/system-requirements · https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/self-contained-deploy/deploy-self-contained-apps · https://blogs.windows.com/windowsdeveloper/2024/09/04/whats-new-in-windows-app-sdk-1-6/
  - **The WinUI `TreeView` does not scale.** Issue #818 was closed as "not planned". There are related virtualisation bugs #8275, #9549 and #2863 [C]: https://github.com/microsoft/microsoft-ui-xaml/issues/818
  - Win2D (`CanvasControl` / `CanvasVirtualControl`) is available for custom drawing [C]: https://learn.microsoft.com/windows/apps/develop/win2d/hellowin2dworld
- **WPF.**
  - The .NET 9 Fluent theme (`ThemeMode`) exists, but setting it from code is still experimental [C]: https://learn.microsoft.com/en-us/dotnet/desktop/wpf/whats-new/net90
  - **WPF cannot be trimmed or Native-AOT compiled** [C]: https://learn.microsoft.com/dotnet/core/deploying/trimming/incompatibilities
  - That means a large self-contained output, or a dependency on the installed .NET Desktop Runtime.
- **Tauri 2.**
  - WebView2 ships with Windows 11. Windows 10 may need it installed, or a large Fixed Version runtime [C]: https://learn.microsoft.com/microsoft-edge/webview2/concepts/evergreen-vs-fixed-version
  - Community measurements show roughly 200 ms to move 10 MB over binary IPC on Windows, and maintainers see limited room for improvement [C, community]: https://github.com/orgs/tauri-apps/discussions/11915
  - [A] WebView memory overhead is 100–200 MB.
- **Slint.**
  - Licence choice is GPLv3, Royalty-free 2.0 (with attribution, plus an ambiguous "exposes the APIs" clause), or commercial [C]: https://cdn.jsdelivr.net/gh/slint-ui/slint@1.16.0/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md
- **egui.**
  - AccessKit provides UI Automation [C]: https://github.com/AccessKit/accesskit
  - The look is non-native. Accessibility depth is lower than XAML [A].
- **Iced.** The accessibility issue #282 has been open since 2020 [C].
- **gpui.** Pre-1.0 and not accessible yet [C].
- **Avalonia.** **TreeDataGrid moved to a commercial tier** [C]: https://avaloniaui.net/blog/building-a-sustainable-future-for-avalonia
- **Qt.** LGPL dynamic-linking obligations apply. There is C++/Rust FFI overhead and the look is non-Win11 [C/A].

### 3.2 Recommendation

**The engine is Rust. The desktop UI is WinUI 3 (C#, .NET 10 LTS, Native AOT, unpackaged and self-contained). The two are connected through a narrow, blittable C ABI generated with csbindgen.** WPF is the documented fallback.

Rationale:

- **Engine in Rust.**
  - Memory-safe parsing of untrusted on-disk structures (MFT records from any USB stick).
  - Predictable memory layout for compact indexing.
  - Excellent parallelism (rayon/crossbeam).
  - First-party Windows bindings (`windows-sys`, `windows`) [C].
  - The same engine powers a CLI that needs no UI runtime.
- **UI in WinUI 3.**
  - It is the only option with native Fluent controls, Mica, modern UI Automation and Native AOT together [C].
  - Its known weakness (`TreeView` at scale) is avoided by design: the tree is rendered as a **flattened, virtualised row list** paged from Rust.
- **Why not WPF first?** WPF scores marginally higher on robustness. Its trade-offs are no AOT or trimming, a bigger distribution or a runtime prerequisite, and an experimental Fluent API. If the Phase 0 prototype fails its gates (§12, P0-2/P0-5), switch to WPF. The engine and FFI do not change.
- **Why not Tauri?**
  - IPC throughput on Windows is a measured weakness for exactly our workload.
  - WebView memory overhead.
  - Windows 10 runtime availability is not guaranteed.
  - Revisit only if macOS/Linux become goals.
- **Why not pure Rust (egui/Slint)?**
  - Native fidelity and accessibility are weaker.
  - Slint's licence terms complicate TreeDock's licence: its GPLv3 option is incompatible with PolyForm Noncommercial, which leaves the Royalty-free 2.0 terms and their ambiguous clauses.
  - Accepted cost of not choosing this: **two languages** (Rust + C#) raise the contributor bar. This is mitigated by a strict boundary: all logic lives in Rust, and C# is presentation only.

### 3.3 Process model

```
┌──────────────────────────────────────────────┐
│ TreeDock.exe (WinUI 3, C#, asInvoker)        │
│  Views ─ ViewModels ─ EngineClient (P/Invoke)│
│                │                             │
│        treedock_engine.dll (Rust, in-proc)   │
│  scanners · tree store · queries · layout    │
│  hashing · snapshots · file ops · reports    │
└─────────────┬────────────────────────────────┘
              │ (only when the user requests elevated fast scan)
              ▼
┌──────────────────────────────────────────────┐
│ treedock-cli.exe scan --mft --emit-pipe …    │
│ (same Rust engine, launched via "runas")     │
│ streams a snapshot over a named pipe         │
└──────────────────────────────────────────────┘
```

- **One process by default.**
  - No services and no resident helper.
  - The engine is an in-process DLL. This keeps queries in the tens-of-nanoseconds per call range [A] and avoids IPC.
- **Elevation, two options:**
  - **Option A (Phase 2):** "Restart as administrator" relaunches the whole app elevated. This is simple and is what competitors do.
  - **Option B (Phase 5, decided after P0 evaluation):** an elevated scan runs **the existing CLI binary** elevated. It streams its result through a named pipe secured to the current user SID, in the snapshot format, and exits. The UI stays non-elevated.
  - Option B keeps Explorer drag-and-drop working, because UIPI blocks drag from a non-elevated Explorer into an elevated window [K]. It also adds no new component.
  - USN incremental refresh needs admin [C], so Option B is what makes "refresh without running the whole UI elevated" possible.

### 3.4 Repository layout (planned; not created yet)

```
/engine/                      Cargo workspace
  crates/treedock-core/       tree store, aggregation, queries, filters, treemap/sunburst layout,
                              snapshot format, diff, hashing pipeline, exports (platform-neutral)
  crates/treedock-ntfs/       pure-Rust MFT record parser (no OS calls; fuzzable on any OS)
  crates/treedock-win/        Windows scanners, volume info, USN, change watch, shell/file ops
  crates/treedock-ffi/        C ABI over core+win; csbindgen-generated C# bindings
  crates/treedock-cli/        CLI: scan, export, diff, report, bench
/app/TreeDock.App/            WinUI 3 project (C#)
/app/TreeDock.App.Tests/      C# unit + UI-automation smoke tests
/bench/                       benchmark harness, dataset generator, VHDX fixture scripts
/docs/                        user docs, ADRs (docs/adr/NNNN-*.md), format specs
/.github/workflows/           build/test/release CI only (no CLA workflows; see §11.4)
LICENSE.md  NOTICE  README.md  CONTRIBUTING.md  CLA.md  THIRD_PARTY_NOTICES
```

There are five crates, each with a clear reason to be separate:

- `ntfs` is isolated so it can be fuzzed on Linux CI.
- `core` is platform-neutral so it can be tested anywhere.
- `ffi` and `cli` are thin frontends.

### 3.5 Engine ↔ UI boundary (FFI design)

- **Generator.** csbindgen produces C# `DllImport`/`LibraryImport` declarations and blittable structs from `extern "C"` functions [C]: https://github.com/Cysharp/csbindgen. uniffi was rejected because it has no official C# backend and its serialisation is heavy [C].
- **Handles.** `ScanSession` and `TreeView` are opaque `u64` handles. Errors are returned as `i32` codes, with `td_last_error(buf, len)` for messages.
- **Pull-based paging, no per-file callbacks.** The UI asks for what it is about to display:
  - `td_rows(view, first_row, count, *mut RowView) -> count` serves the flattened tree list. `RowView` is a fixed-size blittable struct:
    - node id
    - depth
    - flags
    - sizes
    - counts
    - times
    - name offset/length into a returned UTF-16 buffer
  - `td_expand(view, row)`, `td_collapse(view, row)` and `td_sort(view, column, dir)`.
  - `td_treemap_layout(node, w, h, min_px, *mut Rect, cap)` returns rectangles as packed floats plus node ids.
  - `td_query(...)` handles search, duplicates and statistics. Results become their own paged views.
- **Progress.**
  - The engine updates atomic counters (files, bytes, current path, phase).
  - The UI polls `td_progress(session, *mut Progress)` on a `DispatcherQueueTimer` at about 10–30 Hz.
  - This avoids marshalling callbacks onto the UI thread and AOT callback pitfalls.
  - A single unmanaged callback (`delegate* unmanaged`) is used only for "phase changed / finished" notifications.
- **Memory ownership.** The side that allocates a buffer frees it. The UI passes pooled, pinned buffers in. No Rust-allocated memory crosses the boundary without a matching `td_free_*`.
- **Threading.** The UI thread is only allowed to call functions that do O(page) work. Long operations (scan, hash, export) run on engine threads and return a session handle.

### 3.6 Principal dependencies (minimal set)

Licences are to be verified at the Phase 1 dependency audit. Versions are from crates.io on 2026-10-20 [C].

| Area | Dependency | Purpose | Licence [K] |
|---|---|---|---|
| Win32 FFI | `windows-sys` 0.61 | FSCTLs, file APIs | MIT/Apache-2.0 |
| COM | `windows` 0.62 | IFileOperation, shell APIs | MIT/Apache-2.0 |
| Parallelism | `rayon` 1.12, `crossbeam` 0.8 | Work-stealing, channels | MIT/Apache-2.0 |
| Regex | `regex` | Search/filter | MIT/Apache-2.0 |
| Hashing | `xxhash-rust` (xxh3) 0.8, `blake3` 1.8 | Partial / full hashes | BSL-1.0 / CC0/Apache-2.0 |
| Serialisation | `serde`, `serde_json`, `csv` | Export, settings | MIT/Apache-2.0 / Unlicense/MIT |
| Compression | `zstd` | Snapshot files | MIT/BSD |
| History DB | `rusqlite` (bundled SQLite) | Trends catalogue | MIT; SQLite public domain |
| CLI | `clap` | Argument parsing | MIT/Apache-2.0 |
| UI | Microsoft.WindowsAppSDK 2.x, Microsoft.Graphics.Win2D, CommunityToolkit.Mvvm 8.3+ | UI, drawing, MVVM (AOT-safe [C]) | MIT [K]; **verify WinAppSDK redistribution terms** |

Deliberately excluded:
- **`bincode`.** 3.0.0 is a tombstone release that raises a compile error [C]: https://docs.rs/crate/bincode/3.0.0
- **The stale `ntfs` crate** (last released 2023) [C].
- **Third-party MFT crates in the product.** `ntfs-reader` 0.6 and `mft` 0.7 are used **only as benchmark and correctness references** in Phase 0/2. The product's MFT reader is an original implementation, so we control performance, correctness and the attack surface.
- **An allocator swap** (`mimalloc`). It is adopted only if a benchmark shows a gain [P].

### 3.7 Settings and portable mode

- **Settings** live in `settings.json`, versioned and schema-checked:
  - installed builds use `%APPDATA%\TreeDock\`
  - snapshots and history use `%LOCALAPPDATA%\TreeDock\`
- **Portable mode** is enabled when a `portable.txt` marker file sits next to the executable. All data then lives in `.\Data\`, and the registry is never written.

### 3.8 Extensibility (designed for, not built)

- Scanner backends implement one internal `trait VolumeScanner`. New filesystem strategies plug in without UI changes.
- Exporters implement `trait Exporter`.
- The snapshot format is documented and versioned (§6.4), so external tools can read it.
- The CLI's JSON output is a stable, versioned contract. This is the integration point for scripting, instead of a plugin API.

---

## 4. Feature specifications and expected behaviour

Each feature lists its phase (P1–P6, see §12).

### 4.1 Scan targets (P1, P2)

- **Targets:**
  - a whole volume, by letter or `\\?\Volume{GUID}\`
  - a folder
  - several targets at once (each becomes a root in one session, or a tab)
  - removable media
  - SMB shares (UNC paths or mapped drives)
- **Volume picker** shows, for each volume:
  - label, letter and mount folders
  - filesystem
  - capacity/used/free bars
  - media type (SSD/HDD/removable/network)
  - **the scan mode that will be used** ("Fast MFT scan — requires administrator", "Standard scan")
- **Unsupported targets** get a clear message: MTP devices (planned to be evaluated later), WebDAV, and anything else.

### 4.2 Scan lifecycle (P1)

- **Progressive results.**
  - Universal scanner: folders appear and their sizes grow as directories complete. A folder still being scanned shows an "in progress" glyph and a "≥" prefix on its size.
  - MFT scanner: the tree appears once linking finishes. Before that, progress shows the fraction of MFT bytes read [A: MFT reads complete quickly, so a partial tree has little value — P0-1 confirms].
- **Live progress:**
  - files and folders seen
  - bytes counted
  - throughput
  - elapsed time
  - current path, throttled
  - error count, clickable
- **Cancel** stops within the target latency (§9). Partial results stay browsable and are marked "Incomplete scan".
- **Pause/resume** for the universal scanner (workers park on a condition variable) and between MFT chunks. It is not offered for SMB operations already in flight.
- **Background priority option.** "Low-impact scan" lowers thread I/O priority with `THREAD_MODE_BACKGROUND_BEGIN` [K] and reduces parallelism.
- **Rescan** of a selected subtree only.

### 4.3 Size accounting (P1, P2)

Definitions are shown in a tooltip and the help docs.

| Measure | Definition | Source |
|---|---|---|
| **Logical size** | End-of-file byte count (plus alternate data streams, if that option is on) | `EndOfFile` from the directory query, or the MFT `$DATA` data size |
| **Allocated size** ("size on disk") | Clusters reserved. For compressed/sparse files, the actual allocated bytes | `AllocationSize` from `FILE_ID_*_DIR_INFO` [C], or the MFT non-resident header. `GetCompressedFileSizeW` [C] only as a fallback |
| **Unique allocated** | Allocated size with each hard-linked file counted once per volume (keyed by file ID) | Hard-link detection (§5.4) |
| **Scanned vs volume** | Volume used space (`GetDiskFreeSpaceExW`) minus scanned unique allocated = **unaccounted** | Shown in the "Why doesn't this add up?" panel |

**Hard-link policy** is user-selectable:
- count once, at first-seen path (default)
- divide equally across links (answers WinDirStat #448)
- count in every location ("Explorer-like")

Folder totals state which policy is in effect.

**Fixed rules:**
- **Small resident NTFS files** have allocated size 0. This is correct, not a bug.
- **Cloud placeholders** (`FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` / `RECALL_ON_OPEN`) [C]:
  - Logical size is shown. Allocated size reflects only locally present data.
  - They are marked with a cloud badge.
  - **They are never opened or read**, so they are never hydrated.
- **Reparse points** [C: https://learn.microsoft.com/en-us/windows/win32/fileio/reparse-point-tags]:
  - Name surrogates (junctions, symlinks, mount points) are shown as links and **not followed by default**. This avoids double counting and cycles.
  - Following them is opt-in, with cycle detection by (volume serial, file ID).
  - DEDUP, WOF (CompactOS) and cloud tags are counted as files with their reported allocation.
  - The UI marks the caveats: "dedup chunk store not attributed"; "WOF allocation accuracy pending verification [P]".
  - WSL `LX_*` / `AF_UNIX` entries are zero-size leaves.
- **ReFS block cloning** [C: https://learn.microsoft.com/en-us/windows-server/storage/refs/block-cloning]:
  - Per-file allocation can **over-count** real usage, and this is labelled as such.
  - The reconciliation panel will show a negative "unaccounted" figure, with an explanation.
- **Inaccessible folders** are marked with ⚠, counted in an error list, and excluded from totals. Totals then carry "≥".
  - One-click "Rescan elevated" is offered.
  - When elevated, `SeBackupPrivilege` + `FILE_FLAG_BACKUP_SEMANTICS` lets the scan read access-denied directories [C/K]: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew
- **Long paths.** All engine paths use `\\?\` / `\\?\UNC\` internally. The manifest also declares `longPathAware` [C]: https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation

### 4.4 Analysis views (P3–P5)

- **Tree list (P3).**
  - Expandable hierarchy with columns:
    - name
    - logical size
    - allocated size
    - % of parent (with a bar)
    - files
    - folders
    - modified, created, accessed
    - attributes
    - owner (P6, optional, slow)
    - extension
    - path
  - Columns are sortable, reorderable and can be shown or hidden. Layout is saved per user.
  - Tooltip shows the full path (answers WinDirStat issue).
- **Top files (P3).** The N largest files under the selection (default N = 1,000, configurable). They are kept incrementally in a bounded heap per directory, so selecting a different folder is cheap [A, P].
- **Extension statistics (P3).** Count, logical/allocated totals and % share **for the current selection**. Results are grouped by user-editable categories (video, images, archives, code, executables, system…).
- **Treemap (P3).** Squarified layout.
  - Coloured by extension category, by age, or uniformly.
  - Optional cushion shading.
  - Click selects (synchronised with the tree list). Double-click drills down. A breadcrumb leads back up.
  - Hover shows path and size.
  - Rectangles below `min_px` are aggregated into a hatched "small files" area.
- **Sunburst (P4).** Ring chart over the selection, depth-limited (default 4), with the same interactions as the treemap.
- **Age view (P4).** A histogram of allocated bytes by last-modified bucket (<1 month … >5 years). Clicking a bucket filters.
- **Empty folders (P4).** Folders whose subtree contains no files. Hidden/system folders and reparse-point folders are excluded by default.
- **Old files (P4).** Files not modified or accessed for more than N days. The UI notes that last-access updates are often disabled on NTFS [K].
- **Tabs (P3).** Several scans or views open at once.

### 4.5 Search and filtering (P4)

- **Live filter bar** over the in-memory index. It matches name/path substrings, glob or regex (`regex` crate, linear-time).
- **Structured filters**, combinable with AND/OR:
  - extension(s)
  - size range (logical or allocated)
  - modified/created/accessed date ranges
  - attributes (hidden, system, compressed, sparse, reparse, cloud, read-only, offline)
  - depth
  - has hard links
  - inaccessible
- **Text query syntax** (documented), e.g. `ext:mp4,mkv size:>1GB modified:<2024-01-01 path:"\Videos\"`.
- **Results** show as a flat, virtualised, sortable list, exportable like any other view. Searches can be saved.
- **Search runs in parallel** over the name pool with rayon. The target is interactive latency on 10M entries [P].

### 4.6 Duplicates and wasted space (P4)

The pipeline uses cheap filters first:

1. **Candidate grouping by logical size.** Apply the user's minimum size (default 1 MiB). Exclude:
   - zero-byte files
   - cloud placeholders (never hydrated)
   - reparse points
   - files sharing a file ID (hard links to the same data are *not* duplicates; they are shown as "already linked")
2. **Partial hash.** XXH3-64 of the first and last 64 KiB (and the middle, for files over 1 MiB). This splits each group.
3. **Full hash.** BLAKE3 over the whole content, for files still grouped.
   - Optional byte-for-byte verification is offered before any destructive action.
   - The UI only says "identical" after a full hash or byte comparison. "Probable" is never presented as certain. This is in contrast to WinDirStat's sparse "probable" mode [C].
4. **I/O scheduling.** Reads are ordered by physical device:
   - HDDs: one reader per spindle, in file order
   - SSDs: parallel readers, bounded
   - Files are opened with `FILE_FLAG_SEQUENTIAL_SCAN` [K], and hashing is cancellable.
5. **Results.**
   - Groups are sorted by wasted bytes, i.e. (copies − 1) × size.
   - A tree view of where duplicates live answers WinDirStat #656.
   - **Rule-based selection helpers** keep the oldest, the newest, the copy in a preferred folder, or the shortest path.
   - Actions: recycle, move, or (P6, opt-in, same-volume NTFS only) replace with a hard link, with strong warnings about shared-edit semantics.
6. **Hash cache.** Results are keyed by (volume serial, file ID, size, last-write time) and reused across sessions.

### 4.7 File management (P3 basic, P4 full)

- **Open, Open with…, Reveal in Explorer** (`SHOpenFolderAndSelectItems` [K]), **Properties** (`SHObjectProperties` [K]).
- **Copy path(s)**, name or a details row (TSV).
- **Shell context menu** of the item (P4, `IContextMenu` [K]).
- **Move / Rename (P4).**
- **Recycle (P3)** and **Permanent delete (P4).** Safeguards are in §8.
- **The tree updates in place** after an operation. Sizes are subtracted from ancestors, with no rescan.
- **Collector / staging list (P4).** Users gather items from any view. It shows the total reclaimable space, and they can review and then act on the whole batch.

### 4.8 Snapshots, comparison and trends (P5)

- **Save scan** to a `.tdsnap` file (§6.4). This is automatic (optional, with a retention policy) or manual.
- **Open snapshot** to browse it without rescanning. It is read-only, and stale items are marked if their paths have since changed.
- **Compare two snapshots**, or a snapshot against a live scan:
  - per-folder delta (absolute and %)
  - new, removed and grown files
  - a "growth" sort and a treemap coloured by delta
- **Trends:**
  - The history DB records per-folder aggregates at each saved scan, down to a configurable depth (default 4).
  - Sparkline columns, a folder growth chart, and a volume used-space timeline.
- **Incremental refresh** (USN, §5.6) brings a loaded snapshot up to date when possible.

### 4.9 Change monitoring (P5)

- While a scan is open, the user can watch for changes:
  - elevated NTFS: USN journal polling
  - otherwise: `ReadDirectoryChangesExW` [C]
- Changed subtrees are marked "dirty". They can be re-scanned automatically (debounced) or manually.
- On buffer overflow, the entire watched root is marked dirty [C: https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw]. There is never a silent wrong total.

### 4.10 Reports and export (P1 CSV/JSON, P5 HTML/PDF/AI)

- **CSV.** One row per item, with selectable columns, depth limit and filter. Encoding is UTF-8, with an optional BOM for Excel.
- **JSON.** Versioned schema (`schema_version`): tree or flat form, with streaming writes for large exports.
- **HTML.** A single self-contained file with no external resources:
  - summary
  - top folders and files
  - extension table
  - an inline SVG treemap of the top levels
- **PDF.** The same content as HTML. The approach needs a decision [P]:
  - (a) a pure-Rust PDF writer such as `printpdf`/`krilla`, or
  - (b) print the generated HTML through the system "Microsoft Print to PDF" printer.
  - Preference is (a), to keep it offline and deterministic. Choose after a spike.
- **AI-assistant summary.** A compact Markdown (+ JSON) document within a token budget. It contains:
  - volume summary
  - the top N folders and files
  - extension breakdown
  - growth since the last snapshot
  - inaccessible areas
  - **Privacy options:** redact the user profile name (`C:\Users\<user>`), hash or omit file names, depth limit.
  - Includes a one-line explanation of the size definitions, so an assistant interprets them correctly.
  - Copied to the clipboard or saved to a file. **TreeDock never sends it anywhere.**

### 4.11 CLI (P1, extended later)

```
treedock-cli scan <path|volume> [--mode auto|mft|walk] [--out scan.tdsnap]
treedock-cli export <scan.tdsnap|path> --format csv|json|html|pdf|ai-md [--depth N] [--filter "..."]
treedock-cli diff <old.tdsnap> <new.tdsnap> [--format ...]
treedock-cli bench <path> [--runs N] [--json]
```

- The CLI is **read-only by design.** It has no delete or move commands, which keeps scripts safe.
- Scheduled scans are documented as "create a Task Scheduler job running `treedock-cli scan …`". TreeDock can open a pre-filled Task Scheduler dialog when the user asks, but never registers a task on its own.

---

## 5. Filesystem scanning strategies and fallbacks

### 5.1 Strategy selection

```
for each scan root:
  fs = GetVolumeInformationW(root)            // NTFS / ReFS / exFAT / FAT32 / remote
  media = seek-penalty query (§5.7)
  if fs == NTFS && local && elevated && root is volume root (or user allows MFT for subfolder):
      MftScanner            (fall back to WalkScanner on any failure, with a logged reason)
  else:
      WalkScanner(profile = SSD | HDD | Network | Removable)
```

- For a **subfolder of an elevated NTFS volume**, the MFT scanner can still read the whole MFT and then present only the subtree. Whether that beats walking depends on folder size relative to the volume [P0-1 decides the threshold].
- **The UI always shows which mode ran, and why.**

### 5.2 MftScanner (elevated, local NTFS) — P2

**Confirmed constraints:**
- Opening `\\.\X:` requires administrative privileges [C: CreateFileW "Physical Disks and Volumes"].
- `FSCTL_ENUM_USN_DATA` returns FRN, parent FRN, attributes and name, but **no sizes** [C/K]: https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_enum_usn_data
- `FSCTL_GET_NTFS_FILE_RECORD` works one record at a time [C], which is too slow for full scans.

**Chosen approach: raw `$MFT` read with an original parser (`treedock-ntfs`).** The on-disk format references are non-Microsoft public documentation, e.g. https://flatcap.github.io/linux-ntfs/ntfs/ [K].

1. Call `FSCTL_GET_NTFS_VOLUME_DATA` to get bytes per cluster, bytes per file record segment, `MftStartLcn` and `MftValidDataLength` [C/K].
2. Read MFT record 0 and parse the `$MFT` `$DATA` runlist, because the MFT itself may be fragmented. Read `$MFT:$BITMAP` to skip unused records.
3. Read the extents sequentially in large aligned chunks (1–4 MiB, tuned in P0-1) on a reader thread.
4. Parse chunks in parallel on rayon:
   - Apply update-sequence fixups and validate the `FILE` signature.
   - Extract `$STANDARD_INFORMATION` (times, attributes).
   - Extract all `$FILE_NAME` attributes. Skip DOS-only names. Each Win32/POSIX name with its parent FRN is one link.
   - Extract unnamed and named `$DATA`: logical size, allocated size, and compressed size for compressed/sparse files.
   - Extract `$REPARSE_POINT` tags.
   - Mark records that have an `$ATTRIBUTE_LIST`.
5. **Second pass for extension records.** Resolve each `$ATTRIBUTE_LIST`: follow the non-resident list where needed, and merge extension-record attributes into the base record.
6. **Link.**
   - FRN → node index uses a dense `Vec<u32>` indexed by record number.
   - Build children lists and attach hard links under every parent.
   - Compute aggregates bottom-up in one post-order pass.
7. **Validate.**
   - Sequence numbers in parent references must match. Otherwise the record is an orphan and goes under a synthetic "Orphaned" node.
   - Corrupt records are logged and skipped.
   - If the error rate passes a threshold, abort to WalkScanner with a reason.

**Hard links and ADS** come free in this mode. **Times** use `$STANDARD_INFORMATION`, not the often-stale `$FILE_NAME` times or sizes [K].

**Safety.** The parser is treated as consuming **untrusted input**:
- bounds-checked
- no `unsafe` in `treedock-ntfs`
- fuzzed (§10)

**Reference comparison.** Results are compared against `ntfs-reader`/`mft` and against WalkScanner on the same volume (correctness oracle, §10).

### 5.3 WalkScanner (universal: non-admin, ReFS, exFAT, FAT32, SMB, removable) — P1

**Enumeration primitive.** Open each directory once with `FILE_LIST_DIRECTORY | SYNCHRONIZE` and `FILE_FLAG_BACKUP_SEMANTICS`. Then call `GetFileInformationByHandleEx` in a loop, with a fallback chain of information classes:

| Order | Class | What it adds | Source |
|---|---|---|---|
| 1 | `FileIdExtdDirectoryInfo` | 128-bit file ID (needed for ReFS) and `ReparsePointTag` | [C]: https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_extd_dir_info |
| 2 | `FileIdBothDirectoryInfo` | `AllocationSize`, `EndOfFile`, 64-bit file ID; "no specific access rights are required" | [C]: https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_both_dir_info |
| 3 | `FindFirstFileExW(FindExInfoBasic, FIND_FIRST_EX_LARGE_FETCH)` | Last resort. Has no `AllocationSize`, so it is derived by rounding up to the real cluster size and the result is marked "estimated" | [C]: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirstfileexw |

- The buffer is 64 KiB per call. That is the documented SMB limit for change notifications [C], and per-call buffer size for directory queries is benchmarked in P0-1.
- `NtQueryDirectoryFileEx` (documented in the WDK [C]) is benchmarked against `GetFileInformationByHandleEx` in P0-1. It is adopted only if it is measurably faster, because it is an ntdll API.

**Traversal.**
- Work-stealing over directories (rayon `scope` or a crossbeam deque) [P].
- Each worker enumerates one directory and pushes subdirectories. It sends a compact `DirBatch` (entries plus name bytes) to a **single builder thread**. The builder appends to the arena and propagates sizes up the ancestor chain.
- One writer means no fine-grained locking.
- UI reads take a short read lock per page (§6.2).

**Concurrency profile** [P: values tuned by benchmark]:

| Media | Default concurrency | Rationale |
|---|---|---|
| SSD/NVMe (no seek penalty) | about 2 × logical cores, capped | Queue depth helps on flash [K] |
| HDD (seek penalty) | 1–2 | Avoids head thrashing (gdu offers `--sequential` for the same reason [C]) |
| SMB | 16–32 | The bottleneck is round-trip latency, not the disk [A] |
| Removable / USB | 2–4 | Conservative |

**Hard links (non-admin).**
- Directory-info classes don't include the link count.
- Opening every file to call `FILE_STANDARD_INFO.NumberOfLinks` is too expensive.
- Policy: an **opt-in "Detect hard links (slower)"** option, enabled by default only for known hard-link-heavy roots (e.g. `\Windows\WinSxS`, pnpm/Nix-style stores) [A]. Without detection, totals are labelled "hard links not deduplicated". This is the same limitation WizTree documents for non-admin [C].

**Error handling.**
- `ERROR_ACCESS_DENIED`, sharing violations, network drops and paths deleted mid-scan are recorded per directory.
- The scan continues past every one of these.
- Each SMB call has a timeout guard: a stuck directory is skipped, logged and can be retried.

### 5.4 Hard-link identity

- The key is (volume serial, file ID). It is 128-bit where available (ReFS needs it [C]).
- Each link appears in the tree, but data size is attributed according to the policy in §4.3.

### 5.5 Filesystem-specific notes

| FS | Scanner | Notes |
|---|---|---|
| NTFS (local, admin) | MftScanner | Full fidelity: hard links, ADS, compression, sparse, reparse. |
| NTFS (non-admin / subfolder) | WalkScanner | AllocationSize is reliable [C]. Hard links are opt-in. ADS is opt-in via `FindFirstStreamW` [K] (slow, per file). |
| ReFS / Dev Drive | WalkScanner | 128-bit IDs. Block-clone over-count caveat [C]. `FSCTL_GET_NTFS_FILE_RECORD` is unsupported [C]. Whether MFT_ENUM_DATA_V1 helps on ReFS is a P2 investigation [C that V1 exists "for ReFS"]. |
| exFAT / FAT32 | WalkScanner (class fallback) | No hard links, ADS or reparse points. File IDs are unstable [C]. Allocation is the size rounded up to the cluster [K]. FAT timestamps are coarse. Mark this. |
| SMB | WalkScanner, Network profile | No FSCTLs [C]. AllocationSize is whatever the server reports [A]. Identity uses (share, file ID) when IDs are reliable, otherwise the path. Server-side dedup and snapshots are invisible. Shown as a caveat. |
| Removable | Per FS above | Handle surprise removal: device-removed errors stop the scan gracefully, and partial results stay. |
| Unknown / third-party FS | WalkScanner, FindFirstFileEx fallback | Sizes marked "estimated" wherever AllocationSize is absent. |

### 5.6 USN journal for incremental refresh — P5

- **Every** change-journal operation requires admin [C]: https://learn.microsoft.com/en-us/windows/win32/fileio/using-the-change-journal-identifier
- **On snapshot**, store `{volume serial, UsnJournalID, NextUsn}`.
- **On refresh**, call `FSCTL_QUERY_USN_JOURNAL` [C]. Then:
  - Journal ID changed, or stored USN < `LowestValidUsn` → **full rescan**.
  - `ERROR_JOURNAL_ENTRY_DELETED` or `ERROR_JOURNAL_NOT_ACTIVE` → **full rescan**.
  - Otherwise, call `FSCTL_READ_USN_JOURNAL` from the stored USN [C]. Collapse records per FRN, using `USN_REASON_CLOSE` to finalise [K].
  - For each touched FRN, re-read its MFT record (`FSCTL_GET_NTFS_FILE_RECORD` is acceptable at this scale [C]) and apply create, delete, rename, move or size change to the tree.
  - Walk records by `RecordLength`, and handle V2/V3, plus V4 range records that follow a V3 CLOSE [C]: https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-usn_record_v4
- **Non-admin journal access** (`FSCTL_READ_UNPRIVILEGED_USN_JOURNAL`) is **unverified**. Treat it as unsupported until P5 testing proves otherwise.
- TreeDock **never creates or resizes a journal** on its own. It may offer to, with an explanation, because doing so changes system configuration.

### 5.7 Volume and media discovery

- **Volumes:** `FindFirstVolumeW` / `FindNextVolumeW`, `GetVolumePathNamesForVolumeNameW` (mount folders), `GetDriveTypeW`, `GetVolumeInformationW`, `GetDiskFreeSpaceExW` [K].
- **SSD vs HDD:** `IOCTL_STORAGE_QUERY_PROPERTY` with `StorageDeviceSeekPenaltyProperty` → `DEVICE_SEEK_PENALTY_DESCRIPTOR.IncursSeekPenalty` [C]: https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-device_seek_penalty_descriptor
  - [A/P] This should work on a volume handle opened with access mask 0 without admin, which CreateFile allows for querying device attributes [C]. P0-1 verifies it.
  - Unknown media defaults to the HDD profile, which is safe.

### 5.8 Cancellation and I/O stalls

- A cooperative cancel token is checked between directories and between MFT chunks.
- For blocking SMB calls, `CancelSynchronousIo` on the worker thread [K] is the escape hatch. Workers are always abandonable: results from a cancelled worker are discarded.

---

## 6. Data structures, indexing, caching and persistence

### 6.1 Tree store (struct-of-arrays arena)

Nodes are identified by a `u32` index. This supports up to about 4.29 billion nodes. Above about 1 billion, the engine refuses with a clear error and suggests summary mode (§6.3).

| Column (per node) | Type | Notes |
|---|---|---|
| `parent` | u32 | |
| `first_child`, `next_sibling` | u32, u32 | Built during the scan. After the scan, children are re-laid **contiguously sorted by size**, so the arrays become `child_start` + `child_count` |
| `name_off` | u32 | Offset into the name pool (UTF-8, not NUL-terminated) |
| `name_len` | u16 | |
| `flags` | u16 | dir, reparse, link, hardlinked, compressed, sparse, cloud, error, incomplete, estimated |
| `ext_id` | u16 | Interned lowercase extension (≤65k distinct) |
| `attrs` | u32 | Raw Win32 attributes |
| `logical` | u64 | Own size. For a directory, the aggregate |
| `allocated` | u64 | Same |
| `mtime` | i64 | FILETIME |
| `ctime`, `atime` | i64 | **Optional columns**, only allocated if enabled (default: created on, accessed off) |
| `file_id` | u64 | Optional column. Needed for hard links, USN and the hash cache. Required in MFT mode. The 128-bit form goes in a side table only for ReFS |

Directory-only aggregates (`file_count`, `dir_count`, `unique_allocated`) live in a **separate dense array indexed by directory ordinal**, so files pay nothing for them.

**Memory estimate** [A, to be measured in P1]:
- Mandatory columns come to about 48 bytes per node.
- Names add about 15–25 bytes on average.
- That gives roughly **70–90 bytes per entry**, i.e. about 0.7–0.9 GB for 10M entries.
- The target ceiling is ≤100 bytes per entry on the reference corpora (§9). This is a target, not a promise.

**Name pool.**
- UTF-8 in one large `Vec<u8>` (chunked, to avoid huge reallocations).
- Converted to UTF-16 only at the FFI boundary, for the rows being displayed.
- [A] Windows file names can contain unpaired surrogates, so they are stored as WTF-8 and convert back losslessly.

**Path reconstruction** walks parent links on demand. It is never stored per node.

### 6.2 Concurrency model

- During the scan, a **single builder thread owns writes**. Workers send batches to it over a bounded channel, which provides back-pressure and bounds memory.
- Readers (UI paging, search) take a `parking_lot::RwLock` read guard per page request [A: or `std::sync::RwLock`; benchmark].
- The builder holds the write lock only while applying one batch, which is bounded in size.
- [P] If lock contention shows up in P0-4, switch to epoch publication: the builder publishes an immutable "visible prefix" length and readers never block.
- After the scan finishes, the tree is frozen (re-laid contiguous and sorted). Queries then run lock-free on an `Arc<FrozenTree>`. Edits from file operations take the write lock briefly.

### 6.3 Bounded memory: summary mode (P6)

- For extremely large volumes (tens of millions of entries), the user, or an automatic threshold, can enable **summary mode**.
- Each directory keeps its subdirectories, its top-K largest files (default 64) and an aggregate "other files" pseudo-node by extension.
- This trades per-file detail for bounded memory. Search and duplicates then show "limited to retained files".
- [P] The threshold is decided from P1 memory measurements.

### 6.4 Snapshot format (`.tdsnap`) — P1 (write/read), P5 (history)

- A custom, versioned, little-endian columnar container. It is documented in `docs/formats/tdsnap.md`.
  - **Header:**
    - magic `TDSNAP\0\1`
    - format version
    - creation time
    - scanner mode
    - volume serial, filesystem, cluster size
    - USN journal ID and NextUsn (if any)
    - hard-link policy
    - options
  - **Sections:** each column array plus the name pool, extension table and error list.
  - Each section is **zstd**-compressed with its own length and XXH3 checksum.
- **Why custom rather than SQLite or rkyv:**
  - The columns are already arrays, so writing them is close to a memcpy and loading is just decompression.
  - Partial or corrupt files are detected per section.
  - Versioned readers stay simple.
  - SQLite would be far slower to bulk-insert at 10M+ rows [A]. rkyv ties the file format to the shape of Rust types.
- **Atomic save:** write to `*.tmp`, flush, then `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` [K].

### 6.5 History / trends database — P5

- `history.sqlite` (rusqlite, bundled):
  - **`scans` table:** id, root, volume serial, time, totals, snapshot path.
  - **`folder_stats` table:** scan_id, path_hash, depth, path, logical, allocated, files, dirs. Only depth ≤ N (default 4), so it stays small.
  - **`hash_cache` table** (§4.6).
- Retention policy: keep the last N scans per root, plus monthly representatives [A: default N=20].

### 6.6 Derived indexes (built on demand, cached per frozen tree)

- **Extension stats** per directory. Computed lazily for the selected node, using a parallel subtree pass with a cached result.
- **Top-K files.** A global top-K, plus per-selection computation using a partial selection, e.g. `select_nth_unstable` [A].
- **Sorted child order** per sort key. Size order is precomputed. Other keys are sorted lazily per expanded directory.
- **Flattened visible-row list** for the tree list. It is maintained incrementally on expand/collapse as a vector of node ids plus depth.

---

## 7. UI/UX requirements and workflows

### 7.1 Principles

- **Native Windows 11:**
  - Mica backdrop, with a solid fallback on Windows 10.
  - Fluent controls.
  - Light, dark and system themes, plus full **high-contrast** support.
  - Accent colour from the system.
- **Never block the UI thread.** All engine calls on the UI thread are O(page). Everything else is asynchronous.
- **Progressive disclosure.** The default view is simple: tree list, treemap and summary. Advanced columns and panels are opt-in. This avoids the settings clutter seen in WinDirStat #697.
- **Honesty in the UI:**
  - "≥" for incomplete totals
  - "estimated" badges
  - the active scan mode is always visible
  - a link to "Why doesn't this add up?"

### 7.2 Layout

```
┌ Title bar (Mica) ─ tabs [C:\ ×][D:\Projects ×][+] ───────────────────────────┐
│ Command bar: Scan ▾ | Rescan | Stop/Pause | Elevate | Search… | View ▾ | ⋯     │
├ Breadcrumb: C:\ › Users › alex › AppData                                     │
├───────────────────────────────┬──────────────────────────────────────────────┤
│ Tree list (virtualised,       │ Visualisation pane (tabs):                   │
│ sortable, columns)            │  Treemap | Sunburst | Types | Age | Top files│
│                               │                                              │
├───────────────────────────────┴──────────────────────────────────────────────┤
│ Status: 1,234,567 files · 412 GB allocated · Fast MFT scan · 3 inaccessible ⚠│
└───────────────────────────────────────────────────────────────────────────────┘
```

- The split is resizable, and the visualisation pane can be collapsed or moved below the tree.
- Every pane follows the **current selection**.

### 7.3 Key workflows

1. **"My drive is full."**
   - Launch → the volume picker shows drives and their fill bars.
   - Click C: → the scan starts. If not elevated, a banner offers "Restart as admin for a faster scan".
   - Sizes stream in. Sort by allocated size, drill into the treemap.
   - Add items to the Collector → Review → Recycle.
2. **"What grew?"** Open a scan → *Compare with…* a previous snapshot → sort by Δ → treemap coloured by growth.
3. **"Find duplicates in Photos."**
   - Select the folder → Duplicates → set the minimum size → run (progress, cancellable).
   - Apply a rule ("keep oldest") → review → Recycle.
4. **"Find old big videos."** Search `ext:mp4,mkv size:>500MB modified:<2023-01-01` → select all → move to an external drive.
5. **"Share with an AI assistant."** File → Export → AI summary → choose privacy options → copy to clipboard.
6. **Explorer entry point.** Right-click a folder → "Analyse with TreeDock" (§7.6) opens a tab and scans it.

### 7.4 Keyboard and mouse

- Every action is reachable from the keyboard. Planned shortcuts:

| Shortcut | Action |
|---|---|
| Ctrl+N | Scan |
| F5 | Rescan selection |
| Esc | Stop |
| Ctrl+F | Search |
| Ctrl+Shift+C | Copy path |
| Enter | Open |
| Ctrl+E | Reveal |
| Del | Recycle |
| Shift+Del | Permanent delete (with confirmation) |
| F2 | Rename |
| Alt+← / Backspace | Up |
| Ctrl+1..5 | Switch visualisation |
| Ctrl+Tab | Next tab |

- Shortcuts can be customised in settings (P6).
- Treemap keyboard navigation: arrow keys move to adjacent rectangles, Enter drills down, Backspace goes up.

### 7.5 Accessibility (built in from P3, audited before 1.0)

- **Tree list.** Custom AutomationPeers expose the TreeGrid semantics a flattened list would otherwise lose:
  - `ExpandCollapse`
  - level
  - position in set / size of set
  - Grid/Table patterns (column headers)
- **Treemap and sunburst** are visual aids. The tree list is the accessible equivalent. The focused rectangle is exposed as a UIA element with name and size, and selection changes are announced.
- **Charts** have a "view as table" alternative.
- **Tested with** Narrator, NVDA and Accessibility Insights for Windows (§10).
- **Visual requirements:**
  - Text scaling to 225%.
  - Per-monitor DPI v2 (WinUI handles this natively).
  - High-contrast colour mapping for the treemap: a pattern + border mode, and never colour alone for meaning.
  - Minimum target sizes per Fluent guidance.

### 7.6 Explorer integration (P4)

- **Classic context-menu verb** "Analyse with TreeDock" on folders and drives, registered per user under `HKCU\Software\Classes`. No admin is needed, it is offered as an opt-in at first run or install, and it is removable.
- **The Windows 11 top-level context menu** requires `IExplorerCommand` with package identity. An unpackaged app needs a **signed sparse package** [K/C-adjacent]. This is tied to the code-signing decision (§11.5) and comes in P6.
- **`treedock.exe <path>`** command-line argument, plus a single-instance option that forwards the path as a new tab.

### 7.7 Localisation

- All UI strings are in `.resw` from day one.
- English only for 1.0. Community translations come after 1.0.
- Sizes and dates use the user's locale. A setting chooses binary (KiB) or decimal (KB) units.

---

## 8. Security, permissions and safe file operations

### 8.1 Privilege model

- The app manifest uses `requestedExecutionLevel = asInvoker`. **The app never auto-elevates.**
- Elevation is requested only on an explicit user action: "Fast scan (administrator)", "Rescan inaccessible folders", or USN refresh. A UAC prompt is always the result of a click.
- **When elevated:**
  - Enable `SeBackupPrivilege` only for the duration of the scan, then remove it with `AdjustTokenPrivileges` [K].
  - **File operations (delete, move) are never performed with backup semantics.**
  - An elevated window shows a persistent "Administrator" badge.
- **Elevated helper (Option B, §3.3):**
  - The named pipe has a DACL restricted to the launching user's SID.
  - The helper accepts only a fixed, validated argument set: scan root and options.
  - It performs **no** file modifications.
  - It exits when finished.

### 8.2 Safe file-operation design

**Recycle (default)**
- `IFileOperation` with `FOFX_RECYCLEONDELETE | FOFX_ADDUNDORECORD | FOF_WANTNUKEWARNING` [C]: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-setoperationflags
- `SHFileOperation` is not used (superseded [C]).
- If the item cannot be recycled (no Recycle Bin on the volume, e.g. many removable or network drives), the shell warns. TreeDock **never silently falls back to permanent delete.**

**Permanent delete**
- Disabled by default behind a setting.
- When used, it shows a dialog listing:
  - item count
  - total size
  - the first 20 paths
  - any protected or system items
- The confirmation button reads "Permanently delete N items".
- **Typed confirmation** is required above thresholds (e.g. >1,000 items or >10 GB) [A: thresholds configurable].

**Pre-action re-validation (prevents acting on stale scan data)**
- Each item is re-stat'ed just before acting. Volume serial + file ID + size + last-write time must match the scan.
- Mismatches are skipped and reported as "changed since scan".

**Reparse points**
- Deleting a junction or symlink removes **the link only**. TreeDock's own deletion code opens with `FILE_FLAG_OPEN_REPARSE_POINT` and never recurses through a name surrogate [K].
- Shell behaviour for junctions is verified in the test suite [P].

**Protected locations**
- A built-in list is blocked by default:
  - `%SystemRoot%`
  - `Program Files*`
  - `ProgramData\Microsoft`
  - `System Volume Information`
  - `$Recycle.Bin`
  - `pagefile.sys` / `hiberfil.sys` / `swapfile.sys`
  - boot files
  - volume roots
  - the user profile root
  - the TreeDock install and data folders
- Each entry explains why it is blocked and links to the right Windows tool. Examples:
  - "Use Disk Cleanup / Storage Sense for Windows.old"
  - "WinSxS sizes are mostly hard links; use DISM"
- An advanced override exists, with an extra confirmation.

**Locked files**
- On a sharing violation, call Restart Manager (`RmStartSession → RmRegisterResources → RmGetList`) with **file** paths [C]: https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmgetlist
- Show which application holds the lock.
- **TreeDock never terminates processes.**

**Bulk operations**
- They run in the background with progress, and **can be cancelled** between items (answers WinDirStat #663).
- They continue past individual failures and produce a result report.

**Operation log**
- Every destructive or moving operation appends a JSON line to `operations.log`: time, action, path, size, file ID, result.
- The log is local only and viewable in the app.

**No automatic actions**
- Suggestions (e.g. "these 3 GB are in Temp") only ever *select* items for review.
- Scheduled CLI runs are read-only.

### 8.3 Untrusted-input hardening

- The MFT parser is treated as hostile-input code (§5.2). It is fuzzed continuously.
- Snapshot and JSON import is also hostile input: sizes are bounded, checksums verified, versions checked, and there is no deserialisation into arbitrary types.
- File names are displayed verbatim but escaped in HTML reports.
- Bidirectional control characters in names are shown visibly to prevent spoofing [A].
- "Open" uses `ShellExecuteExW` only on an explicit user action. The app does **not** auto-preview content.

### 8.4 Privacy and offline operation

- **No network code in the product.** CI enforces this with a dependency audit that rejects HTTP client crates and a review checklist.
- **No telemetry, crash upload, accounts or ads.**
- Crash information is written locally (minidump optional, off by default) and shown to the user for manual reporting.

---

## 9. Performance targets and benchmarking methodology

### 9.1 Targets

These are goals to validate, not promises.

| Area | Target | Gate |
|---|---|---|
| UI responsiveness | No UI-thread stall >50 ms caused by engine calls. Input-to-visual response <100 ms during a scan | P0-2, P3 |
| Scrolling | Smooth scrolling (display refresh rate, no visible hitching) with ≥1M visible rows | P0-2 |
| Progress cadence | ≥10 updates/s | P1 |
| Cancellation | ≤500 ms on local media. Best-effort with a timeout on SMB | P1 |
| Memory | ≤100 bytes per entry average on the reference corpora, including names | P1 |
| Treemap | Interactive re-layout and render (<100 ms) on resize or drill-down, with ≤200k visible rectangles | P0-3 |
| Cold start | <1.5 s to an interactive window on SSD (warm OS cache), measured on the reference machine | P0-5 |
| Scan throughput | **No number is promised.** Goal: competitive with the best available tool in the same mode on the same hardware (MFT vs WizTree/WinDirStat elevated; walk vs WizTree non-admin, dua, dust) | P1, P2 |
| Snapshot load | Faster than rescanning the same volume (otherwise snapshots lose their value) | P5 |

### 9.2 Benchmark methodology (P1 onward, published in `/bench`)

**Hardware matrix (documented in each report):**
- NVMe SSD
- SATA SSD
- 7200 rpm HDD
- USB 3 flash drive
- 1 GbE SMB share (Windows Server or Samba)
- [A] Initially whatever contributors have, reported transparently

**Datasets:**
- **Synthetic, deterministic.** A seeded generator (`treedock-cli bench gen`) creates trees with controlled distributions:
  - 1M tiny files
  - 100k large files
  - deep nesting (depth >100, long paths >260 chars)
  - wide directories (500k entries in one folder)
  - hard links, junctions, symlink cycles, ADS, sparse and compressed files
- **Real-world reference corpora**, captured as manifests and re-materialised by the generator, not redistributed:
  - a Windows 11 system drive
  - a developer tree (node_modules/target heavy)
  - a media library
- **Volumes on VHDX** created by script (`diskpart create vdisk` / `Mount-DiskImage` [K]) and formatted NTFS, ReFS, exFAT and FAT32. They are bit-for-bit reproducible across machines.

**Cold vs warm cache.**
- Cold runs **dismount and re-mount the VHDX** (or volume) to flush the file system cache [A/P: verify this flushes reliably]. This avoids reboots.
- Warm runs repeat immediately.

**Tools compared**, using each tool's documented CLI so runs can be scripted:
- WizTree `/export` [C]
- WinDirStat CLI [C]
- dua, dust, gdu
- TreeSize Pro CLI, if a licence is available
- Everything is a reference for MFT read time only

**Metrics:**
- wall time (hyperfine, ≥5 runs, report median and IQR)
- peak private bytes / working set
- total bytes read
- CPU time
- for TreeDock, also time-to-first-result

**Correctness alongside speed.**
- Each tool's totals are compared to TreeDock's and to an independent oracle (§10.3).
- **Speed numbers are only published together with correctness agreement.**

**Fairness rules:**
- same machine
- same elevation level per comparison class
- versions recorded
- antivirus state recorded
- no cherry-picking: all runs published

---

## 10. Testing and quality assurance

### 10.1 Rust engine

- **Unit tests:** aggregation, hard-link policies, sorting, flattening, filters, query parser, the snapshot round trip, and diff.
- **Property tests (`proptest`):**
  - Random trees plus random operations (insert/delete/move) must equal recomputing from scratch.
  - Snapshot write → read is identity.
  - Diff(a, b) applied to a equals b.
- **MFT parser:**
  - Golden tests on **synthetic MFT records**, constructed by a test builder from the format specification.
  - Captured records from VHDX fixtures.
  - Fuzzing with `cargo-fuzz` on Linux CI. The crate has no OS dependencies, which is why it is a separate crate.
  - Corpus seeded from the fixtures.
- **Windows integration tests** on VHDX fixtures (NTFS/ReFS/exFAT/FAT32) built by script in CI. They contain known content:
  - hard links
  - junction loops
  - ADS
  - compressed/sparse files
  - ACL-denied folders
  - long paths
  - Unicode / unpaired-surrogate names
  - cloud placeholder simulation via the Cloud Files API test provider [A/P]
- **Cross-scanner oracle.** MftScanner and WalkScanner results must agree on the same volume, within documented differences such as ADS.
- **File-operation tests** on fixtures:
  - recycle vs permanent
  - re-validation mismatch is skipped
  - a junction delete removes only the link
  - a locked file reports its holder via Restart Manager
  - the protected-path block

### 10.2 UI

- **ViewModel unit tests** (C#, xUnit/MSTest), with the engine mocked behind an interface.
- **UI Automation smoke tests** with FlaUI (MIT) [K] on CI: launch, scan a fixture, sort, expand, search, recycle a fixture file, theme switch.
- **Accessibility:**
  - Accessibility Insights automated checks on CI where possible.
  - A manual Narrator/NVDA pass per release, using a checklist.
- **Performance regression:**
  - A scripted scroll/expand on a 2M-row fixture records frame times.
  - It fails CI if the p95 regresses by more than a threshold.

### 10.3 Correctness oracle

An independent PowerShell or script-based sum over a fixture volume, plus `fsutil`/`GetDiskFreeSpaceEx` volume used space. It defines expected values for each size measure.

### 10.4 CI/CD

- **GitHub Actions:**
  - `windows-latest` runners: build, test and package for x64 + ARM64 (cross-compiled; ARM64 tests run on an ARM64 runner if available [A]).
  - Ubuntu runners: fuzzing and the platform-neutral core tests.
  - [A/P] Assumed: hosted Windows runners can mount VHDX with admin rights. Verify in P1.
- **Lints and checks:**
  - Rust: `cargo fmt --check`, `clippy -D warnings`, `cargo deny` (licences, advisories, banned crates such as HTTP clients), `cargo audit`.
  - C#: analyzers, with AOT/trim warnings treated as errors.
- **Releases:**
  - Tag → build → SHA-256 checksums → SBOM (CycloneDX) → GitHub Release.
  - **Reproducibility:**
    - pinned toolchains (`rust-toolchain.toml`, `global.json`)
    - locked dependencies (`Cargo.lock`, NuGet lock files)
    - `--remap-path-prefix` to strip local paths
    - `SOURCE_DATE_EPOCH`-style deterministic flags where supported
  - Bit-for-bit reproducibility of the .NET/WinUI output is **not guaranteed** [A]. The goal is to document the build so anyone can verify it from source.
- **Docs:** README, user guide (size definitions, scan modes, safety), architecture doc, ADRs, `CONTRIBUTING.md`, `CLA.md`, `SECURITY.md` (private vulnerability reporting).
- **CLA enforcement is separate from CI.** `ci.yml` contains build, test, lint and audit jobs only. CLA checks come exclusively from PactDock in `pactdock.yml` (§11.4, milestone PD).

---

## 11. Licensing, dependencies and distribution

### 11.1 Licence: PolyForm Noncommercial License 1.0.0 (owner decision)

**Owner and licensor:** TreeDock is owned and maintained by **ASPEC RACING PTY LTD** (Australia), the "licensor" under the licence. ASPEC retains full control over commercial licensing and any future relicensing of its own code.

**Decision (project owner, 2026-10-20):** all TreeDock source code, binaries and documentation published by ASPEC are licensed to the public under the **PolyForm Noncommercial License 1.0.0** (https://polyformproject.org/licenses/noncommercial/1.0.0/). The SPDX identifier is `PolyForm-Noncommercial-1.0.0`.

**What the licence permits** [C: verbatim text fetched from https://raw.githubusercontent.com/polyformproject/polyform-licenses/1.0.0/PolyForm-Noncommercial-1.0.0.md and committed as `LICENSE.md`]:
- Use, copying, modification and redistribution of the software for any **noncommercial purpose**.
- **Personal uses**: research, experiment, testing, private study, hobby projects, and private entertainment, explicitly without anticipated commercial application.
- Use by **noncommercial organisations**: charities, educational institutions, public research organisations, public safety or health organisations, environmental protection organisations and government institutions, regardless of their funding source.
- Redistribution must pass on the licence terms (or a link to them) and the `Required Notice:` lines.
- It includes a patent licence for noncommercial use, terminated if the licensee claims the software infringes a patent.

- Forking and redistributing modified versions for permitted purposes ("Changes and New Works License" and "Distribution License").

**What the licence does not permit:**
- Use by or for a business for commercial purposes, including internal workplace use at a company. This requires **separate written permission or a commercial licence from ASPEC RACING PTY LTD**.
- Sublicensing ("No Other Rights"). Only ASPEC can grant other licences.

**Independent forks** are not required to grant their modifications back to ASPEC. ASPEC gains rights in third-party changes **only** when they are submitted as contributions under the CLA (§11.3).

**Consequences for this plan (accepted by the owner):**

| Topic | Impact |
|---|---|
| Original brief: "100% free and open-source" | **Conflicts.** PolyForm Noncommercial is *source-available*, not an OSI-approved open-source licence, because the Open Source Definition forbids restricting fields of endeavour. Project materials must say "source-available" or "free for noncommercial use", **never "open source"**. |
| Original brief: "all functionality available without payment" | Holds for personal and noncommercial users. Commercial users are not licensed at all, so they cannot use it free *or* paid unless a commercial licence is offered. |
| Competitive positioning | Commercial-use freedom is no longer a differentiator (§2.3). WinDirStat (GPL-3.0) remains the free choice for workplaces. |
| Engine reuse | The Rust crates cannot be reused by OSS projects under OSI licences or by commercial tools. Treat the engine as part of the product, not as an ecosystem library. |
| Dependencies | Permissive dependencies (MIT, Apache-2.0, BSD, Zlib, BSL-1.0, Unicode, CC0) are compatible; their own licences still apply to their code and are shipped in `THIRD_PARTY_NOTICES`. **Copyleft dependencies (GPL, AGPL, LGPL when statically linked) are incompatible**, because those licences forbid the added noncommercial restriction. MPL-2.0 is permitted only for unmodified files after review. |
| Contributions | **Resolved: a CLA is required** for all external contributions (§11.3). A DCO alone would leave ASPEC holding only noncommercial rights in contributed code. |
| Distribution channels | GitHub Releases and direct downloads are fine. winget accepts non-OSS software with a declared licence [A, verify]. Catalogues and directories that list only open-source software are excluded. Free OSS code-signing programmes that require an OSI licence (e.g. SignPath Foundation [A, verify their criteria]) may **not** be available. |
| Hosting/CI | GitHub hosting and Actions are available to non-OSS public repos [A]. Some free tiers and services for "open source projects" may require an OSI licence. Check each one. |

**Alternatives considered** (kept for the record):

| | PolyForm NC 1.0.0 (chosen) | MIT / Apache-2.0 | GPL-3.0 |
|---|---|---|---|
| OSI open source | ✗ | ✓ | ✓ |
| Commercial use allowed | ✗ | ✓ | ✓ |
| Prevents commercial exploitation by others | ✓ | ✗ | ~ (only closed forks) |
| Patent grant | ✓ (noncommercial) | Apache only | ✓ |
| Accepts copyleft deps | ✗ | ✗ | ✓ |
| Contributor friction | Higher (CLA likely) | Lowest | Low |

**Required files** (part of the implementation plan, Phase 1):

| File | Content | Status |
|---|---|---|
| `LICENSE.md` | Verbatim PolyForm Noncommercial 1.0.0 text | Added (verbatim, SHA-256 `c0ea4a89…fabbe5` of the upstream 1.0.0 file) |
| `NOTICE` | `Required Notice: Copyright (c) 2026 ASPEC RACING PTY LTD` | Added |
| `CLA.md` | Explains the CLA requirement, links the ASPEC Individual and Entity agreements in `.github/cla/`, summarises them (including ASPEC's Option Five promise) and describes PactDock signing | Updated for PactDock |
| `.github/cla/ASPEC-ICLA-1.0.md`, `ASPEC-ECLA-1.0.md` | ASPEC's final v1.0 agreements, copied verbatim from PactDock `templates/cla/` at commit `babe751` and hash-pinned in `.github/pactdock.json` | Added |
| `CONTRIBUTING.md` | Contribution workflow, coding/testing standards, and the CLA requirement. States that external pull requests **cannot be merged until enforcement is switched on** | Updated |
| `README.md` | Plain-language licence section (who may use it free, who needs a commercial licence from ASPEC), "source-available, not open source" wording, link to `CLA.md` | Added |
| `THIRD_PARTY_NOTICES` | Licences of all bundled dependencies, generated in CI (`cargo about` or equivalent, plus NuGet licences) | Phase 1 release work |
| SPDX headers | `SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0` on source files | Applied as files are created |

**Terminology rule:** TreeDock is described as **"source-available"** or "free for noncommercial use" everywhere: README, docs, About box, installer, release notes. **Never "open source" or "OSS".**

### 11.2 Dependency policy

- Add a dependency only when it is clearly cheaper and safer than writing the code. Each one is recorded in an ADR if it is non-trivial.
- `cargo deny` allows only licences compatible with redistribution under PolyForm Noncommercial: permissive licences only, with copyleft families denied.
- No network-capable crates in the product.
- **No GPL, AGPL or statically linked LGPL dependencies.** Their terms forbid the noncommercial restriction.
- **Dependencies must also allow ASPEC's commercial licensing.** Because ASPEC may ship commercial or proprietary editions (§11.3), every dependency must allow use in commercial products. Permissive licences (MIT, Apache-2.0, BSD, Zlib, BSL-1.0, ISC, Unicode, CC0) satisfy this. "Noncommercial-only" or "source-available" third-party components are **not** accepted.
- Third-party licence texts are bundled with every release artifact.

### 11.3 Contributor Licence Agreement (CLA)

**Decision:** every external contribution submitted through a pull request requires a signed CLA. **CLA vs DCO is resolved in favour of a CLA.**

**Basis:** the **Harmony Agreements v1.0** (dated 2011-07-04) [C]: https://www.harmonyagreements.org/agreements
- **Individual:** HA-CLA-I, https://www.harmonyagreements.org/docs/ha-cla-i-v1.pdf
- **Entity**, for contributors acting on behalf of an organisation: HA-CLA-E, https://www.harmonyagreements.org/docs/ha-cla-e-v1.pdf. It extends coverage to the entity's Affiliates [C].
- The *licence* variants are used, not the copyright-assignment variants (HA-CAA-*), because contributors must keep ownership.

**How the verified Harmony provisions map to ASPEC's requirements:**

| ASPEC requirement | Harmony HA-CLA v1.0 provision | Fit |
|---|---|---|
| Contributors retain ownership | §2.1(a): "You retain ownership of the Copyright in Your Contribution" [C] | ✓ |
| Perpetual, worldwide, irrevocable, royalty-free, sublicensable rights | §2.1(b): "perpetual, worldwide, non-exclusive, transferable, royalty-free, irrevocable", sublicensable "through multiple tiers of sublicensees" [C] | ✓ |
| Patent rights | §2.2: patent licence on the same terms, limited to claims infringed by the Contribution [C]. No patent-retaliation clause | ✓ |
| Commercial / proprietary relicensing without further permission | **Option Five (§2.3)**: "any license, including copyleft, permissive, commercial, or proprietary licenses" [C] | ✓, with the condition below |
| Option Five condition | When used, ASPEC must "also license the Contribution under the terms of the license or licenses which We are using for the Material on the Submission Date" [C], i.e. it must stay available under PolyForm Noncommercial as well | ✓ Compatible: ASPEC keeps the public PolyForm NC release |
| Use, modify, distribute | §2.1(b) covers reproduce, modify, display, perform and distribute **"as part of the Material"** [C] | ⚠ **Scope limited to the "Material"** (works ASPEC makes available). The ASPEC agreements use a broader standalone grant instead (clause 2.2) |
| Moral rights | §2.4: waiver / non-assertion "to the maximum extent permitted" [C] | ⚠ In Australia, moral rights under the Copyright Act 1968 Pt IX are generally handled by **written consent**, not waiver [K]. Needs adaptation |
| Governing law | §6.1 `[JURISDICTION]` is free text and excludes CISG [C] | ASPEC agreements use Victoria (clause 6.3) |
| Entity contributors | HA-CLA-E with `[or Your Affiliates]` [C] | ✓ |

**Template fields ASPEC must decide** [C]:
- `[PROJECT_NAME]` (defines "We/Us", i.e. ASPEC RACING PTY LTD)
- `[SUBMISSION_INSTRUCTIONS]`
- `[NONOWNER_INSTRUCTIONS]`
- `[LIST_OF_MEDIA_LICENSES]` (optional, for non-software contributions)
- `[JURISDICTION]`
- signatory details

**Agreements adopted (final, 2026-10-20).** TreeDock uses ASPEC's shared agreements, maintained in the PactDock repository (`templates/cla/`, commit `babe751`):
- *ASPEC Individual Contributor Licence Agreement 1.0* (`.github/cla/ASPEC-ICLA-1.0.md`)
- *ASPEC Entity Contributor Licence Agreement 1.0* (`.github/cla/ASPEC-ECLA-1.0.md`)

They are independently worded and modelled on Harmony. The key clauses for TreeDock:

| Clause | Content |
|---|---|
| 2.1 | Contributors keep ownership |
| 2.2 | Perpetual, irrevocable, worldwide, non-exclusive, royalty-free, transferable, sublicensable licence, covering commercial exploitation. A standalone grant, broader than Harmony's "as part of the Material" |
| 2.3 | **Harmony Option Five.** ASPEC may relicense on any terms. If ASPEC makes a work containing the contribution available to third parties, it will **also** license the contribution under the licence that applied to the project on the Submission Date (PolyForm Noncommercial 1.0.0 for TreeDock today) |
| 2.4 | Patent licence |
| 2.5 | Moral rights handled by consent under Part IX of the *Copyright Act 1968* (Cth), with a fallback waiver |
| 6.1 | Electronic acceptance by GitHub comment |
| 6.3 | Governing law: Victoria |

The entity agreement adds Affiliates, signatory authority and maintainer verification (Schedule 1).

Changing an agreement means: update it in PactDock, copy it here verbatim, and update its `sha256` pin. Bump the version if anyone has already signed.

### 11.4 CLA enforcement: PactDock (integrated, awaiting activation)

**PactDock** (https://github.com/ASPEC-RACING-PTY-LTD/PactDock) is ASPEC's own CLA enforcement GitHub Action. It is the **only** CLA enforcement used by TreeDock. ASPEC authorised integration on 2026-10-20.

**How it works** (from PactDock's README and setup guide):
1. On each pull request, PactDock identifies every contributor by numeric GitHub account ID: the PR author, commit authors and `Co-authored-by:` identities.
2. It posts a `PactDock CLA` commit status and one instruction comment.
3. Contributors sign by commenting the exact acceptance phrase. PactDock verifies the comment and appends a record to a **private** signature repository.
4. Entity coverage becomes active only after a maintainer verifies the signatory.
5. Merges are blocked by requiring the `PactDock CLA` status in a ruleset.

**What is in the TreeDock repository:**

| File | Purpose |
|---|---|
| `.github/pactdock.json` | Rights holder; ICLA and ECLA with SHA-256 pins; store `ASPEC-RACING-PTY-LTD/cla-signatures`; bot allowlist (dependabot, github-actions); co-authors must sign. Validated with PactDock's own parser |
| `.github/workflows/pactdock.yml` | PactDock's example workflow, with the following changes: pinned to commit `babe7514a8910bf18adabdf0e15d4084e610f292`, GitHub App mode with `permissions: contents: read`, no checkout, and the job gated on the variable `PACTDOCK_ENABLED == 'true'` |
| `.github/cla/*.md` | The agreement texts the signing phrase refers to |
| `.gitattributes` | Keeps the agreement files LF so the hash pins stay valid |

**Design decisions:**
- **GitHub App authentication** (PactDock's recommendation). Statuses come from the App, so the required check can be pinned to it. Tokens are short-lived and scoped to one repository.
- **Separate private signature store.** TreeDock is a public repository, so self-storage would publish signers' account IDs and comments.
- **Off until configured.** The workflow does nothing until `PACTDOCK_ENABLED` is set, so it cannot fail on every PR before the App, secrets and store exist.

**Remaining owner actions** (none can be done from the code, and they require credentials or GitHub admin rights):
1. Create the private store `ASPEC-RACING-PTY-LTD/cla-signatures`. It must not be public. Give it a README so it has a default branch, and protect `main`.
2. Create and install the PactDock GitHub App on TreeDock and on the store. PactDock's `docs/SETUP.md` §3 lists the required permissions.
3. Set organisation or repository variable `PACTDOCK_APP_ID` and secret `PACTDOCK_APP_PRIVATE_KEY`.
4. Push TreeDock to `ASPEC-RACING-PTY-LTD/TreeDock`.
5. Run the verification steps in PactDock `SETUP.md` §7 on a test PR, then set `PACTDOCK_ENABLED=true`.
6. Add a ruleset on `main`: require the `PactDock CLA` status with the source set to the PactDock App; require pull requests; block force pushes.

**Rules:**
- No other CLA tool (e.g. CLA Assistant) or interim enforcement is added.
- Updating PactDock means reviewing the new commit and changing the pinned SHA deliberately.
- PactDock is not on the critical path of any TreeDock development phase (§12).

### 11.5 Distribution

| Artifact | Description |
|---|---|
| **Portable ZIP** (x64, ARM64) | Unpackaged self-contained WinUI app + engine DLL + CLI, plus a `portable.txt` marker. No install and no registry writes |
| **Installer** (x64, ARM64) | Per-user install by default (no admin), with an optional per-machine install. Tool: **Inno Setup** (free, simple) or **WiX** (MSI, better for enterprise deployment) [P: decide in P4; preference WiX for MSI because workplaces need it]. Offers the opt-in Explorer verb. Clean uninstall |
| **winget manifest** | Submitted to `microsoft/winget-pkgs` after 1.0 |
| **CLI-only ZIP** | For servers and scripting |
| **MSIX / Store** | Not planned for 1.0 (sandbox and identity constraints). Revisit with the sparse package work |

**Windows 10 support.**
- WinAppSDK requires Windows 10 1809+ [C].
- Mainstream Windows 10 support ended 14 Oct 2025, and consumer ESU ended 13 Oct 2026 [K]. Windows 10 22H2 is therefore supported **on a best-effort basis**: CI-tested and bug fixes accepted, but no new Windows 10-specific features.
- Mica falls back to a solid backdrop.

**Code signing.**
- Unsigned binaries trigger SmartScreen warnings.
- Options [A, verify]:
  - SignPath Foundation (free code signing for OSS projects; **likely unavailable** under PolyForm Noncommercial, as it targets OSI-licensed projects [A, verify])
  - Azure Trusted Signing (paid)
  - unsigned, plus published SHA-256 checksums
- **Signing is also required for the sparse package** that enables the Windows 11 modern context menu. Decide by P4.

---

## 12. Phased roadmap

Each phase ends with a tagged pre-release and an acceptance review. Durations are deliberately omitted: they depend on contributors and would be speculative.

### Phase 0 — Prototypes and architecture decisions

**Goal:** settle every [P] item that could change the architecture before building on it. The output is ADRs. Spike code is throwaway.

| Spike | Question | Pass criteria |
|---|---|---|
| **P0-1 Scanner I/O** | MFT raw read vs `FSCTL_ENUM_USN_DATA` + sizes vs walk (`GetFileInformationByHandleEx` vs `NtQueryDirectoryFileEx`). Buffer sizes, thread counts per media. Seek-penalty query without admin. Hard-link record behaviour of ENUM_USN_DATA. Whether WOF AllocationSize is accurate | Data-backed choices for each, recorded in ADR-0001..0003 |
| **P0-2 WinUI virtual tree list** | Flattened ListView/ItemsRepeater with 2M rows paged from Rust: scrolling, expanding a 500k-child node, memory. Narrator/NVDA reading level and expanded state via custom peers | Meets the §9 scrolling/stall targets. Narrator reads level, expanded state and position correctly. **On failure, re-run with WPF and choose** |
| **P0-3 Treemap rendering** | Win2D squarified treemap with 200k rects: hit-testing, drill animation, high DPI | Meets the §9 treemap target |
| **P0-4 FFI paging under load** | Paging while the builder thread writes, under Native AOT. Lock contention | No UI stall >50 ms. No AOT warnings |
| **P0-5 Packaging** | Native AOT + unpackaged self-contained: ZIP size, cold start on a Windows 10 22H2 HDD machine, trimming warnings | Documented size. Cold start meets the target, or the target is revised with justification |

**Exit:**
- ADRs for UI framework, scanner primitives, concurrency model and packaging.
- This plan is updated where spikes disproved assumptions.

### Phase 1 — Engine core + CLI (walk scanner)

**Depends on:** P0.

**Delivers:**
- Cargo workspace: core, win (WalkScanner), cli.
- Tree store and aggregation.
- Volume/media discovery.
- Error collection.
- Cancellation and pause.
- CSV/JSON export.
- `.tdsnap` write/read.
- Benchmark harness and dataset generator.
- CI with fixtures.
- Dependency audit: `cargo deny` licence allow-list per §11.2.
- Licensing and contribution documents (§11.1): `LICENSE.md`, `NOTICE`, `README.md` licence section, `CONTRIBUTING.md`, `CLA.md` (informational placeholder only), and `THIRD_PARTY_NOTICES` generation.

**Acceptance:**
- CLI totals match the oracle on all fixture volumes (NTFS/ReFS/exFAT/FAT32/SMB) within the documented differences.
- Memory ≤ target on the reference corpora, or the target is revised with data.
- Cancellation meets the target.
- First benchmark report published with correctness agreement.

### Phase 2 — NTFS fast path and accounting correctness

**Depends on:** P1.

**Delivers:**
- `treedock-ntfs` parser (fuzzed) and MftScanner.
- Hard-link policies.
- ADS sizes.
- Compression/sparse/cloud/reparse handling.
- "Unaccounted space" reconciliation.
- SeBackupPrivilege elevated walk.
- Elevation Option A.

**Acceptance:**
- MftScanner agrees with WalkScanner (and with the `ntfs-reader` reference) on all NTFS fixtures and on at least 3 real volumes.
- The fuzzer has run 24 h+ with no crashes.
- Automatic fallback is verified for a non-NTFS volume, non-admin, and an injected parse failure.

### Phase 3 — Desktop MVP (0.1 alpha)

**Depends on:** P1 (P2 can run in parallel; it plugs in through the same interface).

**Delivers:**
- WinUI app with the volume picker, tabs, tree list (columns, sort, tooltips), progress/cancel/pause, treemap, extension view and top files.
- Basic file actions: open, reveal, copy path, recycle with re-validation.
- Themes and the accessibility baseline.
- Settings and portable mode.
- Portable ZIP.

**Acceptance:**
- All §9 UI targets pass on the reference machine.
- FlaUI smoke suite is green.
- Narrator checklist passes for the tree list.
- No UI-thread engine calls above O(page) (code review + instrumentation).

### Phase 4 — Analysis and file management (0.5 beta)

**Depends on:** P3.

**Delivers:**
- Search/filter with query syntax.
- Duplicates pipeline with hash cache.
- Sunburst.
- Age, empty-folder and old-file views.
- Collector.
- Move/rename/permanent delete with all §8.2 safeguards, the Restart Manager lock report, the operations log and the protected-path list.
- Shell context menu.
- Explorer classic verb.
- Installer.
- Signing decision.

**Acceptance:**
- Duplicate results match an independent hash tool on fixtures, with zero false "identical" results.
- All safety tests pass: re-validation, junction-only delete, protected paths, no permanent delete without explicit confirmation.
- Bulk operations are cancellable.

### Phase 5 — History, incremental and reports (1.0 RC)

**Depends on:** P2, P4.

**Delivers:**
- Snapshot management.
- Comparison/diff views.
- History DB and trend charts.
- USN incremental refresh.
- Change monitoring.
- Elevated-helper evaluation (Option B).
- HTML, PDF and AI-summary exports.

**Acceptance:**
- An incremental refresh produces a tree identical to a full rescan on scripted change workloads (creates, deletes, renames, moves, growth, journal wrap → correct fallback).
- Exports validate against their schemas.
- The AI summary respects the redaction options.

### Phase 6 — Polish and 1.0 release

**Delivers:**
- Accessibility audit fixes.
- High-contrast treemap.
- Customisable shortcuts.
- Summary mode for huge volumes.
- Owner column.
- Duplicate → hard-link replacement (opt-in).
- ARM64 native release.
- Windows 11 modern context menu (if signing is available).
- Published benchmark report.
- User guide.
- winget submission.

**Acceptance:**
- Full regression suite passes.
- External accessibility review (or documented manual audit) is complete.
- The release is reproducible from tag following the documented steps.

### Milestone PD — PactDock CLA enforcement (repository side DONE; activation PENDING owner actions)

**Done (2026-10-20, authorised by ASPEC):**
- PactDock config, pinned workflow and agreement texts added (§11.4).
- `CLA.md`, `CONTRIBUTING.md` and `README.md` updated.
- Config validated with PactDock's own parser; hash pins verified.

**Pending:** the owner actions listed in §11.4 (store repository, GitHub App, secrets, push, ruleset, `PACTDOCK_ENABLED`).

**Acceptance:**
- An external PR from a contributor without a valid CLA cannot be merged.
- An Individual signer and an Entity-covered signer can both be verified.
- Signature records appear in the private store.
- No third-party CLA tool exists anywhere in the repository.

**Non-blocking:** no other phase depends on PD.

### Dependency graph

```
P0 ─► P1 ─► P2 ──────────────┐
       └──► P3 ─► P4 ─► P5 ◄─┘ ─► P6

PD (PactDock; integrated, awaiting activation) ─► enables merging external contributions
```

---

## 13. Technical risks, unresolved questions and recommended solutions

### 13.1 Risks

| # | Risk | Likelihood / impact | Mitigation |
|---|---|---|---|
| R1 | WinUI virtualised list or accessibility fails at scale (TreeView issues are documented [C]) | Medium / High | Never use `TreeView`. Use a flattened list with custom peers. P0-2 gate. Fallback is WPF, with the engine unchanged |
| R2 | MFT parser mis-reads edge cases (attribute lists, corrupt records), so sizes are wrong, which destroys trust | Medium / High | Cross-scanner oracle, reference crates, fuzzing, error thresholds with automatic fallback, a "verify with standard scan" command |
| R3 | Two-language codebase raises the contributor barrier | High / Medium | All logic in Rust. C# is thin presentation. Generated bindings. Clear docs. The CLI lets engine contributors avoid C# entirely |
| R4 | Windows App SDK churn or regressions (e.g. the ItemsRepeater leak fixed in 2.1.3 [C]) | Medium / Medium | Pin versions, upgrade deliberately with the perf suite, keep the UI layer thin |
| R5 | Antivirus/EDR flags raw volume reads or the unsigned binary | Medium / Medium | Code signing, submit to vendors for false-positive review, explain in docs, provide the walk-only mode |
| R6 | Memory blow-up on volumes with 50M+ entries | Low–Medium / High | Struct-of-arrays layout, optional columns, summary mode (§6.3), clear warning before a scan when the MFT record count implies more than X GB |
| R7 | Accidental data loss from file operations | Low / Critical | §8.2 safeguards, recycle-by-default, re-validation, protected paths, tests, no automatic actions |
| R8 | Cloud placeholders get hydrated by accident (e.g. duplicate hashing, Properties) | Medium / High | A central rule in the engine: any open of a file with RECALL attributes is refused unless the user explicitly chooses it. Tested with a Cloud Files test provider |
| R9 | ReFS block clone / dedup makes totals look "wrong" | High / Low–Medium | Label it explicitly. Reconciliation panel. Documentation |
| R10 | SmartScreen warnings reduce adoption | High / Medium | Free OSS signing programmes are likely closed to a noncommercial licence (§11.1), so budget for a paid option (e.g. Azure Trusted Signing [A]). Publish checksums |
| R12 | The noncommercial licence limits adoption and contributions: no workplace use, no "open source" listings, CLA friction, and the engine can't be reused | High / Medium | Plain-language README licence section. Clear process for commercial-licence enquiries, if the owner wants to offer one. CLA enforcement via ASPEC's PactDock (§11.4). Revisit the licence before 1.0 if contributor uptake is poor |
| R11 | Windows 10 end of support limits testing hardware and APIs | Medium / Low | Best-effort support tier. CI on Windows Server 2022 (Win10 kernel family) [A] |

### 13.2 Unresolved questions (and how they get resolved)

| # | Question | Resolution path |
|---|---|---|
| Q1 | ~~CLA vs DCO~~ **Resolved: CLA** (Harmony HA-CLA-I/E, Option Five, §11.3). Agreements final: ASPEC ICLA/ECLA 1.0 with Option Five (§11.3). Still open: (a) PactDock activation (§11.4 owner actions), (b) ASPEC's commercial-licence terms and process | ASPEC. Neither blocks engine/UI development |
| Q2 | Does `FSCTL_ENUM_USN_DATA` emit one record per hard link? (Matters only if it is used for tree-building) | P0-1 experiment |
| Q3 | Does `AllocationSize` from directory enumeration reflect WOF (CompactOS) compression, or is a per-file query needed? | P0-1/P2 experiment on a CompactOS VM |
| Q4 | Is `FSCTL_READ_UNPRIVILEGED_USN_JOURNAL` usable for non-admin incremental refresh? It is not documented on Learn | P5 experiment. Assume no until proven |
| Q5 | `NtQueryDirectoryFileEx` vs `GetFileInformationByHandleEx` speed | P0-1 benchmark. Prefer the Win32 API unless the gain is significant |
| Q6 | Elevation: relaunch only (A) vs CLI-as-elevated-helper (B) | A in P2. Decide B in P5 after UX testing |
| Q7 | PDF generation approach | Spike in P5. Prefer a pure-Rust writer |
| Q8 | Installer: WiX MSI vs Inno Setup | Decide in P4. Leaning WiX for enterprise MSI deployment |
| Q9 | Code-signing provider; whether to do the Windows 11 sparse-package context menu | Decide in P4 |
| Q10 | Optional integration with an installed Everything index for non-admin speed | Investigate post-1.0. The SDK licence was not confirmed, and it requires Everything to be running |
| Q11 | Does VHDX dismount/remount reliably produce cold-cache conditions? | Verify in P1 benchmark harness work |
| Q12 | Do GitHub-hosted Windows runners allow VHDX mount with admin for integration tests? | Verify in P1. Fallback: self-hosted runner or fixture directories without VHDX |
| Q13 | WinAppSDK runtime redistribution terms for portable ZIPs | Check licence terms in P0-5 |
| Q14 | MTP device scanning (WinDirStat has it [C]) | Out of 1.0 scope. Revisit after 1.0 |

### 13.3 Decisions made in this plan (with rationale summary)

| Decision | Rationale |
|---|---|
| Rust engine + WinUI 3 (.NET 10, Native AOT, unpackaged); WPF as fallback | Native Win11 UI + accessibility + AOT [C]. Rust for safe parsing and parallelism |
| In-process engine DLL, no service; elevation via relaunch, then possibly the CLI as helper | Simplest model that meets requirements. No new components |
| Pull-based paging FFI with polled progress | Avoids callback marshalling cost and AOT pitfalls. UI cost scales with the viewport, not the dataset |
| Original raw-`$MFT` parser; third-party crates only as references | Needs sizes (ENUM_USN_DATA lacks them [C]). Control over correctness, performance and security. Meets the "own implementation" requirement |
| Universal walker on `GetFileInformationByHandleEx` (`FileIdExtd` → `FileIdBoth` → `FindFirstFileEx`) | Documented Win32. Returns AllocationSize without per-file opens [C]. Works on SMB/ReFS |
| Struct-of-arrays u32-indexed arena, single-writer builder | Compact, cache-friendly, trivially serialisable. Simple concurrency |
| Custom columnar `.tdsnap` (zstd) + small SQLite for history | Fast bulk save/load. SQLite where queries matter (trends) |
| Size-then-partial-then-full (BLAKE3) duplicate pipeline | Avoids reading most bytes. Never claims identity without a full hash |
| Recycle by default, re-validation, protected paths, no automatic actions | Safety requirement |
| CLA required for all external contributions: ASPEC ICLA/ECLA 1.0 (Harmony-modelled, Option Five); enforcement via PactDock (integrated, awaiting activation) | ASPEC keeps commercial and relicensing rights in accepted contributions while contributors keep ownership (§11.3–11.4) |
| PolyForm Noncommercial 1.0.0 (owner decision, licensor ASPEC RACING PTY LTD) | Free for personal and noncommercial use, source published, and it stops others exploiting the software commercially. Accepted trade-offs: not OSI open source, no commercial use, no copyleft dependencies (§11.1) |

---

## Appendix A — Primary sources

**Microsoft documentation**

- CreateFileW (volumes, backup semantics): https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew
- FSCTL_ENUM_USN_DATA: https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_enum_usn_data
- MFT_ENUM_DATA_V1: https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-mft_enum_data_v1
- FSCTL_GET_NTFS_FILE_RECORD: https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_get_ntfs_file_record
- Change journal identifiers / privileges: https://learn.microsoft.com/en-us/windows/win32/fileio/using-the-change-journal-identifier
- FSCTL_QUERY_USN_JOURNAL: https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_query_usn_journal
- FSCTL_READ_USN_JOURNAL: https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_read_usn_journal
- USN_RECORD_V4: https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-usn_record_v4
- FindFirstFileExW: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirstfileexw
- FILE_ID_BOTH_DIR_INFO: https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_both_dir_info
- FILE_ID_EXTD_DIR_INFO: https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_extd_dir_info
- NtQueryDirectoryFileEx: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntquerydirectoryfileex
- GetCompressedFileSizeW: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getcompressedfilesizew
- File attribute constants: https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants
- Reparse point tags: https://learn.microsoft.com/en-us/windows/win32/fileio/reparse-point-tags
- Maximum path length: https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation
- ReFS block cloning: https://learn.microsoft.com/en-us/windows-server/storage/refs/block-cloning
- Dev Drive: https://learn.microsoft.com/en-us/windows/dev-drive/
- DEVICE_SEEK_PENALTY_DESCRIPTOR: https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-device_seek_penalty_descriptor
- ReadDirectoryChangesW: https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw
- IFileOperation::SetOperationFlags: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-setoperationflags
- SHFileOperationW (superseded): https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shfileoperationw
- FILE_DISPOSITION_INFORMATION_EX: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddk/ns-ntddk-_file_disposition_information_ex
- RmGetList: https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmgetlist
- Windows App SDK 2.x release notes: https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/release-notes/windows-app-sdk-2-0
- Windows App SDK system requirements: https://learn.microsoft.com/windows/apps/windows-app-sdk/system-requirements
- Self-contained deployment: https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/self-contained-deploy/deploy-self-contained-apps
- Unpackaged WinUI apps: https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/unpackage-winui-app
- Windows App SDK 1.6 (Native AOT): https://blogs.windows.com/windowsdeveloper/2024/09/04/whats-new-in-windows-app-sdk-1-6/
- Win2D: https://learn.microsoft.com/windows/apps/develop/win2d/hellowin2dworld
- WPF in .NET 9 (Fluent): https://learn.microsoft.com/en-us/dotnet/desktop/wpf/whats-new/net90
- Trimming incompatibilities (WPF): https://learn.microsoft.com/dotnet/core/deploying/trimming/incompatibilities
- WebView2 evergreen vs fixed: https://learn.microsoft.com/microsoft-edge/webview2/concepts/evergreen-vs-fixed-version

**Repositories and products**

- WinUI TreeView performance issue: https://github.com/microsoft/microsoft-ui-xaml/issues/818
- TreeSize: https://www.jam-software.com/treesize/ · https://www.jam-software.com/treesize_free/editions.shtml
- WizTree: https://diskanalyzer.com/ · https://diskanalyzer.com/guide · https://diskanalyzer.com/whats-new · https://diskanalyzer.com/donate
- WinDirStat: https://github.com/windirstat/windirstat
- FolderSize: https://github.com/TalionOfRivia/FolderSize
- Squirrel Disk: https://github.com/adileo/squirreldisk
- Everything / SDK: https://www.voidtools.com/ · https://www.voidtools.com/support/everything/sdk/
- dua-cli: https://github.com/Byron/dua-cli
- dust: https://github.com/bootandy/dust
- gdu: https://github.com/dundee/gdu
- pdu: https://github.com/KSXGitHub/parallel-disk-usage
- Czkawka/Krokiet: https://github.com/qarmin/czkawka
- Tauri IPC discussion: https://github.com/orgs/tauri-apps/discussions/11915
- window-vibrancy: https://github.com/tauri-apps/window-vibrancy
- Slint licences: https://cdn.jsdelivr.net/gh/slint-ui/slint@1.16.0/LICENSE.md
- AccessKit: https://github.com/AccessKit/accesskit
- Iced accessibility issue: https://github.com/iced-rs/iced/issues/282
- Avalonia licensing change: https://avaloniaui.net/blog/building-a-sustainable-future-for-avalonia
- csbindgen: https://github.com/Cysharp/csbindgen
- uniffi-bindgen-cs: https://github.com/NordSecurity/uniffi-bindgen-cs
- ntfs-reader: https://github.com/kikijiki/ntfs-reader
- usn-journal-rs: https://github.com/wangfu91/usn-journal-rs
- mft: https://github.com/omerbenamram/mft
- bincode 3.0.0 tombstone: https://docs.rs/crate/bincode/3.0.0
- NTFS on-disk format (non-Microsoft): https://flatcap.github.io/linux-ntfs/ntfs/
