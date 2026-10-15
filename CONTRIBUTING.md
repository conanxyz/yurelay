# Contributing to Glossa

Glossa currently targets macOS 14+, with Rust 1.92.0, Node.js 24, Tauri 2 and
Svelte 5. Tooling also requires Python 3.11+. Install Xcode Command Line Tools, rustup and npm before starting.

## Local workflow

1. Run `npm ci` and `npx playwright install chromium`.
2. Create a focused branch; describe the observable problem before changing code.
3. Keep changes within the architecture below and add tests for new core logic.
4. Run `./scripts/verify.sh`. Resolve every failure before submitting a PR.
5. For native changes, also run `sh scripts/test-native.sh` and exercise the actual
   macOS flow. Use synthetic documents for screenshots and capture tests.

`verify.sh` runs repository hygiene, Rust formatting, Clippy with `-D warnings`,
workspace tests, Svelte types, production frontend build, unit and browser tests.
It creates compact local caches in `artifacts/verify.noindex`; deleting that
cache after verification is safe. CI overrides the target directory for caching.

## Architecture

| Directory | Responsibility |
| --- | --- |
| `src/` | Svelte views, local UI state, interaction and sanitized rendering |
| `src-tauri/` | IPC validation/forwarding, desktop lifecycle and window orchestration |
| `crates/core/` | Translation/SSE, SensitiveGuard, vocabulary rules, SQLite and task state |
| `crates/platform/` | Native capture, OCR, dictionary, language tools and Keychain |

Core business rules must not move into components or `commands.rs`. Preserve
existing configuration, storage migrations, bundle ID and Keychain names.
At the vocabulary marker, finish displaying translation without waiting for
metadata parsing. Never auto-expand vocabulary UI in a way that shifts reading text; preserve the left sidebar anchor and the user's manual collapse for the same attempt.

## PRs

Explain the user-visible behavior, motivation, validation and known limitations.
Include before/after screenshots for visible changes, using synthetic content.
New core behavior needs regression tests, including failure or cancellation paths
where relevant. Report native and mocked browser coverage separately.

Do not commit reference repos (`Easydict/`), build outputs, screenshots of private
content, databases, logs, environment files or credentials. Use fake fixture keys.
Run `python3 scripts/audit.py --staged` before committing. For an initial import or
security-sensitive change, also run Gitleaks with full redaction on an export of
the staged source; the lightweight built-in scanner is not comprehensive.

Changes to dependencies must include license review and updated lockfiles/notices.
Do not broadly disable Clippy, remove assertions or skip checks to make CI pass.
AI assistants must follow [AGENTS.md](AGENTS.md), which shares the same content as
CLAUDE.md. Contributions are provided under GPL-3.0-only, with retained third-party
notices; do not submit code you are not permitted to distribute.

## Release maintenance

All release jobs run on `macos-latest`. The workflow validates version equality
across npm, Cargo and Tauri before building Apple Silicon and Intel targets.
After CI passes, a `vX.Y.Z` tag or manual run on main creates/updates a draft
prerelease, uploads `.dmg`, `.app.tar.gz` and per-architecture SHA-256 checksums.
The draft avoids presenting untested artifacts as stable downloads.

Before publishing, test both architectures, verify checksums and signatures,
check the source tag, review privacy-safe screenshots and release notes, and
record any missing notarization/hardware coverage. Ad-hoc signing is the current
default. Apple certificate/notarization secret names are documented in the
workflow; never commit signing material. Future notarization requires a Developer
ID identity and must also override the current ad-hoc identity in Tauri config.

Configure branch protection to require a PR, a successful `macOS verification`
check and resolved conversations before merging `main`, and to disallow force
pushes and branch deletion. Maintainers may retain an administrator exception.
Contributors and AI assistants use the PR flow unless a maintainer explicitly
requests an administrative exception. Workflow YAML alone
cannot enforce a merge block. Enable private vulnerability reporting and secret
scanning if available. Dependabot updates are configured for review, not auto-merge.

This setup does not create a GitHub repository or publish a release on its own.
