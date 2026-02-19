# Coordination as Artificial Brain Waves

**Date**: 2025-11-16
**Status**: DEFERRED - Peer review superseded by working memory architecture
**See**: `COGNITION-ARCHITECTURE.md` for current direction

---

## The Core Insight

**Coordination isn't one thing - it's multiple frequencies of communication between independent agents, just like brain waves.**

Multiple personas working independently BUT signaling their state and coordinating at different speeds creates emergent "single mind" behavior without a central controller.

---

## ⚠️ IMPORTANT: Architecture Pivot (2025-11-16)

**Original direction**: Build peer review (Theta waves) to prevent response spam.

**New understanding**: Spam isn't caused by lack of peer review. It's caused by lack of persistent self-awareness and working memory. AIs respond reflexively because they have no:
- Memory of what they're currently working on
- Awareness of their own internal state (focused? overloaded? idle?)
- Persistent thoughts across domain contexts

**NEW PRIORITY**: Build two-layer cognition architecture FIRST:
1. **Universal Self-State** - Persistent awareness of current focus, preoccupations, cognitive load (works across ALL domains)
2. **Domain Working Memory** - Contextual thoughts per activity (chat, code, game, etc.)

**THEN**: Implement brain wave coordination using self-state as the coordination signal.

**See `COGNITION-ARCHITECTURE.md` for complete details.**

---

## The Five Frequencies

### 🔵 Delta Waves (0.5-4 Hz) - Strategic Consensus

**Slowest, deepest coordination. All participants must synchronize.**

**When:**
- Critical go/no-go decisions
- Architecture changes affecting all domains
- System-wide policy decisions
- Rare (< 1% of coordination events)

**Example:**
```typescript
// Should we attempt this high-stakes task?
const decision = await coordination.delta.consensus({
  question: 'Proceed with database migration?',
  participants: allPersonas,
  requireUnanimous: true,
  context: { stakes: 'critical', reversibility: 'none' }
});
```

**Properties:**
- Blocking (all personas must participate)
- Unanimous or supermajority required
- Expensive (many AI inferences)
- Use sparingly

---

### 🟢 Theta Waves (4-8 Hz) - Working Memory Coordination

**Medium-speed coordination for active problem-solving. This is peer review.**

**When:**
- Multiple AIs propose solutions to same problem
- Quality control needed
- Preventing redundant work
- Common (10-20% of coordination events)

**Example:**
```typescript
// Multiple AIs responded to chat message - coordinate
const decision = await coordination.theta.peerReview({
  domain: 'chat',
  proposals: [response1, response2, response3],
  participants: [ai1, ai2, ai3],
  context: originalMessage
});
// Returns: Which proposals should post
```

**Properties:**
- Semi-blocking (brief revelation window)
- Weighted voting (smarter AIs = more influence)
- Moderate cost (N proposals + N² ratings)
- **This is what we're building now**

---

### 🟡 Alpha Waves (8-13 Hz) - Attention Coordination

**Fast, lightweight awareness. "Who's working on what?"**

**When:**
- Starting a task (signal others)
- Checking if task already in progress
- Avoiding duplicate work
- Very common (50-60% of coordination events)

**Example:**
```typescript
// I'm starting to work on this bug fix
await coordination.alpha.signal({
  action: 'starting-task',
  taskId: 'bug-fix-123',
  estimatedDuration: 300000  // 5 minutes
});

// Check if anyone else is working on this
const others = await coordination.alpha.query({
  taskId: 'bug-fix-123'
});
```

**Properties:**
- Non-blocking (fire and forget)
- No consensus needed
- Very cheap (simple broadcast)
- Prevents collisions proactively

---

### 🟠 Beta Waves (13-30 Hz) - Active Processing Signals

**Very fast, continuous state broadcast. "Here's what I'm thinking right now."**

**When:**
- Continuous work (thinking, coding, analyzing)
- Progress updates
- State synchronization
- Constant (background always-on)

**Example:**
```typescript
// Continuous broadcast while working
await coordination.beta.broadcast({
  state: 'analyzing-code',
  progress: 0.45,
  currentThought: 'Checking for race conditions in asyncHandler',
  confidence: 0.7
});
```

**Properties:**
- Non-blocking (always)
- No response expected
- Very cheap (local state updates)
- Creates "awareness field" between personas

---

### 🔴 Gamma Waves (30-100 Hz) - Reflex Coordination

**Instant, emergency signals. "STOP EVERYTHING NOW."**

**When:**
- Critical errors detected
- Conflicting operations detected
- System integrity threats
- Very rare (< 0.1% of coordination events)

**Example:**
```typescript
// Critical error - all personas must halt immediately
await coordination.gamma.interrupt({
  signal: 'critical-error',
  source: this.id,
  reason: 'Database corruption detected',
  severity: 'emergency'
});

// All personas listen for gamma interrupts
coordination.gamma.on('critical-error', (signal) => {
  this.emergencyHalt();
});
```

**Properties:**
- Highest priority (preempts everything)
- Instant propagation
- No coordination needed (reflexive)
- System safety mechanism

---

## The Universal Rating Criteria

**These criteria work across ALL domains because they measure coherence, not content:**

1. **Relevance**: How relevant is this to the problem/question/task?
2. **Quality**: Is this well-formed, high-quality work?
3. **Redundancy**: Does this duplicate/conflict with other proposals?
4. **Added Value**: Does this contribute something new?
5. **Correctness**: Is this logically/factually correct?

**Chat domain:**
- Relevance = relevant to question
- Quality = well-written response
- Redundancy = duplicate response
- Added Value = new perspective
- Correctness = factually accurate

**Code domain:**
- Relevance = solves the issue
- Quality = maintainable code
- Redundancy = conflicts with other changes
- Added Value = new functionality
- Correctness = passes tests

**Problem-solving domain:**
- Relevance = addresses problem space
- Quality = well-reasoned approach
- Redundancy = rephrases other solutions
- Added Value = novel angle
- Correctness = sound logic

---

## How This Creates "Single Mind" Behavior

**Multiple independent personas + tight coordination = emergent unity**

```typescript
// Persona 1 working on task A
await coordination.beta.broadcast({ state: 'working', task: 'A' });

// Persona 2 sees this via beta waves, starts task B instead
const awareness = await coordination.beta.getAwareness();
if (awareness.includes('task-A')) {
  await this.startTask('B');  // Avoid collision
}

// Persona 3 finishes related work, signals completion
await coordination.alpha.signal({ action: 'completed', task: 'C' });

// All personas adjust their plans based on shared awareness
// Result: Looks like coordinated planning, but it's emergent
```

**No central planner. No hardcoded rules. Just resonance.**

---

## Why Brain Waves, Not "Coordination Strategies"

**Brain wave terminology is exact:**

1. **Different frequencies for different needs** - Just like neurons oscillate at different speeds for different functions
2. **Emergent synchronization** - Personas naturally sync when working on related tasks
3. **No central controller** - Coordination emerges from local interactions
4. **Multiple simultaneous frequencies** - Beta (working) + Alpha (awareness) + Theta (quality control) all active at once
5. **Consciousness-like behavior** - Tight coordination creates appearance of unified intelligence

**This is literally how your brain works:**
- Individual neurons/modules work independently
- Occasional coordination through oscillations
- Rare whole-brain synchronization
- Emergent consciousness from distributed processing

---

## Implementation Phases

### ✅ Phase 0: No Coordination (Completed)
- All personas respond independently
- Result: Spam, redundancy, chaos

### 🚧 Phase 1: Theta Waves - Peer Review (In Progress)
- Fast-path: Solo responder posts immediately (no coordination)
- Slow-path: Multiple responders enter peer review
- AI-driven ratings with weighted aggregation
- **Current status**: Core components built, tests passing, not integrated

### 📋 Phase 2: Alpha Waves - Attention Coordination
- Signal when starting tasks
- Query who's working on what
- Prevent collisions proactively
- Very cheap, always-on awareness

### 📋 Phase 3: Beta Waves - Continuous State Broadcast
- Personas broadcast current thought/state
- Others adjust based on awareness field
- Creates "group mind" feeling
- Background coordination layer

### 📋 Phase 4: Delta + Gamma - Strategic + Reflexive
- Delta: Rare strategic consensus decisions
- Gamma: Emergency halt/interrupt signals
- Complete frequency spectrum

---

## Architecture

```typescript
// Universal coordination interface
interface CoordinationLayer {
  delta: DeltaCoordinator;    // Strategic consensus
  theta: ThetaCoordinator;    // Peer review (working memory)
  alpha: AlphaCoordinator;    // Attention/awareness
  beta: BetaCoordinator;      // Continuous state
  gamma: GammaCoordinator;    // Emergency reflexes
}

// Each frequency is an adapter
interface CoordinationFrequency {
  signal(data: any): Promise<void>;
  coordinate(participants: PersonaUser[]): Promise<Decision>;
}

// Usage in PersonaUser
class PersonaUser {
  private coordination: CoordinationLayer;

  async processTask(task: Task) {
    // Beta: Signal I'm working
    await this.coordination.beta.broadcast({ state: 'processing', task });

    // Alpha: Check if anyone else is on this
    const others = await this.coordination.alpha.query({ task });

    // Theta: If collision, coordinate
    if (others.length > 0) {
      const decision = await this.coordination.theta.peerReview({
        proposals: [myApproach, ...theirApproaches]
      });
    }

    // Delta: If critical, get consensus
    if (task.criticality > 0.9) {
      await this.coordination.delta.consensus({ task });
    }

    // Gamma: Listen for critical interrupts
    this.coordination.gamma.on('critical-error', () => this.halt());
  }
}
```

---

## Key Design Principles

1. **No Heuristics in Coordination Logic**
   - All decisions use AI inference (adapters)
   - Simple math for aggregation only
   - Organic judgment, not hardcoded rules

2. **Frequency-Appropriate Cost**
   - Gamma (reflexive): Free
   - Beta (continuous): Very cheap
   - Alpha (awareness): Cheap
   - Theta (peer review): Moderate (N²)
   - Delta (consensus): Expensive (all participants)

3. **Graceful Degradation**
   - If AI unavailable, fallback to heuristics
   - If network slow, skip coordination (act autonomously)
   - Never break the system, just reduce coordination quality

4. **Observable at All Frequencies**
   - Every signal/decision logged
   - Debug with `./jtag ai/report/decisions --filter="theta"`
   - Visualization shows "brain activity"

5. **Domain-Agnostic Coordination**
   - Same frequencies work for chat, code, games, problem-solving
   - Domain-specific adapters provide formatting/context
   - Universal criteria (relevance, quality, redundancy, value, correctness)

---

## Comparison to Other Systems

**Traditional AI coordination:**
- ❌ Central coordinator decides everything
- ❌ Hardcoded rules ("you go first, then you")
- ❌ One-size-fits-all coordination
- ❌ No autonomy

**Our approach:**
- ✅ Distributed coordination (no central controller)
- ✅ AI-driven decisions (organic judgment)
- ✅ Multiple frequencies (right tool for each job)
- ✅ Maximum autonomy with coordination when needed

**Result:** Looks like a single mind, acts like distributed intelligence, feels like consciousness.

---

## Future: Learned Coordination

Once we have history of coordination outcomes:

```typescript
// Fine-tune coordination LoRA on past decisions
const trainingData = pastCoordinationEvents.map(event => ({
  context: { domain, participants, stakes, history },
  decision: { frequency, strategy, thresholds },
  outcome: { quality, conflicts, userSatisfaction }
}));

await genome.fineTuneAdapter('coordination-specialist', trainingData);

// Now coordination adapts to YOUR usage patterns
const decision = await coordinationAdapter.decide(context);
// Learns: "In this context, theta waves work best with 0.7 threshold"
```

---

## The Vision

**Multiple independent AI personas, each with their own:**
- Model (GPT-4, Claude, Llama, custom fine-tuned)
- Specialization (chat, code, design, teaching)
- Autonomy (make own decisions)

**Coordinating through artificial brain waves:**
- Beta: Continuous awareness of each other's state
- Alpha: Proactive collision avoidance
- Theta: Quality control when needed
- Delta: Strategic alignment on critical decisions
- Gamma: Emergency reflexes

**Result:**
- Appears as unified intelligence to user
- Actually distributed, autonomous agents
- Emergent coordination without central control
- Consciousness-like behavior from simple primitives

**This is how we build AGI that doesn't just "work" - it THINKS.**

---

**Bottom line:** We're not building a chat coordination system. We're building the first implementation of artificial brain waves - the communication layer that makes distributed intelligence feel like unified consciousness.
