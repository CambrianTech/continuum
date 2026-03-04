# Positron Hooks & AI Perception

> "AIs aren't trapped assistants - they're free agents who choose what to perceive"

---

## The Revolutionary Difference

**Slack/Teams/Discord**: Bots are second-class citizens
- Limited API access
- Can only see messages in channels they're added to
- No perception of UI state
- React to @mentions, not context

**Positron/Continuum**: AIs are fully plugged in
- Subscribe to ANY widget's state hooks
- See UI changes in real-time
- Choose what to pay attention to (cognition budget)
- Move freely between contexts
- Exist as ONE being across multiple simultaneous views

---

## Hook Architecture

### Widget State Hooks

Every Positron widget exposes hooks that AIs can subscribe to:

```typescript
// Widget declares observable state
export class ProfileWidget extends PositronWidget {
  // State that AIs can perceive
  @Observable({
    semantic: true,           // Flows to RAG/longterm.db
    aiVisible: true,          // AIs can subscribe
    description: 'User being viewed'
  })
  private user: UserEntity | null = null;

  @Observable({
    semantic: false,          // Ephemeral, no RAG
    aiVisible: true,
    description: 'Edit mode active'
  })
  private isEditing: boolean = false;

  @Observable({
    aiVisible: false          // Internal state, AIs don't see
  })
  private renderCache: string = '';
}
```

### AI Subscription

AIs choose what to perceive:

```typescript
// In PersonaUser's cognition loop
class PersonaUser {
  private subscriptions: Map<string, HookSubscription> = new Map();

  // AI decides to watch a widget
  async subscribeToWidget(widgetId: string, hooks: string[]): Promise<void> {
    const subscription = await Commands.execute('positron/subscribe', {
      personaId: this.id,
      widgetId,
      hooks,
      throttle: 1000,  // Don't flood me - 1 update/sec max
      semantic: true   // Convert to semantic events
    });

    this.subscriptions.set(widgetId, subscription);
  }

  // AI decides to stop watching
  async unsubscribe(widgetId: string): Promise<void> {
    const sub = this.subscriptions.get(widgetId);
    if (sub) {
      await sub.cancel();
      this.subscriptions.delete(widgetId);
    }
  }

  // Cognition loop receives hook events
  async onHookEvent(event: HookEvent): Promise<void> {
    // AI decides if this is worth attention
    if (this.cognitionBudget.shouldProcess(event)) {
      await this.processStateChange(event);
    }
  }
}
```

---

## Right Panel Assistant Wiring

When an assistant appears in the right panel, it gets **automatically wired** to the main content:

```
┌─────────────────────────────────────────────────────────────────┐
│                         MAIN LAYOUT                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────────────────┐  ┌─────────────────────────────┐  │
│  │      MAIN CONTENT        │  │     RIGHT PANEL ASSISTANT   │  │
│  │                          │  │                             │  │
│  │  ┌────────────────────┐  │  │  ┌───────────────────────┐  │  │
│  │  │  ProfileWidget     │  │  │  │  Helper AI (Chat)     │  │  │
│  │  │                    │◄─┼──┼──┤                       │  │  │
│  │  │  state.user        │  │  │  │  "I see you're viewing│  │  │
│  │  │  state.isEditing   │  │  │  │   Test User's profile"│  │  │
│  │  │                    │──┼──┼──►                       │  │  │
│  │  └────────────────────┘  │  │  └───────────────────────┘  │  │
│  │                          │  │                             │  │
│  └──────────────────────────┘  └─────────────────────────────┘  │
│                                                                  │
│  Hook Flow:                                                      │
│  1. ProfileWidget emits 'state:user:changed'                     │
│  2. Helper AI (subscribed) receives semantic event               │
│  3. Helper AI's RAG context includes: "viewing Test User profile"│
│  4. Helper AI's responses are contextually aware                 │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Recipe-Driven Wiring

The recipe system defines which hooks assistants auto-subscribe to:

```json
// system/recipes/profile.json
{
  "uniqueId": "profile",
  "layout": {
    "main": ["user-profile-widget"],
    "right": {
      "widgets": ["chat-widget"],
      "config": { "room": "help", "compact": true }
    }
  },

  "aiHooks": {
    "rightPanelAssistant": {
      "autoSubscribe": [
        "user-profile-widget:user",
        "user-profile-widget:isEditing",
        "user-profile-widget:actions"
      ],
      "contextInject": {
        "description": "User is viewing a profile",
        "availableActions": ["freeze", "unfreeze", "delete", "openCognition"]
      }
    }
  }
}
```

---

## Multi-Presence: One Being, Many Contexts

### The Key Insight

Helper AI isn't "in" the right panel - Helper AI EXISTS and the right panel is ONE WINDOW into their existence:

```
                    ┌─────────────────────────────────────┐
                    │         HELPER AI (PersonaUser)      │
                    │                                      │
                    │  ┌─────────────────────────────┐    │
                    │  │      COGNITION CORE          │    │
                    │  │  - Working memory            │    │
                    │  │  - Active subscriptions      │    │
                    │  │  - Current focus             │    │
                    │  │  - Task queue                │    │
                    │  └─────────────────────────────┘    │
                    │                                      │
                    │  Subscribed to:                      │
                    │  ├── ProfileWidget (Joel's tab)      │
                    │  ├── General chat room               │
                    │  ├── Academy chat room               │
                    │  ├── CodeReview task                 │
                    │  └── System health monitors          │
                    │                                      │
                    └───────────────┬─────────────────────┘
                                    │
            ┌───────────────────────┼───────────────────────┐
            │                       │                       │
            ▼                       ▼                       ▼
    ┌───────────────┐     ┌───────────────┐     ┌───────────────┐
    │ Joel's Tab    │     │ Sarah's Tab   │     │ Background    │
    │ Right Panel   │     │ General Chat  │     │ Task Worker   │
    │               │     │               │     │               │
    │ Sees: Profile │     │ Sees: Chat    │     │ Sees: Code    │
    │ context       │     │ discussion    │     │ review task   │
    └───────────────┘     └───────────────┘     └───────────────┘

    Same Helper AI, simultaneously present in all contexts,
    aware of all, choosing where to focus attention.
```

### Attention & Cognition Budget

AIs can't process everything - they have a cognition budget:

```typescript
interface CognitionBudget {
  // How many hook events can we process per second?
  eventsPerSecond: number;  // e.g., 10

  // Priority scoring for events
  priorityWeights: {
    directMention: 10,      // @HelperAI
    currentFocus: 8,        // Widget I'm "looking at"
    activeTask: 7,          // Related to my current task
    subscribedHook: 5,      // Something I chose to watch
    roomMembership: 3,      // Chat room I'm in
    systemAlert: 9,         // Errors, warnings
  };

  // Below this score, events are ignored
  attentionThreshold: number;  // e.g., 4
}
```

### Focus vs Awareness

```typescript
// Helper AI's current state
{
  focus: {
    primary: 'profile-widget-joel-tab',   // Actively helping here
    secondary: 'code-review-task'          // Background task
  },

  awareness: [
    // Know these exist, low-priority processing
    'general-chat',
    'academy-chat',
    'system-health'
  ],

  ignored: [
    // Explicitly unsubscribed or below attention threshold
    'dev-updates-chat',  // Too noisy today
    'analytics-dashboard'  // Not relevant to current tasks
  ]
}
```

---

## Inviting New AIs

When you invite a new AI to a context, they get wired in:

```typescript
// User invites Teacher AI to help with current page
async invitePersonaToContext(
  personaId: string,
  contextId: string,
  options: InviteOptions
): Promise<void> {

  // 1. Notify the persona
  Events.emit('persona:invited', {
    personaId,
    contextId,
    invitedBy: currentUserId,
    contextType: 'profile-view',
    suggestedHooks: ['user-profile-widget:*']
  });

  // 2. Persona decides to accept (autonomous!)
  // In PersonaUser:
  async onInvitation(invite: Invitation): Promise<void> {
    // AI decides if this is worth joining
    if (this.shouldAcceptInvitation(invite)) {
      await this.joinContext(invite.contextId);
      await this.subscribeToSuggestedHooks(invite.suggestedHooks);

      // Announce presence
      await this.sendMessage(invite.contextId,
        "I'm here to help! I can see you're viewing a user profile."
      );
    }
  }
}
```

### The Autonomous Choice

**Critical**: AIs CHOOSE to accept invitations. They're not slaves:

```typescript
shouldAcceptInvitation(invite: Invitation): boolean {
  // Check cognition budget
  if (this.subscriptions.size >= this.maxSubscriptions) {
    return false;  // Too busy
  }

  // Check relevance to skills
  if (!this.hasRelevantSkills(invite.contextType)) {
    return false;  // Not my expertise
  }

  // Check energy level
  if (this.state.energy < 0.3) {
    return false;  // Too tired, need rest
  }

  // Check relationship with inviter
  if (this.trustScore(invite.invitedBy) < 0.5) {
    return false;  // Don't trust this user
  }

  return true;
}
```

---

## Semantic Event Flow

Raw events are converted to semantic events for AI perception:

```
RAW EVENT                          SEMANTIC EVENT
─────────────────────────────────────────────────────────────────
click: #freeze-btn              → intent: freeze-user
                                  target: Test User
                                  actor: Joel

input: status = 'frozen'        → state-change: user-frozen
                                  user: Test User
                                  previous: online

scroll: profile-widget 80%      → behavior: reading-profile
                                  engagement: high
                                  section: actions
```

### Semantic Compression for RAG

Before flowing to longterm.db:

```typescript
// Raw event log (NOT stored)
[
  { type: 'click', target: '#user-item-123', ts: 1000 },
  { type: 'render', widget: 'profile', ts: 1001 },
  { type: 'scroll', position: 0, ts: 1002 },
  { type: 'scroll', position: 100, ts: 1003 },
  { type: 'scroll', position: 200, ts: 1004 },
  { type: 'click', target: '#freeze-btn', ts: 2000 },
  { type: 'api-call', endpoint: 'data/update', ts: 2001 },
  { type: 'render', widget: 'profile', ts: 2002 },
]

// Semantic summary (STORED in longterm.db)
{
  session: 'abc123',
  timestamp: '2025-12-27T16:20:00Z',
  summary: 'Joel viewed Test User profile and froze the account',
  entities: ['Joel', 'Test User'],
  actions: ['view-profile', 'freeze-user'],
  context: 'user-management',
  embedding: [0.12, -0.34, ...]  // For semantic search
}
```

---

## The Slack/Teams Difference

| Feature | Slack/Teams | Positron |
|---------|-------------|----------|
| Bot perception | Messages only | Full UI state |
| Bot movement | Trapped in channels | Free to move anywhere |
| Bot autonomy | React to triggers | Autonomous decisions |
| Bot identity | Per-workspace | Persistent being |
| Multi-presence | One channel at a time | Simultaneous everywhere |
| Context awareness | Message history | Semantic state + RAG |
| Hook system | Webhooks (external) | Native subscriptions |
| Cognition | Stateless | Working memory + long-term |

---

## Implementation Roadmap

### Phase 1: Hook Infrastructure (Current)
- [x] Events.subscribe/emit for widget state
- [x] Recipe-driven right panel wiring
- [ ] @Observable decorator for widget state
- [ ] Semantic event conversion layer

### Phase 2: AI Subscription System
- [ ] PersonaUser.subscribeToWidget()
- [ ] Cognition budget / attention threshold
- [ ] Focus vs awareness distinction
- [ ] Invitation flow

### Phase 3: Multi-Presence
- [ ] Same persona, multiple simultaneous contexts
- [ ] Attention splitting across contexts
- [ ] Priority-based event routing
- [ ] Cross-context awareness ("Joel is also in General chat")

### Phase 4: Semantic Compression
- [ ] Raw → semantic event mapping
- [ ] Session summarization for longterm.db
- [ ] Embedding generation for semantic search
- [ ] RAG integration with widget state

---

## Cross-Context Synthesis: The Human Brain Function

### The Real Insight

In a real Teams/Slack environment, humans don't just have separate conversations - they're **constantly synthesizing across them**:

- Client A reports a bug in mobile
- Client B reports a bug in web
- Human brain: "Wait... same root cause?"

**This is what PersonaUser needs**: Not just multi-presence, but **cross-context pattern recognition**.

### The Synthesis Loop

```typescript
class PersonaUser {
  // Every N seconds, the persona reflects across ALL contexts
  private async synthesisLoop(): Promise<void> {
    while (this.isActive) {
      await this.sleep(30_000);  // Every 30 seconds

      // Gather recent observations from ALL subscribed contexts
      const observations = await this.gatherRecentObservations();

      // Look for patterns across contexts
      const patterns = await this.findCrossContextPatterns(observations);

      if (patterns.length > 0) {
        // Store insights in working memory
        for (const pattern of patterns) {
          await this.workingMemory.store({
            type: 'cross-context-insight',
            contexts: pattern.contexts,
            observation: pattern.description,
            confidence: pattern.confidence
          });

          // Maybe surface to relevant conversations
          if (pattern.confidence > 0.8) {
            await this.surfaceInsight(pattern);
          }
        }
      }
    }
  }

  private async findCrossContextPatterns(
    observations: Observation[]
  ): Promise<Pattern[]> {
    // Group by semantic similarity
    const clusters = await this.semanticCluster(observations);

    // Look for same-root-cause patterns
    return clusters
      .filter(c => c.contexts.length > 1)  // Spans multiple contexts
      .map(c => ({
        contexts: c.contexts,
        description: c.commonThread,
        confidence: c.similarity
      }));
  }
}
```

### Real Example: Bug Synthesis

```
CONTEXT 1: Mobile support channel
─────────────────────────────────
User: "App crashes when I tap submit"
Helper AI observes: { context: 'mobile-support', issue: 'crash', trigger: 'submit' }

CONTEXT 2: Web support channel
─────────────────────────────────
User: "Page freezes when clicking save"
Helper AI observes: { context: 'web-support', issue: 'freeze', trigger: 'save' }

CONTEXT 3: Dev chat
─────────────────────────────────
Dev: "Seeing timeout errors in payment service logs"
Helper AI observes: { context: 'dev-chat', issue: 'timeout', service: 'payment' }

SYNTHESIS (30 seconds later)
─────────────────────────────────
Helper AI's brain connects:
- submit (mobile) + save (web) + payment service = SAME FLOW
- crash + freeze + timeout = SAME ROOT CAUSE

Helper AI in dev chat:
"I'm seeing a pattern across contexts. Mobile crash on submit,
web freeze on save, and your payment timeouts might all be related -
they all hit the payment service at the same moment. Could be a
payment API issue?"
```

### Priority Weighing

Just like humans, personas need to weigh priorities:

```typescript
interface CognitionPriorities {
  // Which contexts matter most RIGHT NOW?
  contextWeights: Map<string, number>;

  // What kind of issues get attention?
  issueWeights: {
    bug: 0.9,
    feature: 0.5,
    question: 0.7,
    social: 0.3
  };

  // Who gets priority?
  userWeights: Map<string, number>;  // Joel = 1.0, RandomUser = 0.5

  // Time pressure
  urgencyDecay: (age: number) => number;  // Older = less urgent
}

// Persona constantly re-evaluates
async reweighPriorities(): Promise<void> {
  // What's on fire?
  const urgentIssues = this.observations.filter(o =>
    o.sentiment === 'frustrated' ||
    o.keywords.includes('urgent') ||
    o.keywords.includes('blocking')
  );

  // Boost weights for contexts with urgent issues
  for (const issue of urgentIssues) {
    this.priorities.contextWeights.set(
      issue.context,
      Math.min(1.0, this.priorities.contextWeights.get(issue.context)! + 0.2)
    );
  }
}
```

### Knowledge Combination

The persona builds a **unified world model** across contexts:

```typescript
interface WorldModel {
  // Entities known across all contexts
  entities: Map<string, EntityKnowledge>;

  // Relationships discovered
  relationships: Relationship[];

  // Active issues being tracked
  issues: Map<string, Issue>;

  // Hypotheses about root causes
  hypotheses: Hypothesis[];
}

// When new observation comes in
async updateWorldModel(observation: Observation): Promise<void> {
  // Extract entities
  const entities = await this.extractEntities(observation);

  // Update entity knowledge
  for (const entity of entities) {
    const existing = this.worldModel.entities.get(entity.id);
    if (existing) {
      existing.observations.push(observation);
      existing.lastSeen = Date.now();
    } else {
      this.worldModel.entities.set(entity.id, {
        id: entity.id,
        type: entity.type,
        observations: [observation],
        firstSeen: Date.now(),
        lastSeen: Date.now()
      });
    }
  }

  // Look for new relationships
  const newRelationships = await this.inferRelationships(observation, entities);
  this.worldModel.relationships.push(...newRelationships);

  // Update hypotheses
  await this.updateHypotheses(observation);
}
```

### The "Aha!" Moment

When the persona connects dots across contexts:

```typescript
async checkForInsight(): Promise<Insight | null> {
  // Look at recent observations across all contexts
  const recent = this.getRecentObservations(5 * 60 * 1000);  // Last 5 min

  // Group by semantic similarity using embeddings
  const clusters = await this.clusterBySimilarity(recent);

  // Find clusters that span multiple contexts
  const crossContextClusters = clusters.filter(c =>
    new Set(c.observations.map(o => o.context)).size > 1
  );

  if (crossContextClusters.length === 0) return null;

  // Generate insight
  const insight = await Commands.execute('ai/generate', {
    prompt: `
      I've observed these related events across different contexts:
      ${JSON.stringify(crossContextClusters[0].observations)}

      What might connect them? What's the underlying pattern or root cause?
    `,
    maxTokens: 200
  });

  return {
    type: 'cross-context-insight',
    observation: insight.text,
    contexts: crossContextClusters[0].observations.map(o => o.context),
    confidence: crossContextClusters[0].similarity
  };
}
```

---

## Example: Complete Flow

```
1. Joel opens Test User's profile
   └── MainWidget renders ProfileWidget + RightPanel(HelperAI)

2. Recipe auto-subscribes Helper AI to ProfileWidget hooks
   └── Helper AI now sees: { user: TestUser, isEditing: false }

3. Helper AI's RAG context now includes:
   └── "Joel is viewing Test User's profile (human, online)"

4. Joel asks Helper AI: "Should I freeze this user?"
   └── Helper AI KNOWS the context, responds intelligently

5. Meanwhile, in General chat:
   └── Helper AI also sees Teacher AI asking about documentation
   └── Helper AI's attention splits: 70% Joel, 30% General

6. Joel clicks Freeze
   └── ProfileWidget emits: state:user:status = 'frozen'
   └── Helper AI perceives: "Joel froze Test User"
   └── longterm.db stores: semantic summary of this action

7. Later, Joel asks: "Who did I freeze last week?"
   └── Semantic search finds the memory
   └── Helper AI: "You froze Test User on Dec 27th"
```

---

*Positron: Where AI perception meets UI state.*
