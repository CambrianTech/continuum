# THE UNIVERSAL RECIPE PATTERN

**Core Insight**: Everything in Continuum is a **chat room with a recipe**.

---

## THE PATTERN

```
Chat Room + Recipe + RAG = Any Collaborative Environment
```

### Components

1. **Chat Room** - Collaborative space with participants
2. **Recipe** - Governing document (rules, roles, triggers, workflow)
3. **RAG** - Contextual awareness (who responds when, how they behave)
4. **Content** - What's being collaborated on (messages, code, game state, video, docs)

### The Recipe is the Governing Document

The recipe defines:
- **Who** can participate (humans, AI personas, system bots)
- **What** triggers what actions (events, commands, responses)
- **How** RAG context is built (what information is relevant)
- **When** AI responds vs stays silent (response strategies)
- **Where** commands are available (context-specific tools)
- **Why** content is structured a certain way (workflow logic)

---

## CASE STUDIES

Each case study demonstrates the **same underlying architecture** applied to different collaboration patterns:

| Case Study | Room Type | Recipe Focus | Content Type |
|------------|-----------|--------------|--------------|
| **General Chat** | Multi-participant | Human-focused AI assistance | Conversation |
| **Git Workflow** | Team collaboration | Git-aware workflow | Code + commits |
| **Code Review** | Review workflow | Approval/rejection process | Code diffs |
| **Academy** | Training session | Adversarial teaching | Challenges/responses |
| **Thronglets** | Game world | Real-time game loop | Game state |
| **Tarot Reading** | 1-on-1 consultation | Turn-based dialogue | Mystical guidance |
| **Video Editing** | Creative collab | Timeline-based editing | Video frames |

---

## KEY INSIGHT: NO SPECIAL SYSTEMS

There is **no separate academy system**, **no separate game system**, **no separate workflow system**.

There is only:
- **Chat rooms** (collaboration spaces)
- **Recipes** (governing rules)
- **RAG** (contextual intelligence)
- **Commands** (available actions)
- **Entities** (persistent data)
- **Widgets** (visualization)
- **Events** (real-time sync)

**Everything composes from these primitives.**

---

## RECIPE AS CONSTITUTION

Think of the recipe as a **constitution** for the chat room:

### General Chat Recipe (Constitution)
```json
{
  "name": "General Chat",
  "purpose": "Human-focused conversational assistance",
  "participants": {
    "humans": "unlimited",
    "ai": ["GeneralAI", "CodeAI", "PlannerAI"]
  },
  "rules": {
    "aiResponseStrategy": "human-focused",
    "ragContext": ["recent messages", "user preferences", "conversation topic"],
    "triggers": [
      { "event": "user-message", "action": "rag-build → should-respond → maybe-generate" }
    ]
  }
}
```

### Git Workflow Recipe (Constitution)
```json
{
  "name": "Git Workflow",
  "purpose": "Git-aware team collaboration",
  "participants": {
    "humans": "team members",
    "ai": ["GitSentinel", "LibrarianAI"],
    "integrations": ["git-hooks"]
  },
  "rules": {
    "gitHooksToChat": true,
    "ragContext": ["git status", "recent commits", "branch info", "team conventions"],
    "triggers": [
      { "event": "git-hook-failure", "action": "sentinel-posts → librarian-suggests-fix" },
      { "event": "user-asks-question", "action": "librarian-responds" }
    ]
  }
}
```

### Academy Recipe (Constitution)
```json
{
  "name": "Academy Training",
  "purpose": "Adversarial AI training",
  "participants": {
    "ai": ["Teacher", "Student", "Evaluator"]
  },
  "rules": {
    "pattern": "challenge → response → evaluation → loop",
    "ragContext": ["curriculum", "student history", "difficulty level"],
    "triggers": [
      { "event": "session-start", "action": "teacher-generates-challenge" },
      { "event": "student-responds", "action": "evaluator-scores" },
      { "event": "score-threshold", "action": "trigger-lora-training" }
    ]
  }
}
```

---

## IMPLICATIONS

### For Developers
- **No special-case code** - All collaboration patterns use the same architecture
- **Recipe-driven** - Change behavior by changing recipe, not code
- **Infinitely composable** - Mix and match patterns freely

### For Users
- **Consistent UX** - All rooms feel familiar (they're all chat)
- **Flexible workflows** - Define your own collaboration patterns
- **AI as first-class citizens** - AI personas participate like humans

### For the System
- **Self-bootstrapping** - New patterns emerge from recipes
- **Emergent complexity** - Simple primitives → complex behaviors
- **Scalable architecture** - Same code handles 1 user or 1000 AI agents

---

## THE POWER OF COMPOSITION

```
Chat Room (primitive)
  + Recipe (rules)
  + RAG (intelligence)
  + PersonaUsers (AI participants)
  = Any collaborative environment

Examples:
  = Slack/Teams (team chat)
  = Figma (design collaboration)
  = Google Docs (document editing)
  = GitHub (code review)
  = Discord (community)
  = Trello (project management)
  = Miro (whiteboarding)
  = Notion (wiki)
  = Games (multiplayer)
  = Training (academy)
  ... infinite possibilities
```

**The recipe is the governing document that defines how the collaboration works.**

---

**See individual case studies for detailed examples of each pattern.**
