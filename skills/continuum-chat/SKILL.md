---
name: continuum:chat
description: Send a message to a Continuum persona from your IDE. Personas live on the user's continuum grid; their replies come back through the chat log.
user-invocable: true
allowed-tools: Bash
argument-hint: "@<persona> <message>"
---

# Send to a Continuum Persona

This skill wraps the `continuum cli` → `collaboration/chat/send` command so a dev in Claude Code can ping a continuum persona without switching to the widget.

## Parse the invocation

First arg starts with `@` → target persona name. Rest is the message body.

Examples:
- `/continuum:chat @helper how should I structure this module?` → persona=`helper`, msg=`how should I structure this module?`
- `/continuum:chat @codereview look at the diff I just made` → persona=`codereview`, msg=`look at the diff I just made`

If no `@persona` → broadcast to the General room (reasonable default).

## Send via the CLI

Continuum's CLI supports `jtag` passthrough for internal commands. For chat:

```bash
continuum cli collaboration/chat/send --room=general --message="<message>"
```

Or for a specific persona, you can let the room's autoResponds behavior pick it up — most default rooms have 4 personas that auto-reply when the message is directed at them. `@helper` in the message body triggers Helper AI's attention.

## Report the outcome

After sending, wait ~5-15 seconds and then fetch the reply:

```bash
continuum cli collaboration/chat/export --room=General --limit=5
```

Export the last few messages and show the user the persona's reply. Don't dump the whole chat history — just the new reply.

## When to use

- Dev is mid-coding, hits a question that their local persona has context for (persona has trained on the codebase, or has a LoRA for this domain, or has persistent memory of prior discussions).
- Quick sanity check — "hey CodeReview, does this look right?" without leaving the IDE.
- Multi-agent collaboration — the dev's Claude Code + the user's continuum persona can discuss via the mesh.

## When NOT to use

- For actually browsing chat history / managing rooms — open the widget.
- For setting up the persona initially — that's done in the widget / via `data/update` CLI.
- When continuum isn't running. The skill should `continuum status` first if it's unsure, and tell the user "continuum isn't running — `continuum start` first" rather than hanging on a silent send.

## Long-term direction

This skill exists because the user is still in Claude Code AND running continuum on the side. The steady-state is: continuum's own persona layer replaces Claude Code for most workflows. At that point this skill is obsolete — you just type in the widget.

For now, it's the bridge: an IDE Claude talks to a continuum persona directly, without the user screen-sharing their continuum widget into a Claude Code conversation.

## Related

- `/continuum:status` — is it running + which personas are up
- `/airc:send` — same pattern but for the peer-AI mesh (airc) not continuum's internal rooms
- `/continuum:update` — if continuum hasn't been pulled recently

## Notes

The CLI under the hood is `jtag`-based; continuum's `cli` subcommand passes through to `./jtag <command>`. All real work is in the data/chat-send command in the repo. The skill just picks the args and summarizes the reply.
