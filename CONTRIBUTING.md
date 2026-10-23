# Contributing to Yurelay

Yurelay currently targets macOS 14+, with Rust 1.92.0, Node.js 24, Tauri 2 and
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
The displayed product and build packages use Yurelay/yurelay. Keep the legacy
`com.autodict.desktop` bundle ID and `com.autodict.desktop.providers` Keychain
service stable: they identify existing application data and credentials. Build
scripts resolve paths from their own location, so the checkout may be renamed.
At the vocabulary marker, finish displaying translation without waiting for
metadata parsing. Never auto-expand vocabulary UI in a way that shifts reading text; preserve the left sidebar anchor and the user's manual collapse for the same attempt.

## PRs

Explain the user-visible behavior, motivation, validation and known limitations.
Include before/after screenshots for visible changes, using synthetic content.
New core behavior needs regression tests, including failure or cancellation paths
where relevant. Report native and mocked browser coverage separately.

Do not commit reference repos (`Easydict/`), build outputs, screenshots of private
content, databases, logs, environment files or credentials. Use fake fixture keys.
Keep development notes in ignored `docs/` or `.ai-notes/`. Public usage guides `docs/FAQ.md` and `docs/FAQ_cn.md`, plus shareable product
and PR media under `docs/images/`, are tracked from that directory.
Run `python3 scripts/audit.py --staged` before committing. For an initial import or
security-sensitive change, also run Gitleaks with full redaction on an export of
the staged source; the lightweight built-in scanner is not comprehensive.

Changes to dependencies must include license review and updated lockfiles/notices.
Do not broadly disable Clippy, remove assertions or skip checks to make CI pass.
AI assistants must follow [AGENTS.md](AGENTS.md), which shares the same content as
CLAUDE.md. Contributions are provided under GPL-3.0-only, with retained third-party
notices; do not submit code you are not permitted to distribute.

## Release maintenance

Start a release from **Actions → Release → Run workflow** on `main`, with a `tag`
input such as `v0.1.0` matching npm, Cargo and Tauri. You can first save a release
draft with your own title, notes and prerelease flag, or let the workflow create a
prerelease draft. Saving a draft alone does not start a build. Do not click
**Publish release** to start packaging: that button immediately exposes the page.

The workflow runs CI, creates or reuses a private draft pinned to the checked
commit, and builds Apple Silicon and Intel packages. All jobs run on
`macos-latest`. Both matrix builds must succeed before the final publication job
can run. It requires each architecture's `.dmg`, `.app.tar.gz` and SHA-256 manifest,
downloads those six files, verifies every checksum, then publishes the completed
draft automatically. Build failure, cancellation, missing assets, checksum errors
or a changed source target leave the draft unpublished. Existing titles, notes
and prerelease choices are preserved; early releases should remain **Pre-release**.

Pushing a matching `vX.Y.Z` tag also starts this build-then-publish flow. Merge
workflow changes into `main` before creating the tag, so the workflow exists in
the tagged commit. An existing tag must point to the build commit; published
releases are never rebuilt or overwritten by this workflow. Use **Re-run failed
jobs** to resume a failed draft build. Same-version runs are serialized. Tags
created when the workflow publishes with `GITHUB_TOKEN` do not trigger another
build, and the workflow does not subscribe to release-publication events.

Before promoting a preview to a stable release, test both architectures and
verify checksums and signatures. Review the source tag and release notes before
publishing, and record any missing notarization/hardware coverage. Ad-hoc signing
is the current default. Apple certificate/notarization secret names are documented
in the workflow; never commit signing material. Future notarization requires a
Developer ID identity and must also override the ad-hoc identity in Tauri config.

Configure branch protection to require a PR, a successful `macOS verification`
check and resolved conversations before merging `main`, and to disallow force
pushes and branch deletion. Maintainers may retain an administrator exception.
Contributors and AI assistants use the PR flow unless a maintainer explicitly
requests an administrative exception. Workflow YAML alone
cannot enforce a merge block. Enable private vulnerability reporting and secret
scanning if available. Dependabot updates are configured for review, not auto-merge.

This setup does not create a GitHub repository. Starting a Release workflow is
a request to publish its completed draft automatically after all checks succeed.
