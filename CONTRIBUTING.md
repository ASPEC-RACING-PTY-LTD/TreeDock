# Contributing to TreeDock

Thank you for your interest in TreeDock.

## Current status: external contributions are on hold

Every external contribution requires a signed [Contributor Licence Agreement](CLA.md). The agreements are final. CLA signing is not switched on yet.

Until it is:

- You can open issues, report bugs and suggest features.
- You can open pull requests for discussion, but they **cannot be merged**.
- Please don't send large changes yet. They may not be mergeable under the final CLA.

## Licence of contributions

TreeDock is **source-available** under the [PolyForm Noncommercial License 1.0.0](LICENSE.md). It is not open source.

By signing the CLA, you will keep ownership of your contribution and grant ASPEC RACING PTY LTD the rights described in [CLA.md](CLA.md).

Once signing is switched on, each pull request will be checked automatically and you will be told how to sign. Pull requests will only be mergeable once every contributor is covered.

## Development

Read [MASTER_PLAN.md](MASTER_PLAN.md) first. It describes the architecture, roadmap and quality bar.

- **Engine:** Rust, in `engine/` (a Cargo workspace).
- **Before submitting:**
  - `cargo fmt --all`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`
- **Tests:** every behaviour change needs a test.
- **Size accounting and file operations are safety-critical.** Changes there need tests covering hard links, reparse points, errors and cancellation.
- **Dependencies:**
  - Use permissive licences only (MIT, Apache-2.0, BSD, Zlib, BSL-1.0, ISC, Unicode, CC0).
  - No GPL, AGPL or LGPL.
  - No noncommercial-only components.
  - No network-capable crates in the product.
- **Source files:** start each new file with `SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0`.
- **Wording:** call TreeDock "source-available", never "open source".

## Security

Please report vulnerabilities privately to the maintainers. Do not open a public issue for them.
