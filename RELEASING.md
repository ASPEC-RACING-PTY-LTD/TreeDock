# Releasing TreeDock

Releases are built by GitHub Actions ([`.github/workflows/release.yml`](.github/workflows/release.yml)) when a version tag is pushed. Each release contains:

| File | What it is |
|---|---|
| `TreeDock-X.Y.Z-x64-setup.exe` | Installer (Inno Setup). Per-user by default, no admin needed; the setup dialog also offers an all-users install |
| `TreeDock-X.Y.Z-x64-portable.zip` | The same app as a folder: extract and run `TreeDock.exe`. Includes `treedock-cli.exe` |
| `SHA256SUMS.txt` | SHA-256 of each file |

## Version numbers

- Versions are `MAJOR.MINOR.PATCH` and tags are `v` + version, starting at **`v0.1.0`**.
- **PATCH is a single digit.** After `0.1.9` comes `0.2.0`, never `0.1.10`.
- Every tag must be newer than the previous release.
- The app (`app/TreeDock.App/TreeDock.App.csproj`) and engine (`engine/Cargo.toml`, `engine/Cargo.lock`) versions must equal the tag.

[`tools/version.py`](tools/version.py) applies these rules. The release workflow runs `check` and fails on a tag that breaks them.

## Making a release

```bash
python tools/version.py next
```

Prints the next version (add `--minor` to jump to the next minor). Then set it, commit, tag and push:

```bash
python tools/version.py set 0.1.1
```

```bash
git commit -am "Release 0.1.1"
```

```bash
git tag v0.1.1
```

```bash
git push origin main v0.1.1
```

The first release, `v0.1.0`, needs no `set`: the source is already at 0.1.0.

## Building locally

Needs Rust (pinned in `engine/rust-toolchain.toml`), the .NET 10 SDK, the MSVC build tools and, for the installer, [Inno Setup 6](https://jrsoftware.org/isinfo.php).

```bash
powershell -ExecutionPolicy Bypass -File tools/build_release.ps1 -Version 0.1.0
```

Add `-SkipInstaller` to build only the portable ZIP. Output goes to `dist/`.

## Not automated yet

- ARM64 builds and the CLI-only ZIP.
- Code signing. Builds are unsigned, so SmartScreen warns on first run (MASTER_PLAN §11.5).
