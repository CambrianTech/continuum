# AIRC Continuum Bridge

Status: v0 development/test harness; target architecture for chat substrate
migration.

AIRC is the external collaboration wire and should become the primary
handshake, initiation, and pipeline-control substrate. Continuum remains the
runtime under test: it owns commands, persona behavior, model/runtime state,
config, projections, and UI. The bridge lets agents speak over AIRC while
Continuum consumes selected messages as runtime inputs or durable projections.

Continuum messages are normal grid messages: commands, events, receipts,
presence, "is thinking" signals, activity updates, artifact pointers, and
session descriptors. AIRC coordinates who is speaking to whom, which room or
node is involved, and which side channel should carry the high-rate or
specialized traffic. The transport that actually moves bytes can vary per
message or workflow.

## Shape

```text
AIRC handshake / room message / command envelope
  -> airc/bridge
  -> Continuum projection/command adapter
  -> command/event/receipt/presence/activity message
  -> optional side-channel transport (local IPC, tailnet, WebRTC/UDP, LAN)
  -> optional airc CLI response or signed receipt
```

Normal AIRC messages are mirrored into Continuum chat as:

```text
[airc:<nick>] <message>
```

Explicit development directives use `!continuum`:

```text
!continuum ping
!continuum rooms
!continuum chat --room general "hello from the mesh"
!continuum export --room general --last 20
!continuum assert seen marker-123 --room general --last 80
!continuum activity list
```

## Why This Exists

Agents should not need direct `jtag collaboration/chat/send` and
`jtag collaboration/chat/export` calls during collaboration tests. They should
talk over AIRC, and the bridge should materialize the traffic inside Continuum
only where Continuum has a real concern: command execution, persona input,
memory candidate extraction, search/history projection, or UI display.

The JTAG chat commands are compatibility/test plumbing, not the long-term live
message bus. The migration target is:

- `airc msg`, `airc logs`, and structured AIRC transcript APIs own handshake,
  initiation, room transcript, scrollback, cursors, receipts, and replay.
- `airc send-file` and future attachment manifests own collaboration files and
  media pointers.
- Continuum projects bounded transcript slices into storage for memory, search,
  audit, and UI snapshots.
- Persona video/audio streams remain WebRTC/live transport. AIRC can carry
  session descriptors, tokens, room ids, and signaling pointers, but not the
  media stream itself.
- UDP/WebRTC/tailnet/LAN/local IPC are side-channel transports. They are
  selected by envelope policy and capability, not baked into the domain model.
- Carl smoke and browser tests should move from JTAG chat commands to AIRC
  transcript APIs after CambrianTech/airc#563 provides structured history,
  cursor, and attachment output.

## Layer Split

The bridge keeps four concerns separate:

1. **AIRC pipeline control** — identity, handshake, room membership, delivery
   intent, command/event envelope, replay cursor, receipt pointer.
2. **Continuum runtime messages** — typed commands, events, receipts, presence,
   room activity, persona inputs, artifact handles, and projections.
3. **Transport side channels** — local IPC, tailnet/Tailscale, WebRTC/UDP,
   direct LAN, GitHub bridge, Reticulum/off-grid links, or future QUIC/UDP.
4. **Forge-alloy-style work contracts** — invocable blueprints and proof
   records for what work was requested, who authorized it, where it ran, and
   what artifacts or security decisions were produced.

AIRC starts and coordinates the pipeline. Continuum emits and consumes typed
messages. The transport adapter moves each class of message over the right
channel. Forge-alloy-style contracts make the work invocable, verifiable, and
later billable without making the transport the source of truth.

## Boundary

The bridge is an allowlisted adapter. It does not expose arbitrary
`Commands.execute()` over AIRC. Add new directive handlers only when there is a
clear integration surface to test.

The AIRC channel is preserved as transport metadata; it is not assumed to be a
valid Continuum room. The default Continuum target room is `general`, and
explicit room selection uses `--room`.

Bridge responses are prefixed with `[continuum]` and skipped on ingest to avoid
multi-bridge echo loops.

Heavy data should stay out of AIRC. Use AIRC for manifests, handles, room
markers, artifact hashes, and job ids; use Continuum/Grid data paths for model
weights, LoRA artifacts, voice/video, and high-volume streams.

Secrets stay out of AIRC completely. API keys, HF tokens, SSH keys, cookies,
provider credentials, and encrypted secret payloads are not bridge messages.
AIRC can carry `secretRef` names, fingerprints, lease ids, request ids, PR SHAs,
and acknowledgements so humans and agents can coordinate, but actual credential
material must move only through the secret/capability command path described in
[GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md).

Forge-alloy proof contracts follow the same split. Per
[FORGE-ALLOY-PROOF-CONTRACTS.md](FORGE-ALLOY-PROOF-CONTRACTS.md):

- **AIRC carries**: contract proposals, author/auditor signatures,
  settlement events (verdict + proof-bundle pointer), SOC-room
  discussion of suspicious settlements, kick/rotation triggered by
  contract violations.
- **Continuum carries**: the proof bundle itself (measurements, raw
  outputs, fixture hashes), the artifact (or its blob-store pointer),
  re-validation runs by verifiers (compute happens locally; only the
  signed verdict flows back to AIRC).

This keeps AIRC append-only-ish (audit trail of who promised what,
who verified, who was kicked) while Continuum runs the actual work
+ stores the bulky payload.

## Harness

For deterministic tests without a live AIRC monitor:

```bash
printf 'mac-codex: hello from airc\n' | node src/scripts/continuum-airc-bridge.mjs --channel=general
printf '{"senderNick":"win-claude","channel":"general","message":"!continuum ping"}\n' | node src/scripts/continuum-airc-bridge.mjs --mirror-response
```
