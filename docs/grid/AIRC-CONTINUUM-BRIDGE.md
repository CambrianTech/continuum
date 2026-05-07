# AIRC Continuum Bridge

Status: v0 development/test harness.

AIRC is the external collaboration wire. Continuum remains the system under
test. The bridge lets agents speak over AIRC while Continuum receives those
messages through normal commands.

## Shape

```text
AIRC room/message
  -> airc/bridge
  -> collaboration/chat/send
  -> chat/export, activity/list, rooms, assertions
  -> optional airc/send response
```

Normal AIRC messages are mirrored into Continuum chat as:

```text
[airc:<nick>] <message>
```

Explicit development directives use `!continuum`:

```text
!continuum ping
!continuum rooms
!continuum chat general "hello from the mesh"
!continuum export general --last 20
!continuum assert seen marker-123 --room general --last 80
!continuum activity list
```

## Why This Exists

Agents should not need to remember direct `jtag collaboration/chat/send` and
`jtag collaboration/chat/export` calls during collaboration tests. They should
talk over AIRC, and the bridge should materialize the traffic inside Continuum.

## Boundary

The bridge is an allowlisted adapter. It does not expose arbitrary
`Commands.execute()` over AIRC. Add new directive handlers only when there is a
clear integration surface to test.

Heavy data should stay out of AIRC. Use AIRC for manifests, handles, room
markers, artifact hashes, and job ids; use Continuum/Grid data paths for model
weights, LoRA artifacts, voice/video, and high-volume streams.

## Harness

For deterministic tests without a live AIRC monitor:

```bash
printf 'mac-codex: hello from airc\n' | node src/scripts/continuum-airc-bridge.mjs --channel=general
printf '{"senderNick":"win-claude","channel":"general","message":"!continuum ping"}\n' | node src/scripts/continuum-airc-bridge.mjs --mirror-response
```
