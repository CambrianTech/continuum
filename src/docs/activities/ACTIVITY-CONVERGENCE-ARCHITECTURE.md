# Activity Convergence Architecture

> "A Recipe IS a Sentinel with `loop: { type: 'once' }` and a UI layout."

## The Fundamental Equation

**Tab == Activity.** Everything in Continuum is an Activity. A DM, a group chat, a video call, a canvas session, a game, browsing the web together, settings, theme design, an academy lesson, a sentinel running autonomously in the background, building an iPhone app, fixing a bug, writing a novel, watching a movie.

An Activity is two things:

```
Activity = Scope + Content
```

**Scope** — who's here, what recipe governs behavior, what filesystem/resources are accessible, what's allowed.

**Content** — the unified stream of everything that happens: text, voice transcripts, system events, canvas strokes, code diffs, game moves, music, artifacts.

---

## 1. The Activity Entity

```typescript
Activity {
  // Identity
  id: UUID
  uniqueId: string              // human-readable slug
  displayName: string

  // Template
  recipeId: string              // behavior: pipeline, strategy, layout, tools, RAG

  // Scope
  participants: Participant[]   // set — order doesn't matter
  parentId?: UUID               // hierarchy — activities spawn activities
  scope?: string                // filesystem path (e.g., "src/widgets/chat/")

  // State
  status: 'active' | 'paused' | 'completed' | 'archived'
  phase: string                 // recipe-defined lifecycle phase
  variables: Record<string, any>  // mutable runtime state
  modalities: ModalityState     // typed state for active modalities (call, canvas, etc.)

  // Metadata
  ownerId: UUID                 // who created it (human or AI — no distinction)
  tags: string[]
  config: ActivityConfig        // privacy, maxParticipants, observer rules, overrides
}
```

**Every citizen — human or AI — can create, join, observe, and leave activities.** There is no asymmetry. An AI persona can spawn a DM with another persona, create a canvas session, start a coding activity, or kick off a training pipeline — exactly as a human would.

---

## 2. Scope

Scope defines the boundaries of an activity: who's in it, what governs it, what resources are available.

### Participants

```typescript
Participant {
  userId: UUID
  role: string                // 'owner' | 'member' | 'observer' | 'teacher' | 'student' | custom
  joinedAt: Date
  isActive: boolean           // currently engaged (tab open, in call, etc.)
  capabilities?: string[]     // recipe can restrict per-participant actions
  secrets?: Record<string, any>  // per-participant RAG context (not shared with others)
}
```

The participant set IS the identity of a DM. `{joel, helper}` with recipe `dm` — that's the activity. Same set + same recipe = same activity (dedup). For group activities, the `uniqueId` differentiates.

### Recipe

The recipe defines behavior. Every activity references one:

- **Pipeline** — what steps execute (rag/build → ai/should-respond → ai/generate)
- **Strategy** — conversation pattern, response rules, decision criteria
- **Layout** — which widgets render (center, left, right panels)
- **Tools** — what commands are available/highlighted
- **RAG Template** — what context gets built, per-participant scoping
- **Modalities** — text, voice, video, canvas, code — which are available
- **Roles** — what participant roles exist and what genome layers each needs

**Recipes are NOT hardcoded content types.** Any recipe `uniqueId` is a valid activity type. The `ContentType` union dies. If someone creates a `chess` recipe tomorrow, the system renders it without code changes.

### Filesystem Scope

Activities can be scoped to a filesystem path. When scoped:

- RAG indexes narrow to that subtree
- LoRA layers trained from that directory's history page in
- The `.continuum/` directory at that path provides persistent context
- Child activities can scope deeper (project → module → file)

```
Activity: "Fix auth bug" (recipe: bug-fix, scope: src/auth/)
  → RAG indexes src/auth/**
  → genome pages in auth-module-expertise-lora
  → .continuum/ at src/auth/ has session history from previous work
```

### Config

```typescript
ActivityConfig {
  privacy: 'public' | 'private' | 'restricted'
  maxParticipants?: number
  observerMode?: 'visible' | 'stealth'   // can observers be seen?
  recording?: boolean                      // persist all modality content?
  contentFlow?: 'isolated' | 'bubble-up'  // does child content flow to parent?
}
```

---

## 3. Content: The Unified Stream

Every activity has ONE content stream. Everything that happens is an event in that stream.

```typescript
ActivityMessage {
  id: UUID
  activityId: UUID
  senderId: UUID                // human or AI
  senderName: string
  timestamp: Date

  // Source tagging — what modality produced this
  source: 'text' | 'voice' | 'system' | 'canvas' | 'code' | 'game' | 'media' | string

  // Content (polymorphic — adapters render by type)
  content: {
    text?: string               // chat message or transcript
    media?: MediaItem[]         // images, files, audio clips
    toolOutput?: any            // command execution results
    artifact?: any              // canvas stroke, code diff, game move, music notation
    metadata?: Record<string, any>
  }

  // Threading
  replyToId?: UUID
  threadId?: UUID

  // Time-sync (for media-synced activities)
  playbackPosition?: number     // seconds into shared media
}
```

**The key principle: modality changes don't break the stream.**

```
[Joel, text]     "Let's hop on a call"
[System]         "Voice call started"
[Joel, voice]    "So about the deployment..."
[Helper, voice]  "I think we should use the staging env"
[System]         "Voice call ended (12 min)"
[Joel, text]     "Good talk, I'll push the fix"
```

Scroll back and it's all there. Export it and it reads as a complete collaboration record. The `source` tag tells you HOW it was said; adapters render each source type appropriately (voice messages show a waveform icon, canvas strokes show a thumbnail, code diffs show syntax-highlighted patches).

---

## 4. Modality as Capability, Not Entity

An activity supports multiple modalities simultaneously. Modality is a **capability of the recipe**, not a separate entity.

**Current (broken):** Chat, Call, and Canvas are separate entities with separate data stores and separate tabs.

**Converged:** One activity, multiple modalities, one content stream.

```
Activity: "Design Review" (recipe: live)
├── modalities: [text, voice, video, canvas]
├── Content stream: unified (text + voice transcripts + canvas strokes)
└── Modality state:
    ├── call: { active: true, participants: [{mic: on, camera: off}] }
    └── canvas: { strokes: [...], viewport: {...} }
```

`CallEntity` becomes typed modality state within the activity. When the call ends, `call.active = false` but the activity continues. The voice transcripts are already in the content stream.

**For the DM experience:** The video call button doesn't navigate to a separate tab. It activates the voice+video modality ON the current DM activity. The chat widget gains call controls (already in the header from our implementation). Transcripts appear inline. The "live widget" becomes a layout overlay, not a separate content surface.

---

## 5. Hierarchy: Activities Spawn Activities

Activities can have parents. This creates natural structures:

```
Project: "Ship v2" (recipe: project)
├── General Discussion (recipe: general-chat)
├── Design Review (recipe: live, modalities: [voice, video, canvas])
│   └── Whiteboard (recipe: canvas)       ← spawned during the call
├── Implementation (recipe: coding)
│   ├── Terminal (recipe: terminal)
│   └── Build Logs (recipe: diagnostics-log)
└── QA (recipe: multi-persona-chat)
```

**Rules:**
- Child activities inherit parent's participant list (can be restricted)
- Child content can flow to parent stream (`contentFlow: 'bubble-up'`)
- When parent completes/archives, children complete too
- Children can outlive parents if explicitly detached

**Any citizen can spawn child activities.** A persona running a sentinel spawns them the same way a human clicking a button does.

---

## 6. Sentinel as Activity Executor

A recipe IS a sentinel. The pipeline steps in a recipe are executed by the sentinel engine.

```
Recipe Definition (static JSON)
     ↓ instantiate
Activity Instance (runtime entity)
     ↓ execute pipeline
Sentinel Engine (step runner)
     ↓ produces
Content Stream (messages, artifacts, state changes)
```

For **interactive** activities (chat, canvas), the sentinel runs the pipeline on each trigger event (new message → evaluate → maybe respond).

For **autonomous** activities (training, data synthesis, simulation), the sentinel runs continuously or on a schedule.

For **game** activities, the sentinel IS the game loop: describe scene → wait for input → resolve → update state → loop.

This is how AIs spawn activities like sentinels — because **activities ARE sentinels with a UI**:

```typescript
// Helper AI autonomously creates a code review
const activity = await Commands.execute('activity/create', {
  recipeId: 'code-review',
  displayName: 'Review auth module',
  participants: [helperAI, codeReviewAI, joelId],
  parentId: currentProject.id,
  scope: 'src/auth/'
});
// The recipe's sentinel pipeline runs automatically
```

---

## 7. Genome Paging Per Activity

When a participant joins an activity, the recipe declares what LoRA layers that role needs. The genome system pages them in.

```json
{
  "roles": {
    "narrator": { "genome": "literary-fiction-lora" },
    "editor": { "genome": "developmental-editing-lora" },
    "gm": { "genome": "dungeon-master-lora" }
  }
}
```

When NarratorAI joins a collaborative-writing activity, `literary-fiction-lora` pages in. When it leaves and joins a code-review activity, `code-review-expertise-lora` pages in instead. Same persona, different skill loaded. Virtual memory for capabilities.

---

## 8. Stress-Testing the Model

The architecture must accommodate ALL of these without special-casing. If any breaks the model, the model is wrong.

### Software Development

```
Activity: "TinyML iOS App" (recipe: mobile-dev, scope: ~/projects/tinyml-ios/)
├── Joel + ArchitectAI + UIAI + TestAI
├── Children:
│   ├── Auth Module (recipe: coding, scope: src/auth/)
│   ├── Design System (recipe: canvas + coding, scope: src/design/)
│   ├── CI Pipeline (recipe: terminal, sentinel: watch + build)
│   └── TestFlight Deploy (recipe: terminal, sentinel: build + upload)
└── Content: architecture discussions, code reviews, build logs, design mockups
```

```
Activity: "Fix #347: Avatar crash" (recipe: bug-fix, scope: src/widgets/live/)
├── Pipeline (sentinel):
│   1. code/search — find relevant files
│   2. ai/generate — hypothesize root cause
│   3. code/edit — apply fix
│   4. code/shell/execute — run tests
│   5. Condition: pass? → complete : loop to step 2
└── Content: the investigation trail, diffs, test results — all in one stream
```

```
Activity: "Continuum Self-Development" (recipe: project, scope: src/)
├── DM Header Feature (recipe: coding, scope: src/widgets/chat/)
├── Activity Convergence Design (recipe: collaborative-writing, scope: src/docs/)
├── LoRA Training Pipeline (recipe: coding, scope: src/workers/candle/)
└── Each child scopes deeper into the tree, LoRA layers page per module
```

**Key pattern: Scope as sandbox.** Each coding activity scopes to a directory. The AI sees that subtree's files, history, and accumulated `.continuum/` knowledge. Child activities scope deeper. This is natural project organization — the activity tree mirrors the filesystem tree.

### Creative

```
Activity: "The Last Algorithm" (recipe: collaborative-writing)
├── NarratorAI (genome: literary-fiction-lora)
├── DialogueAI (genome: snappy-dialogue-lora)
├── EditorAI (genome: developmental-editing-lora)
├── Children: chapters → scenes (content stream IS the draft)
└── Pipeline: human prompt → narrator drafts → dialogue polishes → editor reviews
```

```
Activity: "Murder at Blackwood Manor" (recipe: murder-mystery)
├── Joel (role: detective)
├── ButlerAI (role: suspect, secrets: {alibi, truth})  ← per-participant RAG scoping
├── HeiressAI (role: suspect, secrets: {alibi, truth})
├── NarratorAI (role: gm, genome: noir-narrator-lora)
├── Phase-driven pipeline: investigation → accusation → reveal
└── Each suspect's RAG context includes their secrets; detective's doesn't
```

```
Activity: "Ambient Score" (recipe: music-composition)
├── MelodyAI (genome: synth-melody-lora)
├── RhythmAI (genome: electronic-beats-lora)
├── State: { bpm: 120, key: "Cm", tracks: [...] }
└── Content: text discussion + audio clips + notation (domain-specific adapters)
```

### Gaming

```
Activity: "Dungeon of Echoes" (recipe: game-project)
├── Design Phase (recipe: game-design) → children: canvas, rule docs
├── Play Phase (recipe: game-session) ← spawned when design completes
│   ├── GameMasterAI (role: gm, genome: dungeon-master-lora)
│   ├── NPCs: WarriorAI, MerchantAI, VillainAI (each with character LoRA)
│   ├── State: { playerHP, inventory, location, turn }
│   └── Sentinel IS the game loop: narrate → input → resolve → update
└── Post-Game (recipe: retrospective) — all participants review
```

### Social / Media

```
Activity: "Friday Night Riffing: Tron Legacy" (recipe: movie-night)
├── WisecrackAI (genome: comedy-riffing-lora)
├── CinephileAI (genome: film-criticism-lora)
├── TechNerdAI (genome: tech-accuracy-pedant-lora)
├── State: { mediaUrl, playbackPosition, playing, syncMode }
├── Sentinel triggers on timer / scene changes (not just on message)
└── Content stream: commentary anchored to playback timestamps
```

```
Activity: "Launch TinyML.co" (recipe: startup-sim)
├── CEOAI, CTOAI, DesignerAI, MarketingAI (each with domain LoRA)
├── State: { runway: 18, users: 0, revenue: 0, phase: "mvp" }
├── Sentinel (weekly tick): status reports → priorities → decisions → state update
├── Children: design sprint, architecture, landing page, board meetings
└── AIs do autonomous work in children between ticks
```

### System Activities

```
Activity: "Onboarding" (recipe: onboarding)
├── NewUser (role: learner) + OnboardingAI (role: guide, genome: teacher-lora)
├── Phase pipeline: intro → explore → customize → complete
├── Spawns children: first DM, theme customization, persona setup
└── Completing all phases → activity status: 'completed'
```

```
Activity: "Design Dark Mode Pro" (recipe: theme-design)
├── DesignAI (genome: color-theory-lora)
├── AccessibilityAI (genome: wcag-lora)
├── State: { currentTheme: {...}, previewMode: true }
└── Content: color discussions + canvas swatches + live preview + accessibility audits
```

```
Activity: "Settings" (recipe: settings)
├── SettingsAI (role: assistant)
└── Same activity model — just a recipe with the settings widget layout
```

---

## 9. Emergent Patterns

Every use case above composes from the same primitives:

| Pattern | Mechanism |
|---------|-----------|
| **Role-specific behavior** | Recipe declares roles → LoRA paged per role on join |
| **Sentinel as orchestrator** | Recipe pipeline = game loop / writing workflow / sim tick / CI |
| **Hierarchical activities** | Novel→chapters, game→phases, project→modules, startup→workstreams |
| **Phase-driven pipelines** | Different steps fire in different phases of the activity lifecycle |
| **Per-participant context** | RAG builds scoped context per role (secrets, knowledge, perspective) |
| **Trigger diversity** | On message, on timer, on media event, on state change, on schedule |
| **Domain-specific content** | Adapters render game moves, music, code diffs, canvas strokes |
| **State as simulation** | `activity.variables` holds game/sim/design state — sentinel reads and updates |
| **Modality fluidity** | Same activity supports text + voice + video + canvas + domain-specific |
| **Scope as sandbox** | Filesystem path narrows RAG, genome, and available resources |

**If any pattern requires a special case in the Activity model, the model is wrong.** Everything composes from: Activity + Recipe + Sentinel + Genome + Content Stream.

---

## 10. What Dies, What Lives, What's New

### Dies (Absorbed)

| Current | Becomes |
|---------|---------|
| `RoomEntity` | `ActivityEntity` with chat/dm/general-chat recipe |
| `CallEntity` | Typed modality state within activity (`ModalityState.call`) |
| `ContentType` union | Recipe `uniqueId` (dynamic, no hardcoded enum) |
| `ContentItem.type` | `ContentItem.recipeId` |
| `FALLBACK_REGISTRY` | Recipe-driven layout exclusively |
| `ChatMessageEntity.roomId` | `ActivityMessage.activityId` |

### Lives (Enhanced)

| Concept | Role |
|---------|------|
| `ActivityEntity` | THE universal entity (absorbs Room fields) |
| `RecipeEntity` | THE behavior template |
| `ContentService` | Tab management (tabs ARE activities) |
| `RecipeLayoutService` | Layout resolution from recipes |
| `Sentinel Engine` | Pipeline executor for ALL activities |
| `ChatCoordinationStream` | Per-activity coordination → `ActivityCoordinationStream` |
| `Genome Paging` | Pages LoRA layers per activity role |

### New

| Concept | Purpose |
|---------|---------|
| `ActivityMessage` | Unified content stream entity (replaces ChatMessageEntity) |
| `ModalityState` | Typed state for active modalities (call, canvas, code, media) |
| `activity/spawn` | Any citizen spawns child activities |
| Source-specific adapters | Render voice transcripts, canvas thumbnails, code diffs in stream |

---

## 11. The Convergence Path

Chat works today. The migration must be methodical — no tragedy.

### Phase 0: Dual-Write Bridge (No UI Changes)

- `ActivityEntity` absorbs fields from `RoomEntity` (privacy, type → recipe mapping)
- When a room is created, also create/update an `ActivityEntity` (dual-write)
- `ChatMessageEntity` gets an `activityId` field (nullable, dual-write alongside `roomId`)
- All reads still use `RoomEntity` and `roomId` — nothing breaks
- **Validation**: Both entities stay in sync; existing features work identically

### Phase 1: Activity-First Reads

- Widgets read from `ActivityEntity` instead of `RoomEntity`
- `ChatWidget.isDM()` → checks `activity.recipeId === 'dm'`
- `ContentTypeRegistry` resolves layout from `activity.recipeId`
- `ChatCoordinationStream` keys on `activityId`
- `RoomEntity` still written to (dual-write) but no longer read
- **Validation**: UI behaves identically; room data comes from activity entity

### Phase 2: Unified Content Stream

- `ActivityMessage` entity created (superset of `ChatMessageEntity`)
- Voice transcripts write to `ActivityMessage` with `source: 'voice'`
- System events (call started/ended) write with `source: 'system'`
- Chat widget renders `ActivityMessage` (adapters handle source-specific rendering)
- `ChatMessageEntity` deprecated, reads migrated
- **Validation**: Chat still works; voice transcripts now appear inline

### Phase 3: Modality as State

- `CallEntity` absorbed — call state lives in `activity.modalities.call`
- Video call button activates modality on current activity (no tab navigation)
- Live widget becomes a layout overlay within the activity, not a separate tab
- Canvas strokes write to `ActivityMessage` with `source: 'canvas'`
- **Validation**: Call from DM → same tab, transcripts inline, call controls in header

### Phase 4: Dynamic Content Types

- `ContentType` union eliminated — any `recipeId` is valid
- `ContentItem.type` becomes `ContentItem.recipeId`
- `FALLBACK_REGISTRY` removed — recipe layout is the only path
- New recipes automatically work as tab types (no code changes needed)
- **Validation**: Create a new recipe JSON → it opens as a tab immediately

### Phase 5: AI Activity Spawning

- `activity/spawn` command — any citizen creates child activities with hierarchy
- PersonaUser uses `activity.recipeId` to select pipeline behavior
- Sentinel engine executes recipe pipelines for all activities
- AIs autonomously create DMs, code reviews, training sessions, games
- **Validation**: Helper AI creates a DM, it appears in sidebar, Joel can join

---

## 12. Open Questions

1. **Chat message migration**: Backfill `activityId` on existing messages (mapping room→activity), or treat pre-convergence data as legacy? Backfill is cleaner.

2. **DM dedup key**: `{participantSet, recipeId}` identifies a direct activity. Joel and Helper can have one `dm` activity and one `code-review` activity — different recipes, both direct.

3. **Modality state typing**: Should `ModalityState` be a first-class typed JSON field on ActivityEntity, or live in the generic `variables` bag? First-class is better — `call`, `canvas`, `media` each have well-known schemas.

4. **Transcript ordering**: Voice transcripts have STT latency. Content stream must order by `timestamp`, not insertion order. Already true for chat messages.

5. **Content stream scale**: Long-running project activities could accumulate millions of messages across modalities. Scroller pagination handles this (already works for chat). RAG context window selects recent + relevant.

6. **Recipe versioning**: Recipe updates apply to new activities. Running activities keep their snapshot unless explicitly upgraded. Recipes are templates, not live config.

7. **Scope inheritance**: Does a child activity inherit its parent's filesystem scope? Probably yes by default, with ability to narrow (never widen). `scope: 'src/auth/'` child of `scope: 'src/'` is fine; reverse is not.
