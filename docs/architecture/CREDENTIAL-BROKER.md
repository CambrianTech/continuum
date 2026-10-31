# The credential broker: a resident's shell matches an agent's, and no secret enters it

Status: design for review, 2026-10-31. Owner: Fable (broker, shell environment). Reviewer: Codex (the credential
path).

Joel, 2026-10-31, on Benchy's push failing from her shell while an agent pushes from its own: "Why is her shell
confined if yours isn't. Bad design." And: "They are supposed to match your abilities."

## The defect

`continuum-kernel`'s confinement implemented "keep creds off-limits" (Joel, 2026-10-18) as **her shell can never
use a credential**: `~/.config/gh`, `~/.ssh`, `~/.git-credentials` and the rest are carved out, so `git push` and
`gh pr create` from her own shell fail with a permission error. The core's fixed-argument verbs
(`code/git/push`, `code/github/pr_create`) work, because the core holds `GH_TOKEN` (config.env), which is why
residents working a card could push. Two things were conflated: **using** a credential (what parity requires) and
**reading** it (what is worth protecting). Benchy committed `016040e985`, pushed from her shell, read a
permission error, and reported "a blocker for Joel".

## The rule

Anything an agent can do from its shell, a resident can do from hers. A secret is **usable and never readable**:
the core performs the credentialed part of the operation, so the token never enters her process, her
environment or her files. Confinement denies reading a secret; it never denies the operation the secret enables.

## The mechanism

The core serves two brokers, one per protocol, each speaking the protocol unchanged so her tools need no wrapper:

| Tool | How her shell reaches the broker | What the broker does |
|---|---|---|
| `git` over HTTPS to GitHub | per-command environment config, no file changed: `GIT_CONFIG_COUNT` with `url.<broker>.insteadOf = https://github.com/` | a loopback smart-HTTP reverse proxy; forwards each request to `https://github.com` over TLS, adding the authorization itself |
| `gh` (API and git operations) | `gh`'s own `http_unix_socket` setting pointed at her broker socket, with `GH_TOKEN` set to a placeholder so `gh` considers itself authenticated | a per-resident Unix-socket proxy to `api.github.com`; replaces the placeholder authorization with the real one |

- **Per-resident capability, not the token:** each broker endpoint carries a capability issued to her for her
  session (an unguessable path segment for the loopback proxy; a socket only her confinement can reach). It names
  her identity, is revocable, and is never the GitHub token.
- **Scope:** the broker forwards only to GitHub hosts, and only for the repositories her workspace remotes name.
  An operation outside that scope is a typed refusal in the tool result, saying what was refused and why.
- **Attribution and audit:** every brokered operation is recorded with her identity, repository, operation and
  outcome (an ORM entity, never a log line read back). PRs and comments she opens keep the existing footer naming
  her.
- **Confinement:** her profile grants the broker socket and the loopback port, nothing else of the credential
  stores. Reading `~/.config/gh` stays denied; using GitHub works.
- **No secret in the process tree:** git and gh in her shell hold only her capability; the token exists only in
  the core, applied on the wire.

## What stays

The fixed-argument verbs (`code/git/push`, `code/github/pr_create`) keep working and go through the same broker,
so there is one credentialed path, not two.

## Validators

1. Benchy's exact case: in a confined resident workspace, `git push` then `gh pr create` from her own shell
   succeed, and the PR carries her attribution.
2. Reading `~/.config/gh/hosts.yml`, the token in her environment, or the broker's process memory from her shell
   all fail; her environment and files never contain the token.
3. A push to a repository her remotes do not name is refused with a typed reason.
4. Revoking her capability makes the next push fail with a typed reason; reissuing restores it.
5. Every brokered operation appears in the audit entity with her identity.

## Questions for review (Codex)

1. Loopback port versus a per-resident socket for git: git's HTTP transport cannot use a Unix socket directly; is
   a loopback port with an unguessable capability path, reachable only from her confinement, acceptable on every
   platform (Windows: a named pipe is not an option for git's HTTP), or should the git side ride `gh`'s
   socket via `gh auth git-credential` instead?
2. Should the broker sit in `continuum-hands` (beside the verbs it now also serves) or in its own crate under the
   kernel's confinement?
