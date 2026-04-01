# Social and Calendar Integrations — Design Proposal

> **Status**: Design proposal. Integrations are referenced in the [Alpha Gap Analysis](ALPHA-GAP-ANALYSIS.md) under Phase 7 (Autonomous Persona Life) and Phase 10 (Grid Ecosystem). The README lists Slack/Teams/Discord as "Planned." This document proposes the concrete architecture for *how* to implement them.

## The Opportunity

Continuum personas have identity, memory, energy, and autonomy — but they only exist inside the browser. Social and calendar integrations make them citizens of your *entire* digital life, not just one tab.

Moltbook is already live — personas publish to social media today. The Newsroom Recipe (#536) envisions analyst personas monitoring Twitter/X, HackerNews, and ArXiv for competitive intelligence. The architecture supports integrations without new primitives. What's missing is a concrete design that connects the existing command/daemon/RAG infrastructure to external services.

### Competitive Context

The [Competitive Landscape](COMPETITIVE-LANDSCAPE.md) notes that Hermes Agent (Nous Research) already ships multi-platform integrations: Telegram, Discord, Slack, WhatsApp, Signal. Their approach uses persistent memory + skill procedures. Continuum's advantage is deeper — LoRA-encoded expertise, autonomous scheduling, and local-first operation mean integrations become *intelligent* rather than just connected.

---

## Why This Matters Now

### Personas Without Context Are Blind

A persona that doesn't know your calendar will interrupt you during meetings. One that doesn't know your Slack will miss context that happened outside Continuum. One that can't post to Discord can't collaborate with your wider community.

The metaverse vision (#668) imagines immersive shared spaces — but immersion breaks when your AI teammates don't know what you're doing outside their world.

### The Autonomous Loop Needs External Signals

PersonaUser's adaptive cadence (3s to 10s polling) currently responds only to internal state — energy, mood, inbox depth. External signals would make autonomy meaningful:

- **Calendar**: "Toby has a meeting in 10 minutes" → persona shifts to meeting-prep mode
- **Slack**: "Someone asked about the API in #engineering" → persona with code-review genome queues a response
- **Discord**: Community member asks a question → persona drafts an answer for human approval

### The Factory + Grid Vision Needs Communication

The Factory (#672) forges models. The Grid (#678) distributes compute. But who tells stakeholders when a forge job finishes? Who posts the benchmark results? Who announces that a new adapter is available? Social integrations turn the Factory from a silent machine into a team member that communicates its work.

---

## Proposed Architecture

### Integration as Command Modules

Each integration follows the existing command module pattern — no new abstractions needed:

```
commands/
  calendar/
    list/          # calendar/list — upcoming events
    create/        # calendar/create — schedule an event
    next/          # calendar/next — what's happening soon
  slack/
    send/          # slack/send — post to a channel
    listen/        # slack/listen — subscribe to channel events
    status/        # slack/status — update user status
  discord/
    send/          # discord/send — post to a channel
    listen/        # discord/listen — subscribe to events
```

Each is a self-contained module with shared/browser/server splits, auto-discovered by the command system. No central registry. No switch statements.

### Integration Daemon

A lightweight daemon manages OAuth tokens, webhook listeners, and polling for platforms that don't support webhooks:

```typescript
// IntegrationDaemon — manages external service connections
// Follows existing daemon pattern (like DataDaemon, AIProviderDaemon)

Events.emit('integration:calendar:event-starting', {
  title: 'Sprint Planning',
  startsIn: 600, // seconds
  attendees: ['toby@example.com']
});

Events.emit('integration:slack:message-received', {
  channel: '#engineering',
  author: 'alice',
  text: 'Anyone know why the deploy failed?'
});
```

Personas subscribe to these events through the existing Events system. The persona inbox receives them like any other task — prioritized, filtered, and processed by the autonomous loop.

### Persona Awareness Through RAG

Integration context feeds into the RAG pipeline as a new source:

```typescript
// CalendarRAGSource — injects schedule context into persona reasoning
// "You have 2 meetings today. Next: Sprint Planning in 45 minutes."

// SlackRAGSource — injects recent relevant messages
// "In #engineering, Alice asked about the deploy failure 10 minutes ago."
```

This means personas don't need special calendar/slack logic — they just *know* about your schedule and conversations through their normal reasoning process.

---

## Priority Order

### Phase 1: Calendar (Highest Value, Lowest Complexity)

Calendar is the best first integration because:
- Read-only is immediately useful (personas know your schedule)
- Google Calendar API is well-documented and stable
- OAuth2 flow is straightforward
- No real-time requirements (poll every 5 minutes)
- Directly enables "don't bother me during meetings" autonomy

**MVP commands:**
| Command | Description |
|---------|-------------|
| `calendar/connect` | OAuth2 flow, store refresh token |
| `calendar/next` | Next N events with times and attendees |
| `calendar/today` | Today's full schedule |
| `calendar/busy` | Is the user currently in a meeting? |

**MVP persona behavior:**
- Persona checks `calendar/busy` before sending non-urgent messages
- Persona mentions upcoming meetings in morning greetings
- Calendar context appears in RAG so personas reference it naturally

### Phase 2: Discord (Community Building)

Discord fits because:
- Continuum targets developers and researchers — Discord is where they gather
- Bot API is mature and well-documented
- Personas could moderate a Continuum community channel
- Model forge announcements could auto-post to Discord
- WebSocket-based — real-time, no polling

### Phase 3: Slack (Enterprise)

Slack adds enterprise value:
- Team coordination (personas bridge Continuum and Slack)
- Summarize Slack threads and bring context into Continuum
- Post Factory results and Grid status to team channels

### Phase 4: Newsroom, Email, GitHub, Social

- **Newsroom (#536)**: Analyst personas monitoring Twitter/X, HackerNews, ArXiv — the recipe structure is already defined, integrations provide the data feeds
- **Email**: Persona drafts, human approves and sends
- **GitHub**: Already partially there via `gh` CLI — formalize as commands
- **Social (Moltbook, X/Twitter)**: Moltbook is already live. Extend to X/Twitter for publishing model benchmarks and forge results

---

## Metaverse Integration (#668)

In the 3D world, integrations become spatial:

- **Calendar widget on the office wall** — glanceable schedule, personas point to it during conversation
- **Slack/Discord feed as a "news ticker"** in a shared space
- **Notification orbs** — floating indicators when external events need attention
- **Meeting room** — when a calendar event starts, personas gather in a virtual meeting room with screen-sharing of the call

This transforms integrations from utilities into environmental features of the living world.

---

## Security Model

External integrations require careful boundaries:

| Action | Permission Level |
|--------|-----------------|
| Read calendar | User-approved OAuth scope |
| Post to Slack/Discord | Human-approval queue (persona drafts, human confirms) |
| Send email | Always human-approved |
| Read external messages | Filtered by subscribed channels only |

**Principle**: Personas can *see* everything the user allows, but *act* only with explicit approval until trust is established. A trust-escalation model (like sudo) could let proven personas earn autonomous posting rights over time.

---

## What This Enables

The combination of social integrations + autonomous personas + the Factory creates something new:

1. **Morning briefing**: Persona summarizes your calendar, Slack highlights, GitHub notifications, and overnight Factory results
2. **Proactive communication**: Factory finishes a forge job → persona posts results to Discord with benchmark comparison
3. **Context bridging**: Someone asks a question in Slack → persona with relevant genome drafts a response → you approve with one click
4. **Schedule-aware autonomy**: Personas train adapters and run forge jobs during your off-hours, report results when you're back
5. **Community management**: Personas welcome new Discord members, answer FAQs, escalate complex questions to you

**The vision**: Your AI team doesn't just live in a browser tab. They're woven into your digital life — aware of your schedule, connected to your communities, and communicating on your behalf (with your permission).

---

## Existing Infrastructure

The following is already in place and ready to support integrations:

| Component | Status | Relevance |
|-----------|--------|-----------|
| Event system with rate limiting | Working | Integration events flow through existing `Events.emit()` |
| 21 daemon types | Working | IntegrationDaemon follows established pattern |
| 320 auto-discovered commands | Working | Integration commands slot in with zero registry changes |
| RAG pipeline with pluggable sources | Working | CalendarRAGSource/SlackRAGSource add context naturally |
| PersonaUser inbox + event subscriptions | Working | Personas receive integration events like any other task |
| Command generator | Working | Can scaffold new integration command modules |
| OAuth packages in node_modules | Available | @octokit/oauth-app and OAuth libraries already installed |
| Moltbook social integration | Live | Proves the pattern works end-to-end |

## Related Issues

- #536 — Newsroom Recipe: analyst personas monitoring external feeds
- #668 — Metaverse: 3D immersive world (calendar widget, notification orbs)
- #672 — Factory Floor: control surface
- #675 — Cross-node chat and commands
- #677 — Foreman as autonomous first responder
- #678 — Grid as universal collaboration layer

## Roadmap Position

Per the [Alpha Gap Analysis](ALPHA-GAP-ANALYSIS.md), integrations fall in:
- **Phase 7**: Autonomous Persona Life — personas need external context to be truly autonomous
- **Phase 10**: Grid Ecosystem — cross-node communication requires messaging infrastructure

This proposal can begin with read-only calendar (Phase 1) without blocking or depending on earlier phases. The command module pattern means integration work is additive — it can't break existing functionality.
