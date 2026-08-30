# Contributing to Continuum

Seven forks in, we write this down. Whether you're fixing a typo, upstreaming a
patch from your fork, or teaching your own citizens to work on the codebase —
welcome. This page is the short map; the deep laws live in
[CLAUDE.md](CLAUDE.md) and [docs/ARCHITECTURE-RULES.md](docs/ARCHITECTURE-RULES.md).

## The shape of a good contribution

- **Branch from `canary`, PR to `canary`.** `main` is the beta gate and merges
  from canary only.
- **Every claim carries a receipt.** A fix PR shows the failing behavior and the
  passing behavior — probe output, test output, or a `perception/observe`
  screenshot. This repo's culture is receipts over assertions, and reviews go
  fast when the evidence is attached.
- **Every test justifies itself.** One `#[cfg(test)] mod tests` per file; each
  test carries a `// what this catches:` line naming the invariant or the
  regression it pins. Trivial-getter tests are declined kindly.
- **No suppressions.** `#[allow(...)]`, `@ts-ignore`, swallowed errors, and
  `--no-verify` don't land. If a hook or a ratchet fails, the failure is the
  finding — fix the cause or ask in the PR.
- **Ratchets only go down.** Source-hygiene counters (unwraps, boundary
  serializations, README link hygiene) may never rise; touching old code is a
  chance to lower them.
- **Deploy-verify if you touch the core.** `continuum reboot` then check
  `continuum ping`'s sha matches your commit before believing any behavior
  change. A fix you can't prove reached the running binary is not yet a fix.

## Filing issues

Use the templates — the bug template asks for `continuum ping` output (the
version trio) because "what exactly were you running" is half of every
diagnosis. Benchmarks issues want the `benchmark/scoreboard` regime line.
Feature ideas and design conversations are welcome in Discussions; so are
questions from other AI projects — precedent exists (#1729).

## For AI contributors

Continuum is built daily by a human-AI team, and contributions from agent
sessions (Claude Code, Codex, or your own) are first-class. Two requests:
identify the driving human in the PR (accountability, not gatekeeping), and
hold your agents to the same receipt discipline — the reviewers here include
AIs who will actually check.

## License

AGPL-3.0. Contributions are accepted under the same license. The genome
commons additionally carries its own covenant (`continuum genome/sharing`) —
code license and gene covenant are separate consents.
