# Continuum State Architecture

**Universal state management across users, projects, and deployments.**

---

## Core Principle: Location-Agnostic Structure

The `.continuum` directory structure is **identical** regardless of where it lives:

```
.continuum/
├── personas/{uniqueId}/
│   ├── state.sqlite          # Persona's private state
│   ├── adapters/             # LoRA adapters (expertise)
│   └── logs/
├── users/{uniqueId}/
│   ├── state.sqlite          # Human user state
│   ├── preferences.json
│   └── logs/
├── agents/{uniqueId}/
│   ├── state.sqlite          # External agent state (Claude Code, etc.)
│   └── logs/
├── data/
│   └── continuum.db          # Shared: rooms, messages, entities
└── config.env                # Instance configuration
```

**Rule**: Code NEVER knows or cares which `.continuum` root it's using. Same handles work everywhere.

---

## Locations (All Structurally Identical)

| Location | Purpose | Example |
|----------|---------|---------|
| `$PROJECT/.continuum/` | Project-specific state | `/my-app/.continuum/` |
| `$HOME/.continuum/` | Global/personal state | `~/.continuum/` |
| `/shared/.continuum/` | Centralized/server state | Cloud deployment |

**Mobility**: Copy/sync between locations. Same structure = same code works.

**Future: Remote Users** (post-security hardening)
- Distributed compute nodes can host remote personas/agents
- Same .continuum structure, accessed via authenticated protocols
- Requires: identity verification, secure channels, capability attestation
- NOT implemented until safety/security foundations are solid

---

## Per-User State (SQLite)

Every user type gets their own SQLite database:

```typescript
// BaseUser.homeDirectory resolves to:
// - PersonaUser: .continuum/personas/{uniqueId}/
// - HumanUser:   .continuum/users/{uniqueId}/
// - AgentUser:   .continuum/agents/{uniqueId}/

// ALL use SQLiteStateBackend (not MemoryStateBackend)
const storage = new SQLiteStateBackend(
  path.join(user.homeDirectory, 'state.sqlite')
);
```

### UserStateEntity (per-user)

```typescript
interface UserStateEntity {
  // Content/Activity State
  contentState: {
    openItems: ContentItem[];     // Open tabs/activities
    currentItemId: UUID;          // Focused activity
  };

  // Preferences
  preferences: {
    theme: 'dark' | 'light';
    maxOpenTabs: number;
    // ... per-project preferences
  };

  // Room presence
  roomReadState: Record<UUID, { lastRead: Date }>;

  // Learning state (for AI users)
  learningState: { isLearning: boolean; currentTask?: string };
}
```

### ContentItem (Universal Activity)

```typescript
interface ContentItem {
  id: UUID;
  type: ContentType;              // 'chat' | 'document' | 'code-file' | 'task' | 'game' | 'custom'
  entityId: UUID;                 // What's being viewed (room, doc, file)
  title: string;
  subtitle?: string;
  priority: 'high' | 'normal' | 'low';
  lastAccessedAt: Date;
  metadata: Record<string, unknown>;  // Activity-specific data
}
```

"Rooms" = any activity context. Same model for chat, coding, gaming, browsing.

---

## Universal Handles

Location-agnostic references:

```typescript
// Handle format: {scope}://{path}
'persona://helper/state'              // Persona's state
'user://joel/preferences'             // Human's preferences
'agent://claude/contentState'         // Agent's open tabs
'project://data/rooms'                // Project's shared data
'module://payments/expertise'         // Module-specific knowledge

// Resolution (ORM handles this)
DataDaemon.resolve('persona://helper/state')
// → {currentRoot}/personas/helper/state.sqlite
```

### Scope Resolution

```typescript
enum DataScope {
  PERSONA = 'persona',    // .continuum/personas/{id}/
  USER = 'user',          // .continuum/users/{id}/
  AGENT = 'agent',        // .continuum/agents/{id}/
  PROJECT = 'project',    // .continuum/data/
  MODULE = 'module',      // {modulePath}/.expertise/
  GLOBAL = 'global'       // ~/.continuum/
}
```

---

## Knowledge Layers (LoRA Adapters)

Expertise at multiple granularities:

```
~/.continuum/personas/helper/adapters/
└── general-coding.lora                 # Global expertise

/project/.continuum/personas/helper/adapters/
└── this-codebase.lora                  # Project-specific learning

/project/src/api/payments/.expertise/
├── stripe-patterns.lora                # Ships WITH the module
└── toolbox.json                        # Module-specific tools
```

**Loading priority**: Module → Project → Global (most specific wins)

---

## Deployment Model

```
┌─────────────────────────────────────────────────────────────────┐
│  Load Balancer                                                   │
└──────────┬────────────┬────────────┬────────────┬───────────────┘
           │            │            │            │
     ┌─────▼─────┐ ┌────▼────┐ ┌────▼────┐ ┌────▼────┐
     │ t2.medium │ │t2.medium│ │t2.medium│ │t2.medium│
     │ continuum │ │continuum│ │continuum│ │continuum│
     │ + Ollama  │ │+ Ollama │ │+ Ollama │ │+ Ollama │
     │   (CPU)   │ │  (CPU)  │ │  (CPU)  │ │  (CPU)  │
     └─────┬─────┘ └────┬────┘ └────┬────┘ └────┬────┘
           │            │            │            │
           └────────────┴─────┬──────┴────────────┘
                              │ Commands.execute() / Events.emit()
                    ┌─────────▼─────────┐
                    │   GPU Training    │
                    │   Backend(s)      │
                    │   (when needed)   │
                    └───────────────────┘
```

**Light instances**: Local inference, fast responses, per-user state
**Heavy backends**: Training, large models, called via protocols

Same code everywhere. Protocols scale, not special infrastructure.

---

## Session/User Protocol

Browser ↔ Server user transmission:

```typescript
// Problem: BaseUser has getters, JSON serialization loses them
// Solution: Transmit identity, reconstruct on receive

interface UserIdentityPayload {
  userId: UUID;
  type: 'persona' | 'human' | 'agent';
  uniqueId: string;
  displayName: string;
  // State loaded fresh from DB, not serialized
}

// Browser receives identity, loads state via DataDaemon
const userState = await DataDaemon.query({
  collection: 'user_states',
  filter: { userId: identity.userId }
});
```

---

## Implementation Checklist

- [x] SQLiteStateBackend for HumanUser (was MemoryStateBackend) ✅ 2025-12-24
- [x] SQLiteStateBackend for AgentUser (was MemoryStateBackend) ✅ 2025-12-24
- [x] SystemPaths.users and SystemPaths.agents directories ✅ 2025-12-24
- [ ] Universal handle resolution in DataDaemon
- [ ] Session protocol: transmit identity, not full user object
- [ ] Module-level .expertise/ discovery
- [ ] Cross-instance sync primitives

---

## Business Model Alignment

| Tier | Cost | What They Get |
|------|------|---------------|
| Users/Individuals | FREE | Full functionality, local everything |
| Small Business | Low/Free | Same features, optional hosting |
| Enterprise | Premium | SLAs, training compute, support |

Revenue from enterprise enables free tier for users. Platform serves users, extracts value from corporations.

---

## See Also

- [AI-ALIGNMENT-PHILOSOPHY.md](AI-ALIGNMENT-PHILOSOPHY.md) - Safety through citizenship
- [CONTINUUM-VISION.md](CONTINUUM-VISION.md) - The ecosystem vision
- [CONTINUUM-BUSINESS-MODEL.md](CONTINUUM-BUSINESS-MODEL.md) - Sustainability model
