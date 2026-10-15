## Motivation

What concrete problem does this solve? Include a before/after example.

## Changes

-

## UI evidence

For visible changes, attach before/after screenshots with synthetic data, including
relevant dark/narrow layouts. Otherwise mark not applicable.

## Validation

Commands run and results; distinguish browser mocks from native macOS verification.

- [ ] `./scripts/verify.sh` exited 0 (includes Clippy, workspace tests and frontend checks).
- [ ] New core logic has behavioral tests.
- [ ] Native changes passed `sh scripts/test-native.sh` and affected desktop flows, or are not applicable.
- [ ] Existing settings, Keychain identities and database compatibility are preserved.
- [ ] SensitiveGuard runs before affected capture data reaches UI/network.
- [ ] No secrets, local paths, databases, generated bundles or reference repos are included.
- [ ] Dependency/license notices and documentation are updated where needed.

## Risks / limitations
