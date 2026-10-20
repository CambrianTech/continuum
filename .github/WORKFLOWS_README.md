# Continuum CI/CD

This directory contains GitHub-specific configuration files for Continuum's CI/CD pipelines and other GitHub integrations.

## Workflows

Required on canary: `cargo test -p continuum-core --lib`, `cargo check windows-msvc (lib + tests)`, and `ts-rs binding drift detector`, all in `continuum-rust-tests.yml`.

| Workflow | What it guards |
|---|---|
| `continuum-rust-tests.yml` | The core: lib tests, the Windows check, ts-rs drift, CLI handoff regressions. Rust jobs skip when no Rust changed. |
| `source-hygiene.yml` | Fast source ratchets. |
| `pipefail-ratchet.yml` | Shell scripts may not hide pipeline failures (`scripts/ratchets/pipefail-baseline.txt`). |
| `carl-install-smoke.yml` | The Linux Docker install, end to end. |
| `host-detect-matrix.yml` | Installer hardware detection on Linux, macOS and Windows. |
| `manifest-projection-drift-guard.yml` | The install manifest matches its projections. |
| `docker-images.yml` | Verifies the published images (main). |
| `dependencies.yml` | npm audit of the web client (main and weekly). |
| `promote-main.yml` | Moves main to canary's green tip daily. |
| `pr-labeler.yml`, `auto-close-queue-cards.yml`, `stale.yml` | Housekeeping. |

The legacy Node tree (`legacy/`) is reference material only. No workflow builds, lints or guards it.

## Issue & PR Templates

- `ISSUE_TEMPLATE/bug_report.md`: Template for bug reports
- `ISSUE_TEMPLATE/feature_request.md`: Template for feature requests
- `PULL_REQUEST_TEMPLATE.md`: Template for pull requests

## GitHub Configuration

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [GitHub Community Contributing Guide](https://github.com/github/docs/blob/main/CONTRIBUTING.md)