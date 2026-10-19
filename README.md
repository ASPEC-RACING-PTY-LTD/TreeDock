# TreeDock

TreeDock is a fast, accurate disk-space analyser for Windows. It shows where your space went and explains why the numbers add up, or why they don't. It has no subscriptions, ads, telemetry or accounts, and it works completely offline.

> **Status:** early development. See [MASTER_PLAN.md](MASTER_PLAN.md) for the architecture and roadmap.

## What works today

- **Desktop app (WinUI 3, preview).**
  - Path bar with drive list, Scan button and Enter to scan.
  - Folder tree with size bars, plus a drives panel showing free space.
  - Chart tab: treemap, sunburst or bars. Also Details (sortable), Extensions and Top files tabs.
  - Right-click menu: Open, Open file location, Properties, Copy path or name, Scan this folder, Move to Recycle Bin (with re-validation).
  - Double-click a file to show it in File Explorer.
  - Scanning a whole drive, or a folder you can't modify, asks for administrator rights (UAC). If you decline, the scan continues in standard mode.
- **Scanning engine (Rust).**
  - Parallel directory walker for NTFS, ReFS, exFAT, FAT32, removable drives and SMB shares.
  - NTFS MFT fast path when running as administrator. It is unit-tested on synthetic records but has not yet been run against a real volume.
  - Logical and allocated sizes. Junctions and symlinks are not followed. Cloud placeholders are never downloaded.
- **`treedock-cli`.** Scan, CSV/JSON export, `.tdsnap` snapshots, top files and types, benchmarking.

## Building

You need the Rust toolchain pinned in `engine/rust-toolchain.toml` and the MSVC build tools.

```bash
cd engine
cargo build --release
cargo test
```

### Examples

```bash
engine/target/release/treedock-cli volumes
engine/target/release/treedock-cli scan C:\Users --top 20 --out users.tdsnap
engine/target/release/treedock-cli export users.tdsnap --format csv --out users.csv
```

## Licence

TreeDock is **source-available**, not open source. It is licensed under the
[PolyForm Noncommercial License 1.0.0](LICENSE.md).

Copyright (c) 2026 ASPEC RACING PTY LTD. See [NOTICE](NOTICE).

In plain language (the [licence text](LICENSE.md) is what actually applies):

- **Free for noncommercial use.** That covers personal use, hobby projects, study and research. Charities, schools and universities, public research organisations, public health and safety organisations, environmental organisations and government institutions may also use it.
- **Allowed for those purposes:** using, copying, modifying, forking and sharing TreeDock, including modified versions. You must pass on the licence terms and the `Required Notice:` line.
- **Commercial use needs permission.** Using TreeDock in or for a business, including internal use at work, requires separate permission or a commercial licence from ASPEC RACING PTY LTD.

PolyForm Noncommercial is not an OSI-approved open-source licence, so please don't describe TreeDock as "open source".

## Contributing

All external contributions require a signed Contributor Licence Agreement. See [CONTRIBUTING.md](CONTRIBUTING.md) and [CLA.md](CLA.md).

CLA signing is handled by [PactDock](https://github.com/ASPEC-RACING-PTY-LTD/PactDock). **External pull requests can't be merged yet:** the agreements are still under legal review, so enforcement is configured but switched off.
