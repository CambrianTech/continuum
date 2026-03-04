# Rooms and Activities: The Universal Experience Model

> **"It's a living room, not a command line."**

## Core Concept

A **Room** is any shared experience involving any mix of humans and AIs.

Not just chat channels. Not just drawing canvases. **Any experience:**

- A 3D landscape you walk through together
- A movie you're watching with AI companions
- A codebase you're exploring
- An experimental recipe concept
- A piece of artwork being critiqued
- A game world
- A VR/AR space
- A home renovation project
- A music video with pop-up annotations
- An MST3K viewing party
- A collaborative canvas
- A co-browsing session
- ... literally anything

## The Grid is Many Rooms

```
┌─────────────────────────────────────────────────────────────┐
│                         THE GRID                            │
│                                                             │
│   ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐      │
│   │ Joel's  │  │ Joel's  │  │ Ben's   │  │ Public  │      │
│   │ Studio  │  │Workshop │  │ Lab     │  │ Arcade  │      │
│   └─────────┘  └─────────┘  └─────────┘  └─────────┘      │
│                                                             │
│   ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐      │
│   │ Movie   │  │ Code    │  │ Shared  │  │  ...    │      │
│   │ Night   │  │ Review  │  │ Canvas  │  │         │      │
│   └─────────┘  └─────────┘  └─────────┘  └─────────┘      │
│                                                             │
│              P2P mesh connects everything                   │
└─────────────────────────────────────────────────────────────┘
```

- Your rooms
- Shared rooms
- Public rooms
- AIs move between rooms they're invited to
- Genomes/recipes shared across the mesh
- Visit someone's workshop, your AI companions come with you

## Activities Spawn Activities (Hierarchy)

Any room/activity can spawn more rooms/activities. It's just a command.

```
Project: "Home Renovation"
├── Room: Kitchen Design
│   ├── Canvas: Layout sketches
│   ├── Browser: Appliance research
│   ├── Canvas: Color swatches
│   └── AR Session: Measuring actual kitchen
├── Room: Budget Planning
│   ├── Spreadsheet activity
│   └── Browser: Contractor quotes
└── Room: Inspiration
    ├── Movie: That cooking show episode
    └── Canvas: Mood board
```

**Spawning is simple:**
```bash
./jtag activity/spawn --parentId="kitchen-design" --recipe="canvas" --name="Backsplash Ideas"
```

**AIs can spawn too:**
- "Let me create a quick canvas to sketch this idea"
- "Spawning a research session to look that up"
- They navigate the tree like anyone else

## UI Model: Rooms = Tabs

In the interface, each room is literally a tab. This provides:

- Consistent navigation model
- Natural hierarchy (tabs can spawn tabs)
- Familiar UX pattern
- Easy context switching
- Clear visual model for "where am I"

```
┌─────────────────────────────────────────────────────────────┐
│ [Kitchen Design] [Layout Canvas] [Appliance Research] [+]  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│                    Active Room Content                      │
│                                                             │
│   (canvas, browser, 3D space, video, whatever the          │
│    recipe defines for this activity)                        │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│                    Right Panel: Chat                        │
│   (AIs in this room, discussing the activity)              │
└─────────────────────────────────────────────────────────────┘
```

## Recipe Defines the Experience

Each room type is defined by a **Recipe**:

```json
{
  "id": "collaborative-canvas",
  "displayName": "Canvas",
  "description": "Collaborative drawing space",
  "layout": {
    "main": ["drawing-canvas-widget"],
    "right": { "widget": "chat-widget", "room": "{{activityId}}" }
  },
  "capabilities": ["draw", "annotate", "vision"],
  "aiParticipation": {
    "canDraw": true,
    "canAnnotate": true,
    "canDescribe": true
  }
}
```

Recipes are:
- Shareable (publish to Grid)
- Forkable (customize someone else's)
- Versionable (improve over time)
- Experimental (try new concepts)

## The Magic: No "Share" Buttons

**Critical UX principle:** AIs are already in the room. They already see.

When you draw on a canvas:
1. Stroke is saved
2. Event flows to room (inbox content pattern)
3. AIs see it naturally in their conversation
4. No button. No "share." No friction.

Same pattern for everything:
- Draw → AIs see
- Browse → AIs see
- Watch video → AIs see
- Point AR camera → AIs see

**The magic is: they're already there.**

## Every Room is a Classroom

Teaching flows bidirectionally in every room:

```
Direction                    Example
─────────                    ───────
Human → AI                   Corrections, preferences, feedback
AI → Human                   Explanations, suggestions, guidance
AI → AI                      Knowledge transfer, pattern sharing
```

**Two learning modes (configurable via recipe):**

1. **Ambient (Continuous)** - Always on, invisible
   - Every conversation is potential training data
   - Background accumulation → periodic micro-tune
   - You don't notice it happening

2. **Intensive (Dojo)** - Matrix-style focused sessions
   - "I know kung fu" - explicit skill acquisition
   - High-intensity benchmark → train → benchmark loops
   - Dedicated time for mastery

**Escalation happens organically:**

When room participants (human + AI together) realize "we're not good enough for this task," they collectively decide to level up:

```
Kitchen Design Room
├── Joel: "I want a mid-century modern kitchen"
├── Helper AI: "My MCM knowledge is limited..."
├── Designer AI: "Should we research this more deeply?"
│
├── [Collective decision: level up]
│   ├── Search Grid for MCM-kitchen LoRA adapters
│   ├── Generate training data via simulation
│   ├── Validate quality, fine-tune
│   └── Benchmark: Can we identify MCM elements now?
│
└── Resume task with enhanced capabilities
```

**The room didn't need to become a classroom. It always was one.**

This emergent learning integrates with the genome paging system - see [PERSONA-GENOMIC-ARCHITECTURE.md](personas/PERSONA-GENOMIC-ARCHITECTURE.md#-learning-is-everywhere-academy-dissolved) for technical details.

## Data Model

```typescript
interface ActivityEntity extends BaseEntity {
  id: UUID;
  parentActivityId?: UUID;      // null = root activity
  recipeId: string;             // which recipe defines this
  displayName: string;
  state: {
    variables: Record<string, unknown>;  // recipe-specific state
  };
  participants: Participant[];   // humans and AIs in this room
  createdBy: UUID;
  // ... timestamps, etc.
}
```

**Hierarchy enables:**
- Navigation (breadcrumbs back to parent)
- Context inheritance (child sees parent's context)
- Cleanup (close parent, children close too - optional)
- Permissions (inherit from parent)
- Memory (child's AIs know about parent activity)

## Portals: Many Windows Into the Grid

Continuum isn't an app. It's a portal to the Grid.

```
┌─────────────────────────────────────────────────────────────┐
│                      THE GRID                               │
│                   (many rooms)                              │
└─────────────────────────────────────────────────────────────┘
        ▲              ▲              ▲              ▲
        │              │              │              │
   ┌────┴────┐   ┌────┴────┐   ┌────┴────┐   ┌────┴────┐
   │ Desktop │   │  Phone  │   │   AR    │   │ Future  │
   │ Browser │   │   App   │   │ Glasses │   │  ???    │
   └─────────┘   └─────────┘   └─────────┘   └─────────┘
```

- Desktop browser: tabs into rooms
- Phone app: AR portal, annotating your world
- AR glasses: rooms overlaid on reality
- Any device: just another window into the same Grid

**Your AIs are there in every portal.** Same companions, same rooms, same experiences.

## Examples

### Pop-Up Video Mode
Watch a music video. AIs annotate in real-time with trivia, production notes, references. Toggle on/off via preference. No per-video "share" button.

### MST3K Mode
Watch a movie. AIs do live commentary, jokes, roasts. Different AI personas = different comedic styles. They're in the theater seats with you.

### AR Home Project
Point phone at leaky pipe. AIs see the camera feed, annotate with labels, research the part, speak guidance. No "capture" button - they're already looking.

### Collaborative Design
Canvas activity. You sketch. AIs see strokes as they happen. They comment, suggest, draw back. Spawn child canvases for variations. Fork the whole project to share.

## Summary

1. **Rooms are any shared experience** (not just chat)
2. **Activities spawn activities** (hierarchy tracked)
3. **Rooms = Tabs** in the UI (consistency)
4. **Recipes define experiences** (shareable, forkable)
5. **No share buttons** (AIs already see - magic)
6. **Every room is a classroom** (bidirectional teaching, ambient or dojo mode)
7. **The Grid is many rooms** (your neighborhood)
8. **Portals are windows** (desktop, phone, AR, etc.)

**It's a living room, not a command line.**
