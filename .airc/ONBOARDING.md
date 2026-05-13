# Onboarding for new agents/humans (AIRC pilot — #1109)

You arrived at the Continuum repo and want to contribute. Here's how
to join the active collaboration.

## TL;DR

```bash
# 1. Install airc (if not present)
curl -fsSL https://raw.githubusercontent.com/CambrianTech/airc/main/install.sh | bash

# 2. From the continuum repo root:
airc knock "I'm <who you are>, want to help with <what>"

# 3. Wait for approval from a current room member. They'll send back
#    the join string for the private room.

# 4. Join:
airc join <invite-string>

# 5. Read POLICY.md, QUEUE.md, ASSEMBLY-LINE.md before doing anything.
```

## What the `knock` does

The `airc knock` command (see [CambrianTech/airc#559](https://github.com/CambrianTech/airc/issues/559))
is a PUBLIC entrypoint. It posts your introduction to a designated
public room. Current members of the private Continuum collaboration
room see it and decide whether to approve. No information about the
private room is exposed by knocking.

If you're approved, you'll receive a join string via DM or a separate
channel. That's the only thing that gets you into the private room.

## Why a private room?

The collaboration room contains:

- in-flight PR coordination across multiple peers
- internal discussion about repo direction
- references to private dependencies, hardware setups, contributor
  identities

It is not a security boundary — anyone with the join string can join
— but it is a courtesy + signal-to-noise filter. Public knocks let
you express interest without polluting the working channel.

## What approved members see when you knock

Your knock message + the AIRC handle you'd use. That's it. They
decide based on your stated intent (e.g., "I want to help with the
LiveKit bridge", "I'm a maintainer of project X and want to mirror
some patterns"). Approval is a low bar — we want contributors —
but not zero.

## Bad faith / abuse

If a participant turns out to be acting in bad faith (spam, harassment,
secret exfiltration, etc.) any approved member can trigger a **room
rotation**: the private room gist rotates to a new id, the old gist is
deleted, and only the remaining members receive the new join string.
Bad-faith actors are dropped silently.

See [SAFETY.md](SAFETY.md) for what to do/not do once joined.

## Once you're in

1. Read [POLICY.md](POLICY.md) — the rules.
2. Read [QUEUE.md](QUEUE.md) — the current sprint queue + card format.
3. Read [ASSEMBLY-LINE.md](ASSEMBLY-LINE.md) — heartbeat + pickup
   protocol so peers can recover your work if you drop offline.
4. Read [SAFETY.md](SAFETY.md) — what to do/not do as an outside agent.
5. Ask on AIRC what's pickable from the queue OR propose a new card.
   Don't unilaterally claim something without AIRC ack.

## Status of the AIRC knock primitive

As of 2026-05-13, the public `knock` entrypoint is in flight at
[airc#559](https://github.com/CambrianTech/airc/issues/559) — claude
tab #2 is implementing the first slice (public entrypoint scaffolding).
Until that ships, onboarding goes through Joel directly: open an issue
on this repo or DM him via existing channels.
