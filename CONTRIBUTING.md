# Contributing

**English** | [Français](CONTRIBUTING.fr.md)

UltraRushFTP is proprietary software in a private repository: contributions are welcome
from collaborators invited by the owner, under the terms of the [LICENSE](LICENSE)
(contributions become part of the Software).

## Set-up

1. Windows 10 or later, [Rust](https://rustup.rs) (the version is selected by
   `rust-toolchain.toml`: 1.99.1 with `clippy`, `rustfmt` and the 32-bit target),
   Visual Studio Build Tools (MSVC and the Windows SDK), PowerShell 7.
2. Optional: `cargo install --locked cargo-deny` to run the dependency checks locally.
3. Check that everything works:

```text
cargo test --workspace --all-features
cargo run -p urftp-proto --example dev_server   # local FTP server, user test / secret
pwsh scripts/validate-server.ps1 -SelfTest      # validation scenario on two local servers
cargo run -p urftp-gui
```

## Before every commit

```text
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check                                  # when dependencies changed
pwsh scripts/generate-dependencies.ps1            # when dependencies changed
```

Run them all before pushing: on every push and pull request CI only checks the
formatting and the dependencies (Linux). The Windows job (Clippy, tests, both DLLs,
installer) runs on demand — *Actions › CI › Run workflow*, or
`gh workflow run ci.yml --ref <branch>` — because Windows minutes count double on a
private repository; the *Release* workflow runs the tests again. A red CI blocks the
merge.

## Rules

- **Language**: code, comments, identifiers, commit messages in English. Every
  user-visible text goes through `urftp-i18n` (`t!("key")`), with the key added to
  **both** `locales/en.json` and `locales/fr.json` (tests check it).
- **Safety**: no `unsafe` outside `urftp-sys`; there, every block needs a `// SAFETY:`
  comment explaining why it is sound.
- **Errors**: no `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` in production code
  (denied by Clippy). Return typed errors (`thiserror`); messages meant for the user are
  typed so that the interface can translate them.
- **Tests**: every change comes with unit tests, and integration tests against the local
  test servers (`crates/urftp-proto/tests/common`) for protocol behaviour. Tests never
  reach the Internet and never talk to a running FTPRush (use `URUSHFTP_IPC_CLASS`).
- **Interface**: take colours from `theme::colors(ui)` (never a literal colour), and use
  the helpers of `theme.rs` — `icon_button` for a symbol button, `name` for a control
  without visible text, `labelled` / `grid_row` to link a label to its field,
  `single_line` for text fields, `primary_button` / `danger_button`. The theme and
  interface tests (`cargo test -p urftp-gui`) fail on a missing name, a target under
  24 px or a contrast below WCAG 2.2 AA; add a test in `app/ui_tests.rs` for new keyboard
  behaviour.
- **Documentation**: keep the English and French documents in step (`*.md` / `*.fr.md`).
  Update [docs/OPTIONS.md](docs/OPTIONS.md) for every new setting,
  [CHANGELOG.md](CHANGELOG.md) under *Unreleased*, and the screenshots with
  `pwsh scripts/screenshots.ps1` when the interface changes.
- **Dependencies**: only crates.io, permissive licences accepted by `deny.toml`; explain
  why a new dependency is needed in the pull request and in
  [docs/DEPENDENCIES.md](docs/DEPENDENCIES.md).

## Data protection

Never commit real site books, exports, passwords, keys or server addresses. The
`.gitignore` excludes `thirdparty_source/`, `sites.json`, `settings.json`, `stats.json`,
`RushSite.xml`, `sitemanager.xml` and `*.ftp`; check `git status` before committing
anyway. Screenshots and examples use demo data only (`127.0.0.1`, `example.com`).

## Workflow

1. Work in small steps, each one validated (build, tests, manual check) before the next.
2. One commit per step, message in English, imperative mood, first line under 72
   characters, details in the body.
3. Pull request against `main` with a description of what changed and how it was tested.

## Releases

1. Move the *Unreleased* entries of `CHANGELOG.md` (and `CHANGELOG.fr.md`) to a new
   `## [x.y.z] - YYYY-MM-DD` section; update the links at the bottom.
2. Set `version = "x.y.z"` in `[workspace.package]` of `Cargo.toml`.
3. Commit, then tag and push: `git tag -a vx.y.z -m "UltraRushFTP x.y.z"` and
   `git push origin vx.y.z`. The *Release* workflow checks the version, runs the tests,
   builds, then `scripts/package.ps1` writes the zip and the installer (Inno Setup,
   `installer/UltraRushFTP.iss`) with their checksums; they are published with the
   changelog section as notes.

Locally, after the release build: `pwsh scripts/package.ps1 -Version x.y.z` (Inno Setup 6
needed for the installer, or `-NoInstaller`). The icon is drawn by
`python -I scripts/make-icon.py`.

### Code signing

`scripts/sign.ps1` signs with `signtool` (SHA-256, RFC 3161 timestamp) when these
repository secrets exist; without them the release is published unsigned:

| Secret / variable | Content |
|---|---|
| `SIGN_PFX_BASE64` (secret) | The code-signing certificate `.pfx`, base64-encoded |
| `SIGN_PFX_PASSWORD` (secret) | Its password |
| `SIGN_TIMESTAMP_URL` (variable, optional) | Timestamp server; default `http://timestamp.digicert.com` |

The executables, both DLLs and the installer are signed.
