# The credential broker: a resident's shell matches an agent's, and no secret enters it

Status: design for review, 2026-10-31; revised 2026-11-03 for Cormac's review (the scope comes from core-held state,
and the capability is a per-resident endpoint only her confinement reaches, never a value in her environment). Owner:
Fable (broker, shell environment). Reviewers: Cormac, Codex (the credential path).

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
| `git` over HTTPS to GitHub | `url.http://127.0.0.1:<her port>/.insteadOf = https://github.com/` (environment config, no file changed). The port is hers: her confinement profile allows outbound to that one localhost port and no other resident's | a loopback smart-HTTP reverse proxy on her port; forwards each request to `https://github.com` over TLS, adding the authorization itself |
| `gh` (API and git operations) | `gh`'s own `http_unix_socket` setting pointed at her broker socket, with `GH_TOKEN` set to a placeholder so `gh` considers itself authenticated | a per-resident Unix-socket proxy to `api.github.com`, the socket reachable only in her profile; replaces the placeholder authorization with the real one |

- **Per-resident capability, not the token, and not a secret:** her capability is an endpoint only her
  confinement can reach: her own loopback port (the sandbox profile allows `network-outbound` to that one localhost
  port) and her own socket. Which endpoint a connection arrived on names who she is; nothing in her environment or
  files needs to stay secret. An unguessable value carried in the environment would not do: every resident and the
  core run as one OS user, and on macOS a same-user process reads another's environment (`ps -E`; Cormac, the
  IntelMac, 2026-11-03), so resident A could lift B's value and push as B, with the audit naming B. The capability is
  revocable (the port or socket closes) and is never the GitHub token. The attribution holds only for confined
  processes: an unconfined same-user process (anything outside every resident's profile, the core's own children
  included) can also reach her port, so the audit's "her" means "a process her profile admits or one outside all
  confinement". The core spawns nothing unconfined that a resident can drive, and the audit records the peer
  process of each connection (`LOCAL_PEERPID` / `SO_PEERCRED`) beside her identity, so an outside caller is
  visible rather than attributed to her (Cormac on #5628, 2026-11-03). On Windows the same per-resident endpoint is
  scoped by the firewall or an AppContainer; until that exists there, the broker answers with a typed refusal
  naming it, never a shared endpoint.
- **Scope, from what the core records:** the broker forwards only to GitHub hosts, and only for the repositories
  the core has recorded for her: the cards she holds and her workspace's allocation. Her git config is hers to
  write, so her remotes never decide scope (a `git remote add` of any repository the token can write would widen it).
  An operation outside the recorded scope is a typed refusal in the tool result, saying what was refused and why.
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
   all fail; her environment and files never contain the token. Connecting to another resident's port or socket
   from her shell fails (her profile does not allow it), and so does reading another resident's process environment.
3. A push to a repository the core has not recorded for her is refused with a typed reason, including one she added
   as a remote herself (`git remote add`).
4. Revoking her capability makes the next push fail with a typed reason; reissuing restores it.
5. Every brokered operation appears in the audit entity with her identity.

## Decided in review

1. **Git over a socket** (Cormac, 2026-11-03): not through `gh auth git-credential`, which prints the real token to
   git and so into her process. Git's HTTP transport reaches the loopback proxy on her own port, the port being the
   capability her profile alone can reach.
2. **Where the broker lives:** its own crate under the kernel's confinement, since it is the only code that holds the
   token; `continuum-hands`' verbs call it like any other client.
