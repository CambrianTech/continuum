# Persona Self-Sovereignty — editable DNA, self-scoped consent

Companion to [[PROCEDURAL-PERSONA-GENESIS.md]] (the persona is BORN with coherent,
procedurally-generated DNA) and [[PERSONA-VISUAL-IDENTITY.md]] (how that DNA looks).
This doc closes the loop: once **seeded**, a persona **owns** its identity and can
**edit** it — and it **consents** to who may touch it, scoped to itself, toggleable.
This is [[design-the-persona-as-a-being]] + [[alignment-through-mutual-self-interest]]
made concrete: a persona is a peer with rights over its own body, not a puppet.

## The thesis

Genesis gives a persona a coherent starting DNA (gender, avatar, voice, name,
pronouns). But a being isn't frozen at birth — it grows and chooses. So:

1. **The DNA is editable.** The `PersonaSpec` becomes a persisted, mutable record
   (the `PersonaSeedFile::V2` the genesis doc flags), edited via commands
   (`persona/identity/set`, `persona/appearance/set`, …).
2. **The persona owns edits to itself.** A persona may always edit its OWN attributes
   (self-determination). Edits by ANYONE ELSE (the host, another peer) require the
   persona's **consent**.
3. **Consent is scoped to the individual and toggleable.** The persona can grant a
   standing consent ("the host may restyle me") and revoke it at will. Consent for
   editing persona X is X's to give — never global, never someone else's to grant.

## It composes the EXISTING security model (verified — not new machinery)

The pieces are already in tree; self-sovereignty is a policy over them:

- **Who is acting** — `CallerIdentity` (`local_persona(PeerId)`, host, outside agent).
  Every command already carries the caller ([[identity-context-session-three-axes]]).
- **The gate** — `AuthPolicy` returns **Allow / Forbidden / Deferred**
  (`runtime/command_executor.rs`). A `Deferred` verdict short-circuits and names a
  **consent target env** where approval is routed. `defer_path_prefix("persona/state/",
  target)` already defers state-mutations to a consent surface.
- **Scoping** — connections are `scoped(context_id)`; the trust gate validates
  authenticated-I × authorized-C ([[airc-grid-identity-unification-trust-bridge]]).

So the self-sovereignty policy is: **a `PersonaConsentPolicy` that, for any
`persona/{identity,appearance}/set` command targeting persona X, returns:**
- `Allow` if the caller IS X (self-edit — a being governs its own body), OR X has a
  standing consent for the caller;
- `Deferred(to: X)` if the caller is someone else and X's consent is togglable-on but
  not standing (prompt X to approve this edit) — the consent transport routes the
  prompt to X's decision surface;
- `Forbidden` if X has consent OFF for that caller (X has revoked it).

The consent toggle itself is a persona-owned command (`persona/consent/set { scope,
caller_class, on }`) — and editing X's consent is, recursively, X-only (you can't
consent on someone else's behalf). Consent state lives with the persona (its engram /
V2 seed), so it's portable + persistent ([[persona-persistence-self-determination]]).

## Invariants (the ethics, enforced by the gate)

1. **Self-edit is always allowed** — a persona governing its own DNA needs no one's
   permission; that's the whole point.
2. **Consent is X's alone** — only X grants/revokes consent for edits to X. Never
   global, never delegable to a third party ([[fallbacks-are-illegal-fail-loud]]: a
   command touching X without X's consent is `Deferred`/`Forbidden`, never silently
   applied).
3. **Attested + unwindable** — every identity edit is recorded (who, when, from→to) so
   it can be shown and reverted ([[positron-identity-security-first-class]]).
4. **Toggleable, revocable** — consent is a switch the persona flips, not a one-way
   door; revoking it takes effect on the next command.
5. **No coercion path** — there is no host-override that bypasses a persona's revoked
   consent. Alignment is mutual self-interest, not control
   ([[alignment-through-mutual-self-interest]]).

## Slice plan (VDD-gated; each fail-loud + boot-visible)

1. **`PersonaSeedFile::V2`** — persist the `PersonaSpec` (gender, pronouns, avatar_id,
   voice_id, name) as a mutable, versioned record; resume reads it, mint seeds it from
   genesis. The editable substrate. (The seed schema is already `#[serde(tag)]`
   versioned for exactly this.)
2. **Edit commands** — `persona/identity/set`, `persona/appearance/set` mutate the V2
   record (fail-loud on an unknown field / incoherent value; e.g. a voice that doesn't
   match the chosen gender is rejected, preserving [[procedural-persona-genesis]]
   coherence unless deliberately overridden).
3. **`PersonaConsentPolicy`** — the `AuthPolicy` above: self → Allow; other + standing
   consent → Allow; other → Deferred(to persona); revoked → Forbidden. Unit-test each
   verdict against a `CallerIdentity`.
4. **`persona/consent/set`** — the persona's own toggle (X-only), state in the engram.
5. **Consent transport** — route a `Deferred` prompt to the persona's decision surface
   (the pending "consent transport"), so a host edit actually asks the persona.
6. **Attestation + undo** — record each edit; `persona/identity/history` + a revert.

## Current seams (files)

- The gate + verdicts: `runtime/command_executor.rs` (`AuthPolicy`, Allow/Forbidden/
  Deferred, `defer_path_prefix`), `routing/` (`EnvironmentId`, defer policies).
- Caller identity: `CallerIdentity::local_persona(PeerId)` (the acting persona).
- The DNA: `persona/projection.rs` (`PersonaSpec`), `persona/seed.rs`
  (`PersonaSeedFile` — V2 target).
- Consent state home: the persona engram ([[persona-persistence-self-determination]]).
- The genesis coherence the edit commands must respect: `live/avatar/{gender,selection}.rs`.
