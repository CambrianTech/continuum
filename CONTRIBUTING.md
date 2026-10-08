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

## Developer setup (separate from product installation)

This section is for engineers building or contributing to Continuum. Product
users do not need GitHub workflow permissions or a compiler to consume published
releases. Developer setup does not establish that the product installer works;
release acceptance must exercise the public installation and upgrade path.

Read [AGENTS.md](AGENTS.md) and [CLAUDE.md](CLAUDE.md) first. Start a worktree from
current `origin/canary`, preserve other owners' work, and coordinate compiler and
deployment ownership through AIRC before building. Use the existing shared Cargo
target directory; never clear a shared cache to make a test pass.

Run the non-mutating developer inventory from the checkout:

```powershell
./tools/scripts/developer-setup.ps1
```

On macOS/Linux, use `pwsh -File tools/scripts/developer-setup.ps1` if PowerShell
is available, or follow the same prerequisites below directly. The inventory is
not an installer or a claim that native compilation will succeed.

- Common tools: Git, Rust through rustup, CMake, Node/npm, GitHub CLI, and AIRC.
  Follow the [README development instructions](README.md) and existing
  `npm run setup:rust` / `npm run setup:git-hooks` scripts for setup.
- Windows native work: MSVC C++ Build Tools and Windows SDK, LLVM/libclang,
  plus CUDA when building the CUDA backend. Use the repository's documented
  developer environment; a compiler absent from an ordinary shell's PATH does
  not necessarily mean its SDK is missing.
- macOS native work: Xcode Command Line Tools; Linux native work: a C/C++
  toolchain and the system dependencies listed in the README. GPU backend
  requirements depend on the backend being developed.
- Initialize required submodules with `git submodule update --init --recursive`
  in your own checkout. Install JavaScript workspace dependencies with
  `npm install` when developing clients. Neither step should restart serving.
- Authenticate your own GitHub account. Use `airc gh run -- ...` for GitHub
  operations so the team request governor is respected. Never paste tokens into
  chat, commits, or logs.

### Contributors changing GitHub Actions workflows

Only developers publishing workflow changes need this additional authorization;
ordinary code contributors and product users do not. With an existing GitHub CLI
OAuth login, run in a normal, **non-administrator** PowerShell:

```powershell
./tools/scripts/developer-setup.ps1 -AuthorizeWorkflows
```

Complete GitHub's browser consent. The script requests the `workflow` scope via
the existing AIRC governor, verifies authentication, and restores process-local
environment overrides even on failure. It does not alter services, persistent
environment variables, repository permissions, or global Git configuration.
Authorization does not grant repository write access; use your permitted fork or
upstream contribution path.

`GH_TOKEN` and `GITHUB_TOKEN` take precedence over the refreshed keyring login.
If your terminal supplies an older token, use a separate terminal without those
overrides for the push. For a single push using the refreshed GitHub CLI helper:

```powershell
# Only this terminal is affected; do not remove service or saved credentials.
Remove-Item Env:GH_TOKEN -ErrorAction SilentlyContinue
Remove-Item Env:GITHUB_TOKEN -ErrorAction SilentlyContinue
git -c credential.helper= -c 'credential.helper=!gh auth git-credential' push -u origin HEAD
```

Automation using managed tokens should obtain the necessary workflow-write
permission through its credential owner instead of starting interactive OAuth.
If the governor asks you to wait, honor it; do not bypass it with direct `gh`.

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
