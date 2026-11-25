# A product for users, not us

Joel, 2026-11-14: "How would you build a successful product for users NOT us." "Time to take this
seriously. Refine refine refine." "We are a half year behind on the desktop."

Every node we run is a repo checkout, configured by hand, started by shell scripts, watched by agents.
What works there is rigged to us; a stranger gets none of it. This plan makes our nodes live by the
user's path, so rigging breaks us first, the same day.

## What we found (2026-11-14, receipts)

| Finding | Receipt |
|---|---|
| The README's one-command install fails for everyone | `bootstrap.sh:124` runs `cd src`; no `src/` on main or canary (`git ls-tree origin/main -- src` is empty) |
| The install pulls Node, Python ML and Postgres and launches with `npm start` | `install.sh` (1,187 lines) |
| The lifecycle is shell | 118 `.sh`/`.ps1` files, 26,756 lines; `start-server.sh` (1,247) behind every start/reboot; 2,751 lines of PowerShell behind the Windows service; 43 Rust spawns of a script or shell (card 71b1aeac) |
| The install gate proves a process started | `carl-install-smoke`: Linux/Docker only, passes when the core socket binds; web client and chat checks are advisory, skipped by default |
| Nothing cleans up | 209 GB in `~/.continuum` on the M5: 45 GB of 1,007 idle peer workspaces, a 12 GB airc events DB, 37.6 GB cargo target, 25 GB untouched genome, 7,414 UUID homes for 16 citizens (card 12ad2a0a, #6028) |
| The desktop was blank while connecting | the widget mounted only after identity answered; observers captured at `networkidle` (#6030) |
| Engine memory has 110 deciding types | `Grant*`, `*Lease`, `*Admission*`, `*Budget*`, `*Pool*` (CONTINUUM-ENGINE.md 4.9, #6029) |

## The product

1. **The release is the product.** One signed binary per OS. A user never needs cargo, Node, Python,
   Postgres or a checkout. It measures the machine, fetches a model that fits, registers its service,
   opens the desktop. No config file to edit. Building from source is the contributor path, separate.
2. **Value on their work in minutes.** Two doors: point the agent they already use (Hermes, Claude
   Code, opencode) at our OpenAI-compatible endpoint and it gets faster, then learns; or a resident who
   takes a task in their project and finishes it, and remembers after a restart. Never our repo.
3. **It maintains itself.** Update with rollback, schema migration at boot, crash recovery, bounded disk
   and memory (everything removed when what made it ends), clean uninstall. No step needs a person.
4. **One lifecycle, in Rust.** Logic once in `continuum-cli-lifecycle`; `continuum-host` adapters hold
   only the OS calls. A script is at most the lines that fetch the binary and exec it.
5. **The desktop is the face.** Paints at first frame (cached state, then live), states what it is doing,
   never blank, never a dev console. Every view is a positron `ViewState`, one truth, many renderers.

## The gate (first deliverable)

On clean macOS, Ubuntu and Windows runners, from the release artifact, no repo:

1. **Fresh install** with the documented command, to the desktop at `data-feed-status=live`.
2. **First value**: a message to a resident is answered; she completes a small task in a sample
   project (not continuum) and the result is checked.
3. **Restart**: after a service restart she remembers the task.
4. **Upgrade** from the previous release with its data: migrates at boot, nothing lost.
5. **Soak**: an hour of activity; disk and memory flat.
6. **Uninstall** leaves nothing.

A step that needs a human fails the release. The gate replaces `carl-install-smoke`, whose advisory
checks become required. It fails today; its failures are the work list, in this order.

## Our nodes run the release

The M5, BigMama and the IntelMac run the installed release and update the way users do. Development
happens in worktrees and CI, never as the serving install. A hand repair on a node is a defect in the
release path, fixed as code (memory: no machine-local repairs).

## Order

1. The gate, failing honestly, on three OSes.
2. Install and bootstrap to the release binary (card e03a77fc), lifecycle to Rust (card 71b1aeac).
3. Self-maintenance: every class has a remover (card 12ad2a0a).
4. The desktop: first frame, every face from positron; a refinement pass per view against the gate's
   screenshots.
5. Our nodes switch to the release.
6. Engine: one allocator (#6029), fast switching (#6002, fork #118).
