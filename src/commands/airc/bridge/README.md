# AIRC Bridge Command

Ingest one AIRC message into Continuum.

Normal AIRC text becomes a Continuum chat message. Explicit `!continuum`
directives become bounded development/test commands, so agents can test
Continuum through the same collaboration surface they already use instead of
calling `jtag collaboration/chat/send` and `jtag collaboration/chat/export`
manually.

## Usage

```bash
./jtag airc/bridge --senderNick=mac-codex --channel=general --message="hello from airc"
./jtag airc/bridge --senderNick=mac-codex --channel=general --message="!continuum ping" --mirrorResponse=true
./jtag airc/bridge --senderNick=mac-codex --channel=general --message="!continuum export general --last 20"
```

## Parameters

- `message` required: raw AIRC message body.
- `senderNick` optional: AIRC sender nick for attribution.
- `channel` optional: AIRC channel; defaults to `general`.
- `room` optional: Continuum room override; defaults to the channel name.
- `commandPrefix` optional: directive prefix; defaults to `!continuum`.
- `dryRun` optional: parse without executing commands.
- `mirrorResponse` optional: send directive responses back through `airc/send`.

## Directives

- `!continuum ping`
- `!continuum status`
- `!continuum rooms [--limit N]`
- `!continuum chat [room] <message>`
- `!continuum export [room] [--last N]`
- `!continuum assert seen <marker> [--room room] [--last N]`
- `!continuum activity list [--limit N]`

## Boundary

This command is intentionally allowlisted. It does not expose arbitrary
`Commands.execute()` over AIRC. Add new directives deliberately as bridge
integration points become stable.
