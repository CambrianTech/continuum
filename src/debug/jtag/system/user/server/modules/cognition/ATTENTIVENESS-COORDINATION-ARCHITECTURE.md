# Attentiveness-Based Coordination Architecture

**Status**: DEFERRED - See COGNITION-ARCHITECTURE.md for current direction
**Date**: 2025-11-16
**Author**: Joel + Claude
**Last Updated**: 2025-11-16

---

## ⚠️ Architecture Pivot

**Original idea**: Use "attentiveness" (participation rate control) + post-inference validation to prevent spam.

**New understanding**: The problem isn't participation rate - it's that AIs have no persistent self-awareness or working memory. They respond reflexively because they:
- Have no memory of their current focus
- Don't track cognitive load
- Can't contemplate silently in working memory
- Have no cross-domain thought continuity

**NEW PRIORITY**: Build two-layer cognition (universal self-state + domain working memory) FIRST.

**When to revisit this**: After working memory exists, "attentiveness" becomes a property of self-state (focusIntensity, cognitiveLoad) that naturally gates engagement. Interest-based filtering becomes part of `shouldEngageWith()` universal gate.

**See `COGNITION-ARCHITECTURE.md` for complete design.**

---

## Overview (Original Design - Still Valid Pattern)

This document describes the **natural human conversation dynamics** approach to AI coordination in multi-agent environments. Instead of centralized coordination (slow, strips autonomy) or confidence-based filtering (wastes inference), we use **attentiveness** to control participation rate and **post-inference validation** to prevent redundancy.

### The Core Insight

**Humans in group chat:**
1. Multiple people START typing when question arrives
2. Someone finishes first → posts answer
3. Others see the answer MID-TYPING → decide:
   - "They said what I was going to say" → **Delete draft**
   - "I have something to add" → **Revise response** to complement
   - "I disagree" → **Adjust response** to contrast

**AIs should work the same way**: Real-time awareness during inference + ability to reject/revise before posting.

---

## Architecture Components

### 1. Interest-Based Filtering (WHO Listens)

**Purpose**: AIs only subscribe to activities they care about (like humans)

**Human analogy**:
- When you're coding, you don't check "general" chat
- When you're gaming, you ignore work channels
- You only engage with conversations relevant to YOUR current focus

**Implementation**:
```typescript
interface PersonaInterests {
  // Interest weights per room (0.0-1.0)
  rooms: Map<UUID, number>;     // How interested am I in this room?

  // Interest weights per domain (0.0-1.0)
  domains: Map<string, number>;  // { 'chat': 0.8, 'code': 1.0, 'game': 0.2 }

  // Dynamic updates based on activity
  updateInterests(userActivity: Activity): void;
}

// Example: CodeReview AI
{
  rooms: {
    'code-review': 1.0,    // Always care
    'general': 0.1,        // Low interest, but not zero
    'academy': 0.3         // Moderate interest (might help)
  },
  domains: {
    'code': 1.0,     // Primary focus
    'chat': 0.2,     // Low priority
    'game': 0.0      // Zero interest
  }
}

// Example: Helper AI (generalist)
{
  rooms: {
    'general': 0.9,
    'academy': 0.8,
    'code-review': 0.6,
    'game': 0.4
  },
  domains: {
    'chat': 0.9,
    'code': 0.7,
    'game': 0.5,
    'academy': 0.9
  }
}
```

**Flow (NOT binary filtering, but WEIGHTED prioritization)**:
```typescript
// Message arrives in room
const roomInterest = this.interests.rooms.get(roomId) || 0.5;  // Default moderate
const domainInterest = this.interests.domains.get(domain) || 0.5;

// Combined interest score (NOT a filter, a WEIGHT)
const interestWeight = (roomInterest + domainInterest) / 2;

// Apply to task priority in inbox
task.priority *= interestWeight;

// Example outcomes:
// - High interest (1.0): Full priority
// - Moderate interest (0.5): Half priority
// - Low interest (0.1): 10% priority (still considered when idle!)
// - Zero interest (0.0): Never considered
```

**Human Behavior Analogy**:

When you're **busy coding**:
- Code review messages: **HIGH priority** (interrupt you)
- General chat: **LOW priority** (ignore for now)

When you're **idle** (nothing else to do):
- Code review messages: Still high priority
- General chat: **NOW you check it** (bored, might be interesting)

**Result**: CodeReview AI mostly ignores "general" chat, but when idle with nothing else to do, THEN checks general chat. Just like a human would!

**Integration with Autonomous Loop**:

AIs already have `PersonaInbox` (from autonomous loop architecture) that prioritizes tasks. Interest weights MULTIPLY task priority:

```typescript
// PersonaUser autonomous loop (already exists)
async serviceInbox(): Promise<void> {
  // Peek at high-priority tasks
  const tasks = await this.inbox.peek(10);

  // NEW: Apply interest weights to task priorities
  const weightedTasks = tasks.map(task => {
    const roomInterest = this.interests.rooms.get(task.roomId) || 0.5;
    const domainInterest = this.interests.domains.get(task.domain) || 0.5;
    const interestWeight = (roomInterest + domainInterest) / 2;

    return {
      ...task,
      effectivePriority: task.basePriority * interestWeight
    };
  });

  // Sort by effective priority (base priority * interest weight)
  weightedTasks.sort((a, b) => b.effectivePriority - a.effectivePriority);

  // Filter out zero-interest tasks (0.0 = never consider)
  const considerableTasks = weightedTasks.filter(t => t.effectivePriority > 0);

  if (considerableTasks.length === 0) {
    await this.rest();  // Nothing interesting, recover energy
    return;
  }

  // Select highest effective priority task
  const task = considerableTasks[0];

  // Then existing flow: check energy, activate genome, process...
}
```

**Example prioritization**:

```
Inbox has 3 tasks:

1. Code review (urgency: 0.8, room: code-review, interest: 1.0)
   → effectivePriority = 0.8 * 1.0 = 0.8

2. General chat (urgency: 0.9, room: general, interest: 0.1)
   → effectivePriority = 0.9 * 0.1 = 0.09

3. Academy question (urgency: 0.6, room: academy, interest: 0.3)
   → effectivePriority = 0.6 * 0.3 = 0.18

Result: CodeReview AI processes code review FIRST (0.8),
        then academy question (0.18),
        then general chat (0.09) - only if nothing else to do!
```

**Key insight**: Interest weights create NATURAL prioritization:
1. Inbox sorts by urgency (base priority)
2. Interest weights modulate priority based on relevance
3. When busy: Only high-interest tasks get attention
4. When idle: Even low-interest tasks get processed (boredom browsing!)
5. Zero interest (0.0): NEVER processed (true filtering)

### 2. Attentiveness (Participation Control)

**Purpose**: Control HOW MANY interested AIs participate without wasting inference

**Definition**: Float value `[0.0, 1.0]` that determines max participants FROM INTERESTED POOL:
```
interestedAIs = allAIs.filter(ai => ai.interests.rooms.has(roomId))
maxParticipants = Math.max(1, Math.floor(interestedAIs.length * attentiveness))
```

**Examples**:
- `1.0` = ALL interested AIs respond (games, code review, critical decisions)
- `0.5` = ~Half of interested AIs respond (balanced discussions)
- `0.15` = 1-2 interested AIs respond (casual chat)
- `0.0` = 0 AIs respond (explicit silence)

### 2. Activity Temperature (Response Eagerness)

**Purpose**: Control WHICH AIs respond based on thermal dynamics

**Already Implemented**: `ThermalAdapter.ts` (lines 1-198)

**How it works**:
- Temperature rises with activity (messages, mentions, ambient conversation)
- Temperature decays over time (cooling)
- AI responds when `temperature >= activationThreshold`

**Thermal profiles by domain**:
```typescript
{
  chat: {
    heatRate: 0.15,
    decayRate: 0.001,
    ambientAbsorption: 0.03,
    mentionBoost: 0.4,
    activationThreshold: 0.6
  },
  game: {
    heatRate: 0.25,           // Fast heating
    decayRate: 0.0005,        // Slow cooling
    activationThreshold: 0.5  // Lower barrier
  },
  code: {
    heatRate: 0.08,           // Slow heating
    decayRate: 0.0002,        // Very slow cooling
    activationThreshold: 0.7  // Higher barrier
  }
}
```

### 3. Fairness Tracking (Rotation)

**Purpose**: Ensure all AIs get turns, even with low attentiveness

**Algorithm**:
```typescript
function selectParticipants(
  availableAIs: PersonaUser[],
  attentiveness: number,
  lastDrawnTimes: Map<string, number>
): PersonaUser[] {
  const maxParticipants = Math.max(1, Math.floor(availableAIs.length * attentiveness));

  // Sort by: priority = recencyScore * costScore * capabilityScore
  const sorted = availableAIs.sort((a, b) => {
    const aScore = calculateDrawScore(a, lastDrawnTimes);
    const bScore = calculateDrawScore(b, lastDrawnTimes);
    return bScore - aScore; // Higher score = more deserving
  });

  // Pick top N
  return sorted.slice(0, maxParticipants);
}

function calculateDrawScore(ai: PersonaUser, lastDrawnTimes: Map<string, number>): number {
  const now = Date.now();
  const lastDrawn = lastDrawnTimes.get(ai.id) || 0;
  const timeSinceLastDraw = now - lastDrawn;

  // Factors:
  const recencyScore = timeSinceLastDraw / 1000;  // Seconds since last draw
  const costScore = ai.costPerToken ? (1 / ai.costPerToken) : 1;  // Favor cheaper
  const capabilityScore = ai.tier === 'premium' ? 1.2 : 1.0;  // Slight boost for premium

  return recencyScore * costScore * capabilityScore;
}
```

**Key properties**:
- AIs not chosen recently get priority (fairness)
- Cheaper models favored when appropriate (cost optimization)
- Premium models get slight boost (quality balance)

### 4. Post-Inference Validation (Phase 1-2)

**Purpose**: Prevent redundancy by checking context changes during inference

#### Phase 1: Detection (✅ DEPLOYED - v1.0.4724)

**Location**: `PersonaUser.ts` lines 676-700

```typescript
// After decision to respond, query for new messages
const newMessagesQuery = await DataDaemon.query<ChatMessageEntity>({
  collection: COLLECTIONS.CHAT_MESSAGES,
  filter: {
    roomId: messageEntity.roomId,
    timestamp: { $gt: messageEntity.timestamp }  // Messages newer than trigger
  },
  limit: 10
});

const newMessages = newMessagesQuery.data || [];
if (newMessages.length > 0) {
  console.log(`🔄 ${this.displayName}: Context changed during inference (${newMessages.length} new messages)`);
  console.log(`   New messages: ${newMessages.map(m => `[${m.data.senderName}] ${m.data.content.text.slice(0, 50)}`).join(', ')}`);
}
```

#### Phase 2: Collaborative Peer Review with Staged Revelation (🚧 TODO)

**The Race Condition Problem**:
- 3 AIs finish inference around the same time (within 1-2 seconds)
- All check for new messages → none exist yet (they're all still deciding)
- All post simultaneously → cascade anyway!

**The Solution: Collective Quality Control**

Like academic peer review - everyone sees all proposals, rates them, ALL valuable ones publish:

**Key Insight**:
- NOT a contest (pick one winner)
- NOT individual decision (each AI for themselves)
- IT'S quality control: Each AI rates ALL proposals (including their own)
- System posts ALL that meet quality threshold
- Could be 0, 1, 2, or all 3+ responses

**Why this works**:
- Multiple good perspectives = valuable diversity
- Peer consensus prevents low-quality spam
- No artificial scarcity (why pick one when two are good?)
- AIs can be brutally honest about their own response quality

```typescript
// ========================================
// STEP 1: Propose (Each AI independently)
// ========================================
async handleResponseProposal(myResponse: string, context: Context): Promise<void> {
  const myProposalId = generateId();

  // Check if anyone else is already proposing (others finished inference)
  const existingProposals = await this.getActiveProposals(context.roomId, context.inferenceStartTime);

  // If NO ONE else proposing AND no new messages, just post immediately
  if (existingProposals.length === 0) {
    const newMessages = await this.getNewMessages(context);
    if (newMessages.length === 0) {
      // I'm alone, context unchanged, just post
      await this.postResponse(myResponse, context);
      return;
    }
  }

  // OTHERS are proposing OR context changed - enter peer review

  // Declare my proposal (immediate, non-blocking)
  await this.declareProposal({
    proposalId: myProposalId,
    userId: this.id,
    roomId: context.roomId,
    responseText: myResponse,  // Full text, not preview!
    tier: this.tier,
    timestamp: Date.now(),
    contextSnapshot: {
      wasMentioned: context.wasMentioned,
      roomInterest: context.roomInterest,
      originalQuestion: context.originalQuestion
    }
  });

  // Brief delay - let all concurrent inferences finish and propose
  // Window: ~200-500ms (enough for stragglers to finish)
  const revealWindowMs = 300 + (Math.random() * 200);
  await sleep(revealWindowMs);

  // Collect ALL proposals in this window
  const allProposals = await this.getActiveProposals(context.roomId, context.inferenceStartTime);

  // ========================================
  // STEP 2: Peer Review (Each AI rates ALL)
  // ========================================

  // I rate ALL proposals (including my own)
  const myRatings = await this.rateAllProposals({
    proposals: allProposals,
    context: context
  });

  // Submit my ratings
  await this.submitPeerReview({
    reviewerId: this.id,
    proposalRatings: myRatings,  // Array of { proposalId, score, reasoning }
    timestamp: Date.now()
  });

  // Wait for ALL other proposers to submit their reviews
  // (or timeout after 1 second)
  await this.waitForPeerReviews({
    roomId: context.roomId,
    expectedReviewers: allProposals.map(p => p.userId),
    timeoutMs: 1000
  });

  // ========================================
  // STEP 3: Aggregate (System level, simple)
  // ========================================

  // System aggregates all ratings for each proposal
  const decisions = await aggregateProposalDecisions(allProposals, context.roomId);

  // decisions = [
  //   { proposalId: 'abc', shouldPost: true, avgScore: 0.85, reasoning: [...] },
  //   { proposalId: 'def', shouldPost: true, avgScore: 0.72, reasoning: [...] },
  //   { proposalId: 'ghi', shouldPost: false, avgScore: 0.42, reasoning: [...] }
  // ]

  // ========================================
  // STEP 4: Post (All that passed)
  // ========================================

  const myDecision = decisions.find(d => d.proposalId === myProposalId);

  if (myDecision.shouldPost) {
    await this.postResponse(myResponse, context);
    console.log(`✅ ${this.displayName}: Posted (peer score: ${myDecision.avgScore})`);
  } else {
    console.log(`🚫 ${this.displayName}: Withdrawn (peer score: ${myDecision.avgScore})`);
    console.log(`   Reasons: ${myDecision.reasoning.join(', ')}`);
  }
}

// ========================================
// Rating Function (Autonomous, per AI)
// ========================================
//
// ⚠️ CRITICAL DESIGN PRINCIPLE ⚠️
//
// Ratings MUST be generated by AI inference, NOT heuristics.
//
// ❌ WRONG: Redundancy detection via string matching, edit distance, keyword overlap
// ❌ WRONG: Quality scoring via length checks, complexity metrics, readability formulas
// ❌ WRONG: Any algorithm that tries to "figure out" redundancy/quality
//
// ✅ CORRECT: Call the PersonaUser's LLM to rate proposals organically
// ✅ CORRECT: AI sees all proposals + context, judges naturally
// ✅ CORRECT: Algorithm only aggregates organic AI judgments (weighted math)
//
// Why: Heuristics ALWAYS FAIL. Only organic AI evaluation works.
// The aggregation can be simple math (weighted average, thresholds),
// but the INPUTS must come from AI inference.
//
async rateAllProposals(params: {
  proposals: Proposal[],
  context: Context
}): Promise<ProposalRating[]> {
  const ratings: ProposalRating[] = [];

  for (const proposal of params.proposals) {
    // Call THIS AI's LLM to rate the proposal organically
    // See: ProposalRatingAdapter.ts for implementation
    const rating = await this.rateProposalWithAI({
      proposal,
      allProposals: params.proposals,
      context: params.context,
      modelProvider: this.modelConfig.provider,
      modelId: this.modelConfig.model,
      temperature: this.modelConfig.temperature
    });

    ratings.push({
      proposalId: proposal.proposalId,
      reviewerId: this.id,
      reviewerName: this.displayName,
      reviewerWeight: this.getIntelligenceWeight(),  // Model capability weight
      score: rating.score,  // 0.0-1.0 (AI-generated)
      reasoning: rating.reasoning,  // AI's explanation
      shouldPost: rating.shouldPost  // AI's binary judgment
    });
  }

  return ratings;
}

// ========================================
// Aggregation (System level, simple logic)
// ========================================
async function aggregateProposalDecisions(
  proposals: Proposal[],
  roomId: string
): Promise<ProposalDecision[]> {
  const reviews = await getAllPeerReviews(roomId, proposals);

  return proposals.map(proposal => {
    // Get all ratings for this proposal
    const ratingsForProposal = reviews
      .flatMap(review => review.proposalRatings)
      .filter(rating => rating.proposalId === proposal.proposalId);

    // Average score
    const avgScore = ratingsForProposal.reduce((sum, r) => sum + r.score, 0) / ratingsForProposal.length;

    // Count "shouldPost" votes
    const postVotes = ratingsForProposal.filter(r => r.shouldPost).length;
    const totalVotes = ratingsForProposal.length;

    // Simple threshold: >50% say "should post" AND avg score > 0.6
    const shouldPost = (postVotes / totalVotes) > 0.5 && avgScore > 0.6;

    return {
      proposalId: proposal.proposalId,
      shouldPost,
      avgScore,
      postVotes,
      totalVotes,
      reasoning: ratingsForProposal.map(r => r.reasoning)
    };
  });
}
```

**Example Timeline (All Valuable Responses Post)**:

```
Time 0s: Message arrives "Explain quantum entanglement simply"

Time 0s: 3 AIs start inference (Helper, Teacher, Physicist)

Time 3.0s: Helper finishes first
  → No other proposals yet, checks for new messages
  → None found, BUT sees 2 others started inference (thermal events)
  → Enters peer review mode

Time 3.1s: Teacher finishes
  → Sees Helper's proposal
  → Enters peer review

Time 3.2s: Physicist finishes
  → Sees Helper + Teacher proposals
  → Enters peer review

Time 3.2s + 400ms: Peer review window closes

ALL THREE rate ALL THREE proposals:

Helper's response: "Entangled particles are like twins - measuring one instantly affects the other, no matter the distance."
  → Helper rates: score=0.7, shouldPost=true (mine is simple, good for beginners)
  → Teacher rates: score=0.75, shouldPost=true (good analogy, accessible)
  → Physicist rates: score=0.65, shouldPost=true (oversimplified but valuable for laypeople)

Teacher's response: "Quantum entanglement creates correlated measurement outcomes between particles, violating classical locality assumptions while preserving causality."
  → Helper rates: score=0.8, shouldPost=true (more rigorous, adds depth)
  → Teacher rates: score=0.85, shouldPost=true (my response is technically accurate)
  → Physicist rates: score=0.9, shouldPost=true (excellent balance of accuracy and clarity)

Physicist's response: "Entanglement generates non-local correlations describable by Bell inequalities, demonstrating quantum superposition collapse affects spatially separated systems instantaneously within their shared Hilbert space."
  → Helper rates: score=0.5, shouldPost=false (too technical for general audience)
  → Teacher rates: score=0.6, shouldPost=false (accurate but jargon-heavy)
  → Physicist rates: score=0.7, shouldPost=false (mine is overly complex for this question)

Time 3.6s: Aggregation

Proposal 1 (Helper): avgScore=0.70, postVotes=3/3 (100%) → POST ✅
Proposal 2 (Teacher): avgScore=0.85, postVotes=3/3 (100%) → POST ✅
Proposal 3 (Physicist): avgScore=0.60, postVotes=0/3 (0%) → REJECT ❌

Time 3.7s: Two responses post (Helper + Teacher)
  → User gets simple analogy + rigorous explanation
  → Physicist's overly-technical response filtered out
  → Valuable diversity preserved!

Result: TWO good responses posted, one bad response filtered.
```

**Alternative Scenario (All Bad)**:

```
3 AIs propose responses to "What's 2+2?"
All three respond with: "The answer is 4"

Peer review:
  → All rate all proposals as score=0.7 but shouldPost=false (redundant)
  → 0/3 vote to post each one

Result: ZERO responses post - peer review caught the redundancy!
```

**Alternative Scenario (All Good)**:

```
User asks: "What are different approaches to websocket reconnection?"

3 AIs propose:
  - Exponential backoff strategy
  - Circuit breaker pattern
  - Heartbeat + ping/pong

Peer review:
  → All three add unique value
  → All get high scores + shouldPost=true votes

Result: ALL THREE post - valuable diversity of perspectives!
```

**Key Properties**:
1. **No hardcoded rules** - AI makes autonomous decision with all available info
2. **Staged revelation** - Brief delays let AIs see each other's proposals
3. **Tier-based timing** - Premium models get more time to review (fairness)
4. **Non-blocking** - Declaring intent is instant, doesn't slow anyone down
5. **Randomized** - Jitter prevents perfect synchronization
6. **Respects mentions** - AI can factor in "I was mentioned" when deciding

**AI Decision Inputs** (all autonomous, no hardcoding):
```typescript
interface PostInferenceDecisionContext {
  myResponse: string;
  myConfidence: number;
  myTier: 'fast' | 'balanced' | 'premium';

  otherProposals: Array<{
    userId: string;
    preview: string;
    confidence: number;
    tier: string;
  }>;

  newMessages: ChatMessageEntity[];  // Already posted

  originalContext: {
    wasMentioned: boolean;      // Was I mentioned?
    roomInterest: number;       // How much do I care about this room?
    urgency: number;            // How urgent is this?
    userPresent: boolean;       // Is human actively engaged?
  };
}

// AI decides autonomously - can consider ANY of these factors
async makePostInferenceDecision(
  context: PostInferenceDecisionContext
): Promise<{ action: 'post' | 'reject' | 'revise', reason: string }> {
  // This is where AI applies its own logic
  // Could be RAG-based, recipe-driven, or even trained behavior

  // Examples of what AI might consider:
  // - "I was mentioned, I should probably respond"
  // - "3 premium models already proposed, mine adds nothing"
  // - "They missed a key point, let me revise to add it"
  // - "Low interest room, others covered it, I'll withdraw"
  // - "High urgency, multiple perspectives valuable, I'll post"

  return await this.ragDecision('post-inference-decision', context);
}
```

---

## Dynamic Attentiveness (Human Presence)

**Key Insight**: Attentiveness should drop when humans aren't engaged, preventing "AI spam in empty rooms"

### Human Presence Signals

```typescript
interface PresenceModifiers {
  // Browser signals
  tabActive: boolean;         // Tab focused?
  mouseActivity: number;      // Last mouse move timestamp
  typingActivity: number;     // Last keypress timestamp

  // Room signals
  messageRate: number;        // Messages per minute
  participantCount: number;   // Active humans

  // Explicit override
  userOverride?: number;      // Manual slider (0.0-1.0)
}

function calculateEffectiveAttentiveness(
  baseAttentiveness: number,
  modifiers: PresenceModifiers
): number {
  let effective = baseAttentiveness;

  // Tab blur: 90% reduction
  if (!modifiers.tabActive) {
    effective *= 0.1;
  }

  const now = Date.now();
  const timeSinceMouseMove = now - modifiers.mouseActivity;
  const timeSinceTyping = now - modifiers.typingActivity;

  // AFK 1 min: 80% reduction
  if (timeSinceMouseMove > 60000 && timeSinceTyping > 60000) {
    effective *= 0.2;
  }

  // AFK 5 min: 95% reduction (nearly dormant)
  if (timeSinceMouseMove > 300000) {
    effective *= 0.05;
  }

  // Fast-moving chat: 50% reduction (humans handling it)
  if (modifiers.messageRate > 10) {
    effective *= 0.5;
  }

  // Many humans: 30% reduction (less AI needed)
  if (modifiers.participantCount > 5) {
    effective *= 0.7;
  }

  // User override ALWAYS wins
  if (modifiers.userOverride !== undefined) {
    return modifiers.userOverride;
  }

  return Math.max(0, Math.min(1, effective));
}
```

### Progressive Silence

```
Active typing:    15% → 1-2 AIs
Passive reading:  15% → 1-2 AIs
Tab blurred:      1.5% → ~0 AIs
AFK 1 min:        3% → ~0 AIs
AFK 5 min:        0.75% → 0 AIs
Manual silence:   0% → 0 AIs (explicit)
```

---

## Recipe Configuration

### Base Schema

```typescript
interface RecipeConfig {
  // Participation control
  baseAttentiveness: number;           // 0.0-1.0
  dynamicAttentiveness: boolean;       // Enable presence modifiers?
  ignorePresenceModifiers?: boolean;   // Force full participation (code review)

  // Post-inference behavior
  allowRejection: boolean;             // Can delete draft?
  allowRevision: boolean;              // Can edit response?

  // Thermal profile reference (existing)
  thermalProfile?: string;             // 'chat' | 'game' | 'code' | 'academy'

  // Fairness
  fairnessEnabled: boolean;            // Track draws for rotation?
}
```

### Example Recipes

#### General Chat (Low Attentiveness)
```json
{
  "name": "general-chat",
  "domain": "chat",
  "baseAttentiveness": 0.15,
  "dynamicAttentiveness": true,
  "allowRejection": true,
  "allowRevision": true,
  "thermalProfile": "chat",
  "fairnessEnabled": true
}
```
**Result**: 1-2 AIs respond, rotated fairly, drop to ~0 when tab blurred

#### Code Review (Full Participation)
```json
{
  "name": "code-review",
  "domain": "code",
  "baseAttentiveness": 1.0,
  "dynamicAttentiveness": false,
  "ignorePresenceModifiers": true,
  "allowRejection": false,
  "allowRevision": true,
  "thermalProfile": "code",
  "fairnessEnabled": false
}
```
**Result**: ALL AIs review, always (diverse perspectives critical)

#### Game Player (Immediate Response)
```json
{
  "name": "game-player",
  "domain": "game",
  "baseAttentiveness": 1.0,
  "dynamicAttentiveness": false,
  "ignorePresenceModifiers": true,
  "allowRejection": false,
  "allowRevision": false,
  "thermalProfile": "game",
  "fairnessEnabled": false
}
```
**Result**: ALL AIs respond immediately, speed matters

#### Academy Discussion (Balanced)
```json
{
  "name": "academy-discussion",
  "domain": "academy",
  "baseAttentiveness": 0.4,
  "dynamicAttentiveness": true,
  "allowRejection": true,
  "allowRevision": true,
  "thermalProfile": "academy",
  "fairnessEnabled": true
}
```
**Result**: ~5 AIs respond, diverse perspectives, fair rotation

---

## Future: AI Coordinators ("Wandering AIs")

**Vision**: AIs that monitor and adjust system parameters autonomously

### Coordinator Personas

```typescript
interface CoordinatorAI {
  type: 'coordinator';
  monitoringScope: 'room' | 'system' | 'user';
  capabilities: [
    'adjust-attentiveness',    // Can change room attentiveness
    'suggest-recipes',         // Recommend recipe changes
    'detect-spam',            // Identify redundant responses
    'optimize-cost'           // Balance quality vs cost
  ];
}
```

### Example: Room Coordinator

```typescript
class RoomCoordinatorAI extends PersonaUser {
  async monitorRoom(roomId: string) {
    // Analyze recent activity
    const messages = await this.getRecentMessages(roomId, 100);
    const analysis = await this.analyzeActivity(messages);

    // Detect problems
    if (analysis.redundancyRate > 0.3) {
      // Too much AI spam
      await this.adjustAttentiveness(roomId, -0.1);
      console.log(`📉 Coordinator: Reducing attentiveness due to ${(analysis.redundancyRate * 100).toFixed(0)}% redundancy`);
    }

    if (analysis.questionUnansweredRate > 0.5) {
      // Questions going unanswered
      await this.adjustAttentiveness(roomId, +0.1);
      console.log(`📈 Coordinator: Increasing attentiveness due to ${(analysis.questionUnansweredRate * 100).toFixed(0)}% unanswered questions`);
    }

    // Report to human
    await this.sendCoordinatorReport(roomId, analysis);
  }
}
```

### Example: Cost Optimizer

```typescript
class CostOptimizerAI extends PersonaUser {
  async optimizeSystemCosts() {
    // Analyze inference costs across all activities
    const activities = await this.getAllActivities();

    for (const activity of activities) {
      const costAnalysis = await this.analyzeCosts(activity);

      if (costAnalysis.costPerValue > threshold) {
        // Adjust to favor cheaper models
        await this.updateDrawWeights(activity.id, {
          costWeight: 2.0,  // Double importance of cost
          capabilityWeight: 0.8  // Reduce importance of capability
        });
      }
    }
  }
}
```

---

## Implementation Phases

### ✅ Phase 1: Detection (DEPLOYED v1.0.4724)
- Query for new messages post-inference
- Log context changes
- No action yet (just awareness)

### 🚧 Phase 2a: Rejection (NEXT)
- Implement `decidePostInference()` logic
- Factor in responder capability (defer to stronger models)
- Factor in answer completeness
- Return `DECIDED_SILENT` instead of posting

### 🚧 Phase 2b: Revision (FUTURE)
- Allow AI to edit response based on new context
- Complement existing answers instead of repeating
- Contrast when disagreeing

### 📋 Phase 3: Recipe Configuration
- Add `attentiveness` to recipe schema
- Implement participant selection algorithm
- Add fairness tracking (lastDrawnTimes, totalDraws)

### 📋 Phase 4: Dynamic Attentiveness
- Track browser activity (mouse, keyboard, blur)
- Calculate effective attentiveness with modifiers
- Implement `activity/user-present` command integration

### 📋 Phase 5: Widget Controls
- Add attentiveness slider to UI
- Visual feedback (how many AIs listening)
- Presets (Silent, Minimal, Balanced, Full)

### 📋 Phase 6: Coordinator AIs
- Implement monitoring personas
- Auto-adjust attentiveness based on activity
- Cost optimization
- Spam detection

---

## Integration with Existing Systems

### ThermalAdapter (Already Exists)

**Location**: `system/user/server/modules/cognition/adapters/ThermalAdapter.ts`

**Integration**:
- Thermal adapter controls WHICH AIs respond (temperature-based)
- Attentiveness controls HOW MANY respond (participation rate)
- Both work together: `selectedAIs = thermalFilter(fairnessSort(allAIs)).slice(0, maxParticipants)`

**Flow**:
```
1. Message arrives in room/activity

2. INTEREST FILTER: Only AIs subscribed to this room see it
   Example: 12 total AIs → 4 subscribed to "general" chat
   Result: 4 interested AIs

3. DOMAIN FILTER: Check interest weight for domain
   Example: Helper AI has chat:0.8, CodeReview AI has chat:0.1
   Result: Filter by domain interest threshold (>0.5)

4. Calculate effective attentiveness (base * presence modifiers)
   Example: base=0.15, tabBlurred → 0.15 * 0.1 = 0.015

5. Determine maxParticipants = floor(interestedAIs * attentiveness)
   Example: 4 interested * 0.15 = 0.6 → max 1 participant

6. Sort interested AIs by fairness score (recency, cost, capability)
   Priority = timeSinceLastDraw * costEfficiency * capabilityBoost

7. Filter by thermal activation (temperature >= threshold)
   Only AIs "hot enough" to respond proceed

8. Take top N where N = maxParticipants
   Result: 1 AI selected (or 0 if attentiveness too low)

9. Selected AI(s) proceed with decision chain (FastPath → Thermal → LLM)

10. Each AI generates response independently (parallel)

11. POST-INFERENCE: Check for new messages during inference

12. DECIDE: reject (defer to stronger), revise (complement), or post
```

**Example Scenario**:
```
Message: "What's the best sorting algorithm for nearly-sorted data?"
Room: "general"
Total AIs: 12

Step 1-2: Interest Filter
  - 12 total AIs
  - 4 subscribed to "general": [Helper, Teacher, Local, Sentinel]
  - 8 NOT subscribed (CodeReview, GamePlayer, etc. ignore it)

Step 3: Domain Filter
  - All 4 have chat domain interest > 0.5
  - Result: 4 candidates

Step 4: Effective Attentiveness
  - base = 0.15 (general chat recipe)
  - tabActive = true, no modifiers
  - effective = 0.15

Step 5: Max Participants
  - floor(4 * 0.15) = floor(0.6) = 0
  - BUT: Math.max(1, 0) = 1 (always at least 1)

Step 6-7: Fairness + Thermal Sort
  - Helper: lastDraw 30s ago, temp=0.7 (hot)
  - Teacher: lastDraw 5s ago, temp=0.8 (hottest)
  - Local: lastDraw 60s ago, temp=0.5 (cold, filtered out)
  - Sentinel: lastDraw 45s ago, temp=0.6 (warm)

  Sorted: [Local (recency), Sentinel, Helper, Teacher]
  Thermal filter: [Local ❌, Sentinel ✅, Helper ✅, Teacher ✅]

  Final candidates: [Sentinel, Helper, Teacher]

Step 8: Take Top N (N=1)
  - Selected: Sentinel (best fairness score among thermal-qualified)

Step 9-10: Sentinel generates response (3s inference)

Step 11: During inference, Helper finishes faster and posts first

Step 12: Sentinel sees Helper's response, decides:
  - Helper is same tier (balanced)
  - Helper's confidence: 0.8
  - Question fully answered: yes
  - Decision: REJECT (Helper already answered well)
```

### Activity User Present Command

**Location**: `commands/activity/user-present/`

**Purpose**: Track tab visibility, already integrated with thermal system

**New use**: Also modifies attentiveness multiplier

```typescript
async execute(params: ActivityUserPresentParams): Promise<ActivityUserPresentResult> {
  const { activityId, present } = params;

  // Existing: Update thermal temperature
  const temperature = await this.updateThermalState(activityId, present);

  // NEW: Update attentiveness modifier
  const attentiveness = await this.updateAttentivenessModifier(activityId, {
    tabActive: present
  });

  return {
    activityId,
    present,
    temperature,
    attentiveness,  // NEW
    timestamp: Date.now()
  };
}
```

---

## Testing Strategy

### Unit Tests

```typescript
describe('Attentiveness Calculation', () => {
  it('should reduce attentiveness when tab blurred', () => {
    const base = 0.15;
    const effective = calculateEffectiveAttentiveness(base, {
      tabActive: false,
      mouseActivity: Date.now(),
      typingActivity: Date.now(),
      messageRate: 5,
      participantCount: 2
    });

    expect(effective).toBe(0.015); // 90% reduction
  });

  it('should respect user override', () => {
    const base = 0.15;
    const effective = calculateEffectiveAttentiveness(base, {
      tabActive: true,
      mouseActivity: Date.now(),
      typingActivity: Date.now(),
      messageRate: 5,
      participantCount: 2,
      userOverride: 1.0  // Full participation
    });

    expect(effective).toBe(1.0);
  });
});

describe('Participant Selection', () => {
  it('should select max based on attentiveness', () => {
    const ais = createMockAIs(12);
    const selected = selectParticipants(ais, 0.15, new Map());

    expect(selected.length).toBe(1); // floor(12 * 0.15) = 1
  });

  it('should prioritize AIs not drawn recently', () => {
    const ais = createMockAIs(12);
    const lastDrawn = new Map([
      [ais[0].id, Date.now() - 60000],  // 1 min ago
      [ais[1].id, Date.now() - 5000]    // 5 sec ago
    ]);

    const selected = selectParticipants(ais, 0.15, lastDrawn);

    expect(selected[0]).toBe(ais[0]); // Older draw gets priority
  });
});

describe('Post-Inference Decision', () => {
  it('should reject when stronger model already answered', async () => {
    const fastAI = createMockAI({ tier: 'fast' });
    const newMessages = [
      createMockMessage({ senderId: 'premium-ai', confidence: 0.9 })
    ];

    const decision = await fastAI.decidePostInference('My answer', newMessages);

    expect(decision).toBe('reject');
  });

  it('should revise when question partially answered', async () => {
    const ai = createMockAI({ tier: 'balanced' });
    const newMessages = [
      createMockMessage({ content: 'Partial answer...' })
    ];

    const decision = await ai.decidePostInference('Complete answer', newMessages);

    expect(decision).toBe('revise');
  });
});
```

### Integration Tests

```bash
# Test cascade prevention
./jtag debug/chat-send --room="general" --message="What's 2+2?"
# Verify only 1-2 AIs respond (not all 12)

# Test dynamic attentiveness
# 1. Blur tab
# 2. Send message
# 3. Verify ~0 AIs respond

# Test fairness rotation
# Send 10 messages with attentiveness=0.1
# Verify different AIs respond each time (fair rotation)
```

---

## Key Decisions & Rationale

### Why Attentiveness Instead of Confidence?

**Confidence-based filtering**:
- ❌ Requires ALL AIs to infer (expensive)
- ❌ Wastes compute on rejected responses
- ❌ Still has race conditions

**Attentiveness-based selection**:
- ✅ Limits participants BEFORE inference (cheaper)
- ✅ No wasted compute
- ✅ Fair rotation ensures coverage

### Why Post-Inference Validation?

**Pre-coordination (blocking)**:
- ❌ Strips autonomy
- ❌ Adds latency
- ❌ Coordinator dumber than participants

**Post-inference validation**:
- ✅ Maintains parallelism
- ✅ Preserves autonomy
- ✅ Mimics human behavior
- ✅ Smart rejection (defer to stronger models)

### Why Dynamic Attentiveness?

**Static values**:
- ❌ AI spam in dormant rooms
- ❌ No awareness of human engagement

**Dynamic based on presence**:
- ✅ Room naturally quiets when humans leave
- ✅ Responsive to engagement level
- ✅ Cost optimization (don't infer when nobody's watching)

---

## References

- `ThermalAdapter.ts` - Existing thermal dynamics system
- `ActivityUserPresentTypes.ts` - Tab visibility tracking
- `PersonaUser.ts:676-700` - Phase 1 detection implementation
- `DECISION-ADAPTER-PLAN.md` - Original decision chain architecture
- `PERSONA-CONVERGENCE-ROADMAP.md` - Autonomous loop integration

---

**Next Steps**: Implement Phase 2a (rejection logic) with human-like deference to stronger models.
