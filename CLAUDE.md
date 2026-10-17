# Yurelay engineering contract

Yurelay is a macOS 14+ Tauri 2 / Rust / Svelte 5 application. Read this file and
CONTRIBUTING.md before editing. AGENTS.md points to this same contract.

## Architecture boundaries

- Translation/SSE parsing, SensitiveGuard, vocabulary validation, merge rules and
  SQLite persistence belong in `crates/core`. Keep business rules out of Svelte
  components and the Tauri command glue.
- `src/` displays state and handles interaction. `src-tauri/src/commands.rs`
  validates the IPC boundary, forwards calls and maps errors. Native OS access
  belongs in `crates/platform`; lifecycle/permission orchestration belongs in the
  desktop layer. Move existing legacy business logic into core when changing it;
  do not expand that debt.
- Streamed translation must remain visible while vocabulary parsing continues.
  At `---VOCAB_JSON---`, immediately finish the visible translation stream and
  process metadata separately. This is a UI completion boundary, not permission
  to drop the remaining network stream or persist incomplete metadata.
- 生词候选到达时严禁自动展开面板顶动译文布局。
  Vocabulary UI must preserve the reading anchor when the left sidebar opens.
  Never reopen a manually collapsed sidebar during the same task attempt.

## Security and compatibility

- Every selection/OCR ingress must run SensitiveGuard before text reaches UI or
  network. Explicit user bypass applies only to that source, never globally.
- Store API keys only in the system Keychain. Never persist keys in JSON,
  SQLite, fixtures, logs, screenshots or commits. In-memory edits are temporary.
- Do not log captured text, clipboard contents or full upstream errors.
- Preserve bundle ID, Keychain service names, SQLite migrations, existing user
  settings and explicit save/close behavior. No destructive cleanup of user data.
- Treat model output as untrusted: sanitize Markdown; validate vocabulary locally;
  never execute returned instructions or automatically follow links.
- Do not bundle local reference repositories, build products or private config.
  Easydict is read-only reference material and must remain ignored.
- Project code is GPL-3.0-only. Review new dependency licenses and preserve notices.

## UI, performance and accessibility

- Do not delay the first translation chunk for animation or vocabulary extraction.
- Respect reduced motion, keyboard focus, localized labels, dark mode and narrow
  windows. Match the existing typography and spacing; avoid unrelated redesigns.
- Prefer browser/native platform capabilities over additional dependencies.
- Do not claim zero latency, perfect secret detection or provider availability.

## Mandatory Self-Check Protocol

Any AI programming assistant that changes code MUST actively run
`./scripts/verify.sh` in the terminal. Visual inspection alone is not completion.
Fix compilation errors, Clippy warnings, type errors and failing tests until the
script exits with code 0 before reporting completion. Never weaken checks, remove
assertions, broadly suppress warnings or skip tests merely to obtain a green run.
If an external prerequisite prevents verification, report the exact blocker and
leave verification explicitly incomplete; never fabricate a successful run.

Add behavioral tests for new core logic. Use synthetic inputs and fake credentials.
For native changes also run `sh scripts/test-native.sh` and exercise affected macOS
flows. Browser mocks do not verify TCC permissions, Keychain or capture APIs.

Work on feature branches and submit PRs targeting `main`; require a successful
`macOS verification` check before merging. Do not push directly to `main` or
bypass the PR flow merely because the active account is an administrator. Use an
administrator exception only when the maintainer explicitly requests that action.

Before committing, run `python3 scripts/audit.py --staged` and inspect the staged
file list. Do not push, create a remote repository, publish a release or message
others without user authorization. Keep Git identity configuration repository-local.

## Communication

Use the user's current language. Report the usable result, verification evidence
and remaining limitations. Act autonomously within authorization; do not hand
executable work back to the user. Distinguish facts from unverified assumptions.
