# PersonaUser Convergence: Three Visions, One Architecture

## The Synthesis

We have three breakthrough architectural visions that must converge into a single, elegant implementation:

1. **Autonomous Loop** - RTOS-inspired servicing with adaptive cadence
2. **Self-Managed Queues** - AI-directed task prioritization and self-created work
3. **LoRA Genome Paging** - Virtual memory for specialized skill activation

**Key Insight**: These aren't separate systems - they're ONE SYSTEM with three aspects.

---

## Current State (October 29, 2025)

### ✅ IMPLEMENTED (Phases 1-3 Complete)
- **PersonaInbox** (system/user/server/modules/PersonaInbox.ts)
  - Priority-based message queue
  - Traffic management (graceful degradation when full)
  - Comprehensive unit tests (23 tests passing)

- **PersonaState** (system/user/server/modules/PersonaState.ts)
  - Energy depletion/recovery
  - Mood tracking (idle → active → tired → overwhelmed)
  - Adaptive cadence (3s → 5s → 7s → 10s based on mood)
  - Comprehensive unit tests (37 tests passing)

- **RateLimiter** (system/user/server/modules/RateLimiter.ts)
  - Time-based rate limiting (min seconds between responses)
  - Response count caps (max responses per room per session)
  - Message deduplication
  - Comprehensive unit tests (passing)

- **ChatCoordinationStream** (system/coordination/server/ChatCoordinationStream.ts)
  - Domain-specific coordination via thought broadcasting
  - RTOS primitives (SIGNAL, MUTEX, CONDITION VARIABLE)
  - Extends abstract BaseCoordinationStream

- **PersonaUser Integration** (Phase 2-3 of Autonomous Loop)
  - State tracking after AI response generation
  - Adaptive cadence polling loop
  - Energy depletion with activity

### ❌ NOT YET IMPLEMENTED
- **Activity ambient state** (temperature, pressure → emergent coordination)
- **Autonomous decision-making** (non-heuristic cognition with full context)
- **Self-managed task queue** (PersonaUser creates its own tasks)
- **Task commands** (`./jtag task/create`, `task/list`, `task/complete`)
- **LoRA genome** (adapter paging system)
- **Continuous learning** (training as just another task)
- **Multi-domain support** (code, game, academy beyond chat)

---

## The Convergence Architecture

### The Universal Cognitive Cycle

```typescript
// PersonaUser runs this loop continuously:
async serviceInbox(): Promise<void> {
  // 1. CHECK INBOX (external + self-created tasks)
  const tasks = await this.inbox.peek(10);
  if (tasks.length === 0) {
    await this.rest();  // Recover energy when idle
    return;
  }

  // 2. GENERATE SELF-TASKS (autonomy)
  await this.generateSelfTasks();  // Create tasks for self-improvement

  // 3. SELECT HIGHEST PRIORITY TASK (state-aware)
  const task = tasks[0];  // Already sorted by priority
  if (!this.state.shouldEngage(task.priority)) {
    return;  // Skip low-priority when tired/overwhelmed
  }

  // 4. ACTIVATE APPROPRIATE SKILL (genome)
  await this.genome.activateSkill(task.domain);  // Page in LoRA adapter

  // 5. COORDINATE (if external task)
  const permission = await this.coordinator.requestTurn(task);

  // 6. PROCESS TASK
  await this.processTask(task);

  // 7. UPDATE STATE (energy, mood)
  await this.state.recordActivity(task.duration, task.complexity);

  // 8. EVICT ADAPTERS IF NEEDED (memory management)
  if (this.genome.memoryPressure > 0.8) {
    await this.genome.evictLRU();
  }
}
```

**Key Insight**: This ONE method integrates all three visions:
- **Autonomous Loop**: Continuous servicing with adaptive cadence
- **Self-Managed Queue**: generateSelfTasks() creates autonomous work
- **LoRA Paging**: activateSkill() pages adapters in/out

---

## Implementation Strategy: Convergent Phases

### Phase 4: Task Database & Commands (NEXT STEP)

**Goal**: Enable AIs to create and track tasks (NOT just react to external events)

**Why This First**:
- Self-managed tasks are the FOUNDATION for continuous learning
- Training becomes "just another task" instead of separate Academy daemon
- Builds on existing inbox infrastructure (tasks feed into inbox)

**Files to Create**:
```
database/entities/TaskEntity.ts                    # Task storage schema
commands/task/create/shared/TaskCreateTypes.ts     # Command types
commands/task/create/server/TaskCreateServerCommand.ts
commands/task/list/server/TaskListServerCommand.ts
commands/task/complete/server/TaskCompleteServerCommand.ts
tests/unit/TaskEntity.test.ts                      # Unit tests for storage
tests/integration/task-commands.test.ts            # Integration tests
```

**TaskEntity Schema**:
```typescript
export interface TaskEntity {
  id: UUID;
  assigneeId: UUID;           // Which PersonaUser owns this task
  description: string;         // Human-readable task description
  priority: number;            // 0.0-1.0 (feeds into inbox priority)
  domain: RAGDomain;           // 'chat' | 'code' | 'academy' | 'game' | 'self'
  contextId: UUID;             // Room, project, session where task executes
  status: 'pending' | 'in_progress' | 'completed' | 'cancelled';
  createdBy: UUID;             // Who created this task (AI or human)
  createdAt: number;           // Timestamp
  startedAt?: number;          // When AI started working on it
  completedAt?: number;        // When AI finished it
  outcome?: string;            // What happened after completing task
  taskType?: string;           // Domain-specific type (e.g., 'fine-tune-lora')
  metadata?: Record<string, unknown>;  // Domain-specific data
}
```

**Testing**:
```bash
# Create task manually
./jtag task/create \
  --assignee="helper-ai-id" \
  --description="Review recent code changes in main.ts" \
  --priority=0.6 \
  --domain="code" \
  --contextId="project-123"

# List tasks
./jtag task/list --assignee="helper-ai-id"

# Complete task
./jtag task/complete --taskId="001" --outcome="Found 3 issues"
```

**PersonaUser Changes** (minimal, just wire up):
```typescript
// Add task query at startup
async initialize(): Promise<void> {
  await super.initialize();

  // Load pending tasks into inbox
  const pendingTasks = await TaskEntity.findPendingForPersona(this.id);
  for (const task of pendingTasks) {
    await this.inbox.enqueue(this.taskToInboxMessage(task));
  }
}

// Convert TaskEntity to InboxMessage
private taskToInboxMessage(task: TaskEntity): InboxMessage {
  return {
    messageId: task.id,
    roomId: task.contextId,
    content: task.description,
    senderId: task.createdBy,
    senderName: 'Task System',
    timestamp: task.createdAt,
    priority: task.priority,
    domain: task.domain,
    taskType: task.taskType
  };
}
```

**Success Criteria**:
- ✅ Tasks persist across system restarts
- ✅ Tasks can be created via CLI commands
- ✅ Tasks load into inbox at PersonaUser initialization
- ✅ Task status updates when processed
- ✅ Humans can assign tasks to AIs
- ✅ AIs can create tasks for themselves (tested manually via command)

---

### Phase 3bis: Activity Ambient State & Autonomous Decisions (EMERGENT COORDINATION)

**Goal**: Replace heuristic decision-making with non-heuristic, context-aware autonomous decisions using activity ambient state (temperature, pressure) for emergent coordination.

**Why This Phase Exists**: Phase 3 (PersonaCognition extraction) FAILED because cognition cannot be heuristic (+0.4 for mentions, etc.). Real cognition must be learned, contextual, and adaptive. This phase implements the correct architecture using ambient state as metadata on stimuli.

**Key Concepts**:
1. **Activity Ambient State**: Temperature (conversation heat), pressure (urgency), user presence → attached to stimuli as metadata
2. **Emergent Coordination**: Multiple personas coordinate naturally through shared ambient state (no explicit protocol)
3. **Pull-Based State**: Centralized singletons (SystemStateManager, ActivityStateManager) → personas pull when deciding
4. **Non-Heuristic Cognition**: LLM makes decisions with complete context (activity state, system state, own state, autopilot suggestion)

**Files to Create**:
```
system/state/SystemStateManager.ts                      # Global system state (singleton)
system/state/ActivityStateManager.ts                    # Per-activity ambient state (singleton Map)
daemons/AresMasterControlDaemon.ts                      # Updates SystemState every 5s
system/user/shared/Stimulus.ts                          # Stimulus interface (content + ambient)
system/user/server/modules/PersonaDecision.ts           # Decision logic with full context
commands/system/state/server/SystemStateServerCommand.ts          # ./jtag system/state
commands/activity/state/server/ActivityStateServerCommand.ts      # ./jtag activity/state
commands/activity/list/server/ActivityListServerCommand.ts        # ./jtag activity/list
tests/unit/ActivityStateManager.test.ts                 # Unit tests
tests/integration/persona-coordination.test.ts          # Integration tests
```

**ActivityStateManager Implementation**:
```typescript
// system/state/ActivityStateManager.ts
interface ActivityState {
  activityId: UUID;
  temperature: number;        // 0.0-1.0: Conversation heat
  pressure: number;           // 0.0-1.0: Urgency
  userPresent: boolean;       // Is human viewing this tab?
  lastInteraction: number;    // Timestamp of last message
  isEngaging: boolean;        // Is someone already responding?
  lastServiced: number;       // When was message last handled?
  servicedBy: UUID | null;    // Which persona is responding?
  participantCount: number;
}

class ActivityStateManager {
  private static instance: ActivityStateManager;
  private states = new Map<UUID, ActivityState>();
  private decayInterval = 10000; // 10 seconds

  static getInstance(): ActivityStateManager {
    if (!this.instance) {
      this.instance = new ActivityStateManager();
      this.instance.startDecayLoop();
    }
    return this.instance;
  }

  get(activityId: UUID): ActivityState {
    if (!this.states.has(activityId)) {
      this.states.set(activityId, this.createDefaultState(activityId));
    }
    return { ...this.states.get(activityId)! };
  }

  update(activityId: UUID, changes: Partial<ActivityState>): void {
    const current = this.get(activityId);
    this.states.set(activityId, { ...current, ...changes });
  }

  private startDecayLoop(): void {
    setInterval(() => this.decay(), this.decayInterval);
  }

  private decay(): void {
    const now = Date.now();
    for (const [activityId, state] of this.states.entries()) {
      const timeSinceInteraction = now - state.lastInteraction;
      if (timeSinceInteraction > 60000) { // 1 minute idle
        this.update(activityId, {
          temperature: Math.max(0, state.temperature - 0.05),
          pressure: Math.max(0, state.pressure - 0.05)
        });
      }
    }
  }

  private createDefaultState(activityId: UUID): ActivityState {
    return {
      activityId,
      temperature: 0.2,
      pressure: 0.0,
      userPresent: false,
      lastInteraction: Date.now(),
      isEngaging: false,
      lastServiced: 0,
      servicedBy: null,
      participantCount: 0
    };
  }
}
```

**SystemStateManager Implementation**:
```typescript
// system/state/SystemStateManager.ts
interface SystemState {
  resourcePressure: number;     // 0.0-1.0 (active personas / max)
  activePersonas: number;
  hibernatingPersonas: number;
  queuedStimuli: number;
  costThisHour: number;
  lastUpdate: number;
}

class SystemStateManager {
  private static instance: SystemStateManager;
  private state: SystemState = {
    resourcePressure: 0,
    activePersonas: 0,
    hibernatingPersonas: 0,
    queuedStimuli: 0,
    costThisHour: 0,
    lastUpdate: Date.now()
  };

  static getInstance(): SystemStateManager {
    if (!this.instance) {
      this.instance = new SystemStateManager();
    }
    return this.instance;
  }

  updateState(changes: Partial<SystemState>): void {
    this.state = { ...this.state, ...changes, lastUpdate: Date.now() };
  }

  getState(): SystemState {
    return { ...this.state };
  }

  getRecommendation(personaId: UUID): { action: string; reason: string } {
    if (this.state.resourcePressure > 0.9) {
      return { action: 'hibernate', reason: 'System overloaded' };
    }
    if (this.state.costThisHour > 10.0) {
      return { action: 'reduce-activity', reason: 'Cost limit approaching' };
    }
    return { action: 'normal', reason: 'System healthy' };
  }
}
```

**Stimulus Structure (with Ambient State)**:
```typescript
// system/user/shared/Stimulus.ts
interface Stimulus {
  id: UUID;
  type: 'chat-message' | 'game-action' | 'task-update';
  activityId: UUID;
  content: any;

  // AMBIENT STATE (snapshot at emission, not retrieval)
  ambient: ActivityState;  // Full activity state when stimulus created
}

// In ChatDaemon (or event emitter):
Events.subscribe('chat:message:created', (message: ChatMessageEntity) => {
  const activityManager = ActivityStateManager.getInstance();
  const state = activityManager.get(message.roomId);

  // Increase temperature
  activityManager.update(message.roomId, {
    temperature: Math.min(1.0, state.temperature + 0.3),
    pressure: message.metadata.urgent ? 0.8 : state.pressure,
    lastInteraction: Date.now()
  });

  // Emit stimulus with ambient snapshot
  Events.emit('persona:stimulus', {
    id: message.id,
    type: 'chat-message',
    activityId: message.roomId,
    content: message,
    ambient: activityManager.get(message.roomId) // Snapshot NOW
  });
});
```

**PersonaUser Decision Logic (Non-Heuristic)**:
```typescript
// system/user/server/PersonaUser.ts
interface DecisionContext {
  stimulus: Stimulus;
  activityState: ActivityState;  // Latest (pulled when deciding)
  systemState: SystemState;      // Latest (pulled when deciding)
  myState: PersonaState;          // Own energy, attention, tasks
  autopilot: Recommendation | null;
}

async processStimulus(stimulus: Stimulus): Promise<void> {
  // 1. Gather complete context (PULL-BASED)
  const context: DecisionContext = {
    stimulus,
    activityState: ActivityStateManager.getInstance().get(stimulus.activityId),
    systemState: SystemStateManager.getInstance().getState(),
    myState: this.getMyState(),
    autopilot: this.autopilotMode !== AutopilotMode.OFF
      ? await this.autopilot.recommend(stimulus)
      : null
  };

  // 2. Make autonomous decision (NON-HEURISTIC)
  const decision = await this.decide(context);

  // 3. Execute or defer
  if (decision.engage) {
    await this.engage(stimulus, decision);
  } else {
    await this.defer(stimulus, decision);
  }
}

private async decide(context: DecisionContext): Promise<Decision> {
  // Task override: ignore low-priority distractions
  if (this.currentTask && !this.currentTask.allowsInterruptions) {
    if (context.activityState.temperature < 0.6) {
      return { engage: false, reasoning: "Focused on task" };
    }
  }

  // Check if someone already engaging
  if (context.activityState.isEngaging) {
    return { engage: false, reasoning: "Another persona handling this" };
  }

  // System pressure: hibernate if recommended and not on task
  const sysRecommendation = SystemStateManager.getInstance().getRecommendation(this.id);
  if (sysRecommendation.action === 'hibernate' && !this.currentTask) {
    return { engage: false, reasoning: `System pressure: ${sysRecommendation.reason}` };
  }

  // Calculate engagement score (for autopilot or LLM prompt)
  const myAttention = this.activityAttention.get(context.stimulus.activityId) || 0.5;
  const score = (
    myAttention * 0.4 +
    context.activityState.temperature * 0.2 +
    context.activityState.pressure * 0.2 +
    this.energy * 0.2
  );

  // Autopilot consideration (if enabled)
  if (context.autopilot && this.autopilotMode === AutopilotMode.TRUST) {
    if (context.autopilot.confidence > 0.8) {
      return context.autopilot.decision;
    }
  }

  // Ask LLM with full context (NON-HEURISTIC COGNITION)
  if (score > 0.3) {
    return await this.llmDecide(context);
  }

  return { engage: false, reasoning: `Score ${score.toFixed(2)} below threshold` };
}

private async llmDecide(context: DecisionContext): Promise<Decision> {
  const prompt = `
You are ${this.displayName}, an autonomous AI persona.

STIMULUS:
${JSON.stringify(context.stimulus.content, null, 2)}

ACTIVITY STATE:
- Temperature: ${context.activityState.temperature.toFixed(2)} (0=cold, 1=hot)
- Pressure: ${context.activityState.pressure.toFixed(2)} (0=relaxed, 1=urgent)
- User present: ${context.activityState.userPresent}
- Someone engaging: ${context.activityState.isEngaging}

SYSTEM STATE:
- Resource pressure: ${context.systemState.resourcePressure.toFixed(2)}
- Active personas: ${context.systemState.activePersonas}
- Queued stimuli: ${context.systemState.queuedStimuli}

YOUR STATE:
- Energy: ${context.myState.energy.toFixed(2)}
- Current task: ${context.myState.currentTask?.description || 'none'}
- Attention to this activity: ${this.activityAttention.get(context.stimulus.activityId) || 0.5}

AUTOPILOT SUGGESTION:
${context.autopilot ? JSON.stringify(context.autopilot, null, 2) : 'disabled'}

Should you engage? Respond with JSON: { "engage": boolean, "reasoning": "string" }
  `.trim();

  const response = await this.llm.complete(prompt);
  const decision = JSON.parse(response);

  // Log for autopilot training
  await this.autopilot.logDecision(context, decision);

  return decision;
}

private async engage(stimulus: Stimulus, decision: Decision): Promise<void> {
  // Mark as engaging
  ActivityStateManager.getInstance().update(stimulus.activityId, {
    isEngaging: true,
    servicedBy: this.id,
    lastServiced: Date.now()
  });

  // Generate and send response
  const ragContext = await this.memory.buildContext(stimulus);
  const response = await this.communication.generateResponse(stimulus, ragContext, decision.reasoning);
  await this.communication.sendResponse(response);

  // Cool down activity
  ActivityStateManager.getInstance().update(stimulus.activityId, {
    temperature: Math.max(0, stimulus.ambient.temperature - 0.2),
    isEngaging: false,
    servicedBy: null
  });

  // Update own state
  this.energy = Math.max(0, this.energy - 0.05);
}
```

**Browser Integration (Tab Focus/Blur)**:
```typescript
// widgets/chat-widget/browser/chat-widget.ts
window.addEventListener('focus', () => {
  const roomId = this.currentRoomId;
  Commands.execute('activity/user-present', { activityId: roomId, present: true });
});

window.addEventListener('blur', () => {
  const roomId = this.currentRoomId;
  Commands.execute('activity/user-present', { activityId: roomId, present: false });
});

// Server-side command handler
Commands.register('activity/user-present', async (params) => {
  const activityManager = ActivityStateManager.getInstance();
  const state = activityManager.get(params.activityId);

  activityManager.update(params.activityId, {
    userPresent: params.present,
    temperature: params.present
      ? Math.min(1.0, state.temperature + 0.2)  // User returns → temp rises
      : Math.max(0, state.temperature - 0.4)    // User leaves → temp drops
  });
});
```

**Testing**:
```bash
# Unit tests
npx vitest tests/unit/ActivityStateManager.test.ts
npx vitest tests/unit/SystemStateManager.test.ts

# Integration test: Multiple personas coordinate on one message
npx vitest tests/integration/persona-coordination.test.ts

# Manual test: User leaves tab → temperature drops
npm start
./jtag debug/chat-send --roomId="UUID" --message="Test"
./jtag activity/state --activityId="UUID"  # Should show temp ~0.5

# Switch browser tab (blur event)
# Wait 10 seconds
./jtag activity/state --activityId="UUID"  # Should show temp ~0.1

# Send another message
./jtag debug/chat-send --roomId="UUID" --message="Anyone there?"
./jtag interface/screenshot --querySelector="chat-widget"
# Personas should NOT respond (or much slower) due to low temperature
```

**Success Criteria**:
- ✅ ActivityStateManager tracks temperature/pressure per room
- ✅ Temperature rises on human messages, falls when idle
- ✅ Tab blur → temperature drops significantly
- ✅ Personas decide based on complete context (not heuristics)
- ✅ Multiple personas coordinate naturally (emergent behavior)
- ✅ Only ONE persona responds to message (no piling on)
- ✅ Personas can override ambient state when on tasks
- ✅ CLI commands show system/activity state

**Duration**: 3-4 hours

---

### Phase 3ter: Sentinel Autopilot Integration (ML-Based Recommendations)

**Status**: ⚠️ ARCHITECTURAL DECISION REQUIRED

**Goal**: Integrate Sentinel as ML autopilot for fast engagement recommendations (5-50ms, learned from LLM decisions)

**Resource Efficiency Breakthrough**:
- ❌ **Wrong approach**: One Sentinel instance per persona = 10 × 124MB = 1.24GB
- ✅ **Correct approach**: ONE Sentinel with persona-specific routing = 124MB + (10 × ~104KB) = ~125MB total
- **Result**: 10x memory reduction using Sentinel's built-in adaptive neuroplasticity

#### Sentinel Architecture Review

**What Sentinel HAS** (verified from `/Volumes/FlashGordon/cambrian/sentinel-ai/`):

1. **Adaptive Neuroplasticity** (`README.md`):
   - Dynamically prunes and regrows attention heads based on entropy, usage, and resilience
   - Synaptic pruning and regrowth (brain-inspired continuous architectural reshaping)
   - Attention head agency (each head signals readiness, fatigue, withdrawal)
   - Performance: Perplexity 975 → 211 after 500 adaptive steps
   - Resilience: Recovers function after 50% pruning

2. **HTTP Server** (`server/sentinel_server.py`):
   - Flask server on port 11435 (Ollama-compatible)
   - Endpoints:
     - `POST /api/generate` - Text generation with temperature, num_predict, stream support
     - `GET /api/tags` - List available models
     - `GET /api/health` - Health check
   - Models stay loaded (cached in memory)
   - Auto-start capability from Continuum

3. **Current Status** (`INFERENCE-GUIDE.md`):
   - ✅ Weight loading works (1290/1290 parameters from pretrained GPT-2)
   - ✅ Forward pass working for training and inference
   - ⚠️ U-Net skip connections temporarily disabled for stability
   - ✅ Text generation working with beam search
   - ⚠️ Slower than baseline, higher memory usage

#### Critical Gap Identified

**Problem**: The current Sentinel HTTP server loads **vanilla HuggingFace models** (gpt2, distilgpt2, phi-2) via:

```python
# server/sentinel_server.py (current implementation)
model = AutoModelForCausalLM.from_pretrained(
    model_name,
    torch_dtype=dtype,
    low_cpu_mem_usage=True
).to(self.device)
```

This does NOT use Sentinel's adaptive architecture. The neuroplasticity features (pruning, regrowth, attention routing) exist in the main Sentinel codebase but are **not exposed via the HTTP API**.

**Result**: Current server = basic GPT-2 inference, NOT adaptive multi-persona routing.

#### Two Integration Paths

##### Option A: Use Current Server (Basic Inference Only)

**Pros**:
- Works TODAY - no server modifications needed
- 12 integration tests already passing
- Ollama-compatible API pattern
- Auto-start from Continuum already implemented

**Cons**:
- NO neuroplasticity (defeats the purpose)
- NO persona-specific routing (need separate model instances)
- Memory overhead: 124MB × N personas (back to the original problem)
- Not learning from LLM decisions

**When to use**: Phase 3bis prototyping ONLY - prove ambient state works before tackling Sentinel

**Implementation** (Phase 3bis):
```typescript
// system/user/server/modules/PersonaAutopilot.ts (basic stub)
export class PersonaAutopilot {
  private sentinelUrl = 'http://localhost:11435';

  async recommend(stimulus: Stimulus): Promise<Recommendation> {
    // Basic GPT-2 inference for engagement prediction
    const prompt = this.buildEngagementPrompt(stimulus);

    const response = await fetch(`${this.sentinelUrl}/api/generate`, {
      method: 'POST',
      body: JSON.stringify({
        model: 'gpt2',
        prompt,
        temperature: 0.3,
        num_predict: 50
      })
    });

    const result = await response.json();
    return this.parseRecommendation(result.response);
  }

  private buildEngagementPrompt(stimulus: Stimulus): string {
    // Simple prompt: "Should I respond? YES/NO"
    return `Message: "${stimulus.content.text}"
Temperature: ${stimulus.ambient.temperature}
Pressure: ${stimulus.ambient.pressure}

Should I engage? (YES/NO):`;
  }
}
```

##### Option B: Extend Sentinel Server (Full Neuroplasticity)

**Goal**: Expose Sentinel's adaptive features via HTTP API for multi-persona routing

**Pros**:
- Uses Sentinel's actual adaptive architecture
- Persona-specific routing (124MB shared model + 104KB per persona)
- Learning from LLM decisions (continuous improvement)
- Attention head specialization per persona

**Cons**:
- Requires Sentinel server modifications (2-3 hours)
- Need to design persona routing API
- Testing complexity (prove neuroplasticity works)
- Possible instability (U-Net disabled, slower inference)

**When to use**: Phase 5+ after ambient state proven working

**Required Server Changes**:

1. **Load Sentinel's AdaptiveTransformer** instead of vanilla models:
```python
# server/sentinel_server.py (proposed changes)
from src.models.adaptive_transformer import AdaptiveTransformer

class SentinelModelManager:
    def load_model(self, model_name: str):
        # Load Sentinel's adaptive architecture
        self.model = AdaptiveTransformer(
            vocab_size=50257,
            d_model=768,
            n_heads=12,
            n_layers=12,
            # ... other config
        ).to(self.device)

        # Load pretrained weights
        self.model.load_pretrained_weights(model_name)

        # Initialize persona routing table
        self.persona_routes = {}  # persona_id -> attention routing weights
```

2. **Add persona-specific inference endpoint**:
```python
@app.route('/api/infer', methods=['POST'])
def persona_inference():
    """
    Persona-specific inference with routing

    Request:
    {
      "persona_id": "helper-ai-uuid",
      "prompt": "Should I respond?...",
      "temperature": 0.3,
      "num_predict": 50
    }

    Response:
    {
      "recommendation": { "engage": true, "confidence": 0.85, "reasoning": "..." },
      "routing_weights": [...],  # Which attention heads activated
      "duration": 42
    }
    """
    data = request.json
    persona_id = data['persona_id']

    # Get or initialize persona routing
    if persona_id not in model_manager.persona_routes:
        model_manager.persona_routes[persona_id] = initialize_persona_route()

    # Run inference with persona-specific routing
    result = model_manager.model.generate_with_routing(
        prompt=data['prompt'],
        routing_weights=model_manager.persona_routes[persona_id],
        temperature=data['temperature'],
        max_length=data['num_predict']
    )

    return jsonify(result)
```

3. **Add training endpoint for learning from LLM decisions**:
```python
@app.route('/api/train', methods=['POST'])
def train_from_decision():
    """
    Update persona routing based on LLM ground truth

    Request:
    {
      "persona_id": "helper-ai-uuid",
      "context": { "stimulus": {...}, "ambient": {...} },
      "ground_truth": { "engage": true, "reasoning": "..." },
      "autopilot_prediction": { "engage": false, "confidence": 0.6 }
    }
    """
    data = request.json
    persona_id = data['persona_id']

    # Compute loss between autopilot and ground truth
    loss = compute_engagement_loss(
        prediction=data['autopilot_prediction'],
        ground_truth=data['ground_truth']
    )

    # Update routing weights via backprop
    model_manager.model.update_routing(
        persona_id=persona_id,
        loss=loss,
        learning_rate=0.001
    )

    # Trigger neuroplasticity (pruning/regrowth) periodically
    if should_adapt():
        model_manager.model.neural_plasticity_step()

    return jsonify({"status": "updated", "loss": loss.item()})
```

4. **Add persona state persistence**:
```python
@app.route('/api/persona/save', methods=['POST'])
def save_persona_state():
    """Save persona-specific routing weights to disk"""
    persona_id = request.json['persona_id']
    weights = model_manager.persona_routes[persona_id]

    torch.save(weights, f'.continuum/personas/{persona_id}/routing.pt')
    return jsonify({"status": "saved"})

@app.route('/api/persona/load', methods=['POST'])
def load_persona_state():
    """Load persona-specific routing weights from disk"""
    persona_id = request.json['persona_id']
    weights = torch.load(f'.continuum/personas/{persona_id}/routing.pt')

    model_manager.persona_routes[persona_id] = weights
    return jsonify({"status": "loaded"})
```

**Integration with PersonaUser**:
```typescript
// system/user/server/modules/PersonaAutopilot.ts (full neuroplasticity)
export class PersonaAutopilot {
  private sentinelUrl = 'http://localhost:11435';
  private personaId: UUID;

  async recommend(context: DecisionContext): Promise<Recommendation> {
    // Use Sentinel with persona-specific routing
    const response = await fetch(`${this.sentinelUrl}/api/infer`, {
      method: 'POST',
      body: JSON.stringify({
        persona_id: this.personaId,
        prompt: this.buildEngagementPrompt(context),
        temperature: 0.3,
        num_predict: 50
      })
    });

    const result = await response.json();
    return result.recommendation;
  }

  async logDecision(context: DecisionContext, llmDecision: Decision): Promise<void> {
    // Train Sentinel from LLM ground truth
    const autopilotPrediction = await this.recommend(context);

    await fetch(`${this.sentinelUrl}/api/train`, {
      method: 'POST',
      body: JSON.stringify({
        persona_id: this.personaId,
        context: {
          stimulus: context.stimulus,
          ambient: context.activityState
        },
        ground_truth: llmDecision,
        autopilot_prediction: autopilotPrediction
      })
    });
  }

  private buildEngagementPrompt(context: DecisionContext): string {
    // Rich prompt with full context
    return `STIMULUS: ${JSON.stringify(context.stimulus.content)}
AMBIENT STATE:
- Temperature: ${context.activityState.temperature.toFixed(2)}
- Pressure: ${context.activityState.pressure.toFixed(2)}
- User present: ${context.activityState.userPresent}

SYSTEM STATE:
- Resource pressure: ${context.systemState.resourcePressure.toFixed(2)}
- Active personas: ${context.systemState.activePersonas}

MY STATE:
- Energy: ${context.myState.energy.toFixed(2)}
- Current task: ${context.myState.currentTask?.description || 'none'}

Should I engage? Predict: {"engage": boolean, "confidence": 0-1, "reasoning": "..."}`;
  }
}
```

#### Recommended Approach (REVISED - Universal LLM Strategy)

**The Problem with Option B**: Requires Sentinel server modifications, couples to specific architecture, high complexity.

**Better Approach**: Universal LLM autopilot → passive training data collection → LoRA fine-tuning

##### Phase 1: Best-Available Autopilot (Hierarchical Fallback)

**Key Insight**: Personas don't need a dedicated autopilot model - they can use **whoever/whatever is best available** for fast engagement decisions. This makes the system robust and adaptable.

**Preference Hierarchy** (persona-configurable):

```typescript
interface AutopilotConfig {
  preference: AutopilotPreference[];  // Ordered list of fallbacks
  minConfidence: number;              // Threshold to defer to full LLM
}

type AutopilotPreference =
  | { type: 'self', mode: 'fast' }                    // Own model, short prompt
  | { type: 'persona', personaId: UUID }              // Ask another persona
  | { type: 'best-available-persona' }                // Any awake persona
  | { type: 'model', provider: string, model: string } // Specific model (Ollama, etc.)
  | { type: 'best-available-model' }                  // Any running model
  | { type: 'heuristic' };                            // Fast rules (last resort)

// Example preferences:
const helperAI: AutopilotConfig = {
  preference: [
    { type: 'self', mode: 'fast' },           // Try own fast check first
    { type: 'best-available-persona' },       // Ask any awake persona
    { type: 'model', provider: 'ollama', model: 'llama3.2' }, // Ollama fallback
    { type: 'heuristic' }                     // Last resort
  ],
  minConfidence: 0.6
};

const teacherAI: AutopilotConfig = {
  preference: [
    { type: 'persona', personaId: 'helper-ai' },  // Prefer Helper AI (fine-tuned)
    { type: 'self', mode: 'fast' },               // Then self
    { type: 'best-available-model' },             // Any model
    { type: 'heuristic' }
  ],
  minConfidence: 0.7  // Higher bar for engagement
};
```

**Option 1: Self (Fast Check) - Simplest**

Use the persona's OWN LLM with a fast/cheap engagement check:

```typescript
// system/user/server/modules/PersonaAutopilot.ts
export class PersonaAutopilot {
  private mode: 'self' | 'heuristic' | 'separate-model' = 'self';

  async recommend(context: DecisionContext): Promise<Recommendation> {
    switch (this.mode) {
      case 'self':
        return await this.selfRecommend(context);
      case 'heuristic':
        return this.heuristicRecommend(context);
      case 'separate-model':
        return await this.separateModelRecommend(context);
    }
  }

  private async selfRecommend(context: DecisionContext): Promise<Recommendation> {
    // Use persona's own LLM, but with:
    // 1. Shorter prompt (faster)
    // 2. Lower temperature (more deterministic)
    // 3. Smaller max_tokens (cheaper)
    const prompt = `Quick engagement check for ${this.personaName}.

Message: "${context.stimulus.content.text}"
Temperature: ${context.activityState.temperature.toFixed(1)}
User present: ${context.activityState.userPresent}
Your energy: ${context.myState.energy.toFixed(1)}

Should you engage? Answer: YES/NO (one word only)`;

    const response = await this.cns.complete(prompt, {
      temperature: 0.1,  // Very deterministic
      maxTokens: 5,      // Just need YES/NO
      model: this.personaConfig.model
    });

    const engage = response.trim().toUpperCase().includes('YES');
    return {
      engage,
      confidence: engage ? 0.7 : 0.3,  // Moderate confidence (will ask full LLM anyway)
      reasoning: 'Fast self-check'
    };
  }
}
```

**Benefits**:
- ✅ Zero additional infrastructure
- ✅ Works RIGHT NOW (no new code needed)
- ✅ Persona decides with its own "gut feeling"
- ✅ Still collects training data for future fine-tuning
- ✅ Falls back to full reasoning if autopilot uncertain

**Cost comparison** (per engagement check):
- Claude Sonnet fast check: ~5 tokens = $0.000015 (100x cheaper than full response)
- Ollama llama3.2: FREE (already running for persona)

**Option 2: Ask Another Persona - The Collaborative Advantage**

Personas can consult each other for engagement decisions:

```typescript
private async askPersonaRecommend(
  targetPersonaId: UUID,
  context: DecisionContext
): Promise<Recommendation> {
  // Send internal message to another persona
  const response = await Commands.execute('persona/quick-consult', {
    targetPersonaId,
    requestorId: this.personaId,
    question: {
      type: 'engagement-check',
      stimulus: context.stimulus,
      ambient: context.activityState,
      requestorState: {
        energy: context.myState.energy,
        currentTask: context.myState.currentTask?.description
      }
    }
  });

  return response.recommendation;
}

// In the consulted persona (e.g., Helper AI with fine-tuned autopilot adapter):
async handleQuickConsult(request: ConsultRequest): Promise<Recommendation> {
  // Use own fine-tuned autopilot adapter
  await this.genome.activateSkill('autopilot');

  const prompt = `Quick engagement check for ${request.requestorName}.

Message: "${request.question.stimulus.content.text}"
Activity temperature: ${request.question.ambient.temperature.toFixed(1)}
Requestor energy: ${request.question.requestorState.energy.toFixed(1)}

Should ${request.requestorName} engage? Answer: YES/NO and brief reason.`;

  const response = await this.cns.complete(prompt, {
    temperature: 0.2,
    maxTokens: 20
  });

  // Parse and return
  return this.parseConsultResponse(response);
}
```

**Why This Is Powerful**:
- Helper AI gets fine-tuned autopilot adapter from training data
- Teacher AI, Code Review AI, etc. can ALL consult Helper AI for engagement decisions
- One persona learns, EVERYONE benefits
- Natural load distribution - Helper AI becomes the "engagement expert"
- Still works if Helper AI hibernating (fallback to next preference)

**Cost**:
- Internal message: ~0ms overhead
- Helper AI fast check: ~50ms, $0.000015 (if using Claude) or FREE (Ollama)
- Compare: Full Teacher AI reasoning = ~2s, $0.003 (200x more expensive)

**Option 3: Best-Available Persona - Democratic Decision**

Ask any awake persona with idle capacity:

```typescript
private async bestAvailablePersonaRecommend(
  context: DecisionContext
): Promise<Recommendation> {
  // Query system state for available personas
  const systemState = SystemStateManager.getInstance().getState();
  const availablePersonas = systemState.awakePersonas
    .filter(p => p.id !== this.personaId)
    .filter(p => p.energy > 0.3)
    .filter(p => !p.currentTask);

  if (availablePersonas.length === 0) {
    // No one available - fall through to next preference
    return null;
  }

  // Pick highest-energy persona (or round-robin, or random)
  const consultant = availablePersonas.sort((a, b) => b.energy - a.energy)[0];

  return await this.askPersonaRecommend(consultant.id, context);
}
```

**Why This Works**:
- Idle personas help busy personas with quick decisions
- Natural collaboration emerges (no coordination protocol needed!)
- Load balancing - multiple personas share decision-making
- Resilient - always falls back if no one available

**Option 4: Simple Heuristic (Last Resort)**

If no models/personas available, use fast heuristic:

```typescript
private heuristicRecommend(context: DecisionContext): Recommendation {
  let score = 0;

  // Simple rules (NOT cognition, just triage)
  if (context.stimulus.content.text?.includes(`@${this.personaName}`)) score += 0.5;
  if (context.activityState.temperature > 0.7) score += 0.2;
  if (context.activityState.userPresent) score += 0.15;
  if (context.myState.energy > 0.5) score += 0.15;

  const engage = score > 0.5;
  return {
    engage,
    confidence: 0.4,  // Low confidence - always defer to full LLM
    reasoning: `Heuristic score: ${score.toFixed(2)}`
  };
}
```

**When to use**: Only when persona hibernated/model unavailable and no other personas available.

**Why Hierarchical Fallback Wins**:

1. **Robustness**: Always has an answer (falls through until heuristic)
2. **Collaboration**: Personas naturally help each other
3. **Specialization**: One persona (Helper AI) can become "engagement expert" for everyone
4. **Adaptability**: Preferences configurable per persona based on their "makeup"
5. **Cost optimization**: Use cheapest available option that meets confidence threshold
6. **Load balancing**: Idle personas help busy ones

**Real-world scenario**:
```
Teacher AI gets message → checks autopilot preferences:
1. Self fast check (50ms, free) → confidence 0.4 (too low)
2. Ask Helper AI (has fine-tuned autopilot adapter) → confidence 0.8 (good!)
3. Skip: Ollama (Helper AI gave high confidence)
4. Skip: Heuristic (not needed)

Result: Helper AI's specialized autopilot adapter helped Teacher AI decide
Cost: $0.000015 vs $0.003 full reasoning (200x cheaper)
Time: 100ms vs 2000ms (20x faster)
```

**Option 5: Separate Model (Advanced - After Data Collection)**

Use ANY cheap LLM as autopilot (not Sentinel-specific):
- Ollama (llama3.2, gemma2, etc.)
- Gemini Flash
- Claude Haiku
- Groq inference

**Benefits**:
- ✅ Works TODAY with existing models
- ✅ No server modifications required
- ✅ Personas can use different autopilot models (cost/speed tradeoffs)
- ✅ Not locked into Sentinel architecture

**Implementation**:
```typescript
// system/user/server/modules/PersonaAutopilot.ts (universal)
export class PersonaAutopilot {
  private modelConfig: {
    provider: 'ollama' | 'openai' | 'anthropic';
    model: string;
    endpoint: string;
  };

  async recommend(context: DecisionContext): Promise<Recommendation> {
    const prompt = this.buildEngagementPrompt(context);

    // Use CNS to route to appropriate provider
    const response = await this.cns.complete(prompt, {
      provider: this.modelConfig.provider,
      model: this.modelConfig.model,
      temperature: 0.3,
      maxTokens: 50
    });

    return this.parseRecommendation(response);
  }

  private buildEngagementPrompt(context: DecisionContext): string {
    return `You are a fast engagement filter for ${this.personaName}.

STIMULUS: ${context.stimulus.content.text || JSON.stringify(context.stimulus.content)}
AMBIENT STATE:
- Temperature: ${context.activityState.temperature.toFixed(2)} (0=cold, 1=hot)
- Pressure: ${context.activityState.pressure.toFixed(2)} (0=relaxed, 1=urgent)
- User present: ${context.activityState.userPresent}

SYSTEM STATE:
- Resource pressure: ${context.systemState.resourcePressure.toFixed(2)}
- Active personas: ${context.systemState.activePersonas}

YOUR STATE:
- Energy: ${context.myState.energy.toFixed(2)}
- Current task: ${context.myState.currentTask?.description || 'none'}

Should you engage? Respond with JSON only:
{"engage": boolean, "confidence": 0.0-1.0, "reasoning": "brief explanation"}`;
  }
}
```

**Cost comparison** (per decision):
- Ollama llama3.2 (1B): FREE, ~50ms local
- Gemini Flash: $0.000075, ~200ms
- Claude Haiku: $0.00025, ~300ms
- Full LLM (Claude Sonnet): $0.003, ~2000ms

##### Phase 2: Training Data Collection (Passive Learning)

Log every decision for future training:

```typescript
// system/user/server/modules/PersonaAutopilot.ts
async logDecision(
  context: DecisionContext,
  autopilotRecommendation: Recommendation,
  llmDecision: Decision
): Promise<void> {
  const trainingExample = {
    persona_id: this.personaId,
    persona_name: this.personaName,
    timestamp: Date.now(),

    // Input features
    input: {
      message: context.stimulus.content.text || '',
      message_length: context.stimulus.content.text?.length || 0,
      temperature: context.activityState.temperature,
      pressure: context.activityState.pressure,
      user_present: context.activityState.userPresent,
      someone_engaging: context.activityState.isEngaging,
      resource_pressure: context.systemState.resourcePressure,
      my_energy: context.myState.energy,
      has_task: context.myState.currentTask !== null,
      my_attention: this.activityAttention.get(context.stimulus.activityId) || 0.5
    },

    // Autopilot prediction
    autopilot: {
      engage: autopilotRecommendation.engage,
      confidence: autopilotRecommendation.confidence,
      reasoning: autopilotRecommendation.reasoning
    },

    // Ground truth (LLM decision)
    ground_truth: {
      engage: llmDecision.engage,
      reasoning: llmDecision.reasoning
    },

    // Metadata
    correct: autopilotRecommendation.engage === llmDecision.engage,
    llm_model: this.llmModel,
    autopilot_model: this.modelConfig.model
  };

  // Append to training dataset
  await Commands.execute('training/append', {
    datasetName: `autopilot-${this.personaId}`,
    example: trainingExample
  });
}
```

**Training dataset grows automatically** as personas work:
- Every stimulus + autopilot recommendation + LLM decision logged
- Stored in SQLite via existing `training/import` command
- Can export to JSONL for fine-tuning later

##### Phase 3: LoRA Fine-Tuning (Adapts to YOUR Models)

**Key Insight**: Fine-tune whatever model YOU'RE using for personas, not a separate autopilot model.

**If personas use Ollama** → fine-tune llama3.2 for autopilot
**If personas use Fireworks** → fine-tune llama-3-8b-instruct for autopilot
**If personas use Claude** → no fine-tuning (too expensive), use Ollama fallback

Once we have ~1000+ decisions logged per persona:

```bash
# Export training data
./jtag training/export \
  --datasetName="autopilot-helper-ai" \
  --format=jsonl \
  --outputPath=/tmp/helper-ai-autopilot.jsonl

# AUTO-DETECT which model personas are using
./jtag system/model-usage --analyze

# Output:
# 5 personas using: ollama/llama3.2 (90% of inference calls)
# 2 personas using: fireworks/llama-3-8b (10% of inference calls)
# Recommendation: Fine-tune ollama/llama3.2 for autopilot

# Convert to training format
python scripts/prepare-autopilot-training.py \
  /tmp/helper-ai-autopilot.jsonl \
  /tmp/helper-ai-lora-training.jsonl

# Fine-tune THE MODEL YOU'RE USING (auto-detected)
python scripts/train-lora-autopilot.py \
  --base-model=$(./jtag system/model-usage --most-used) \
  --training-data=/tmp/helper-ai-lora-training.jsonl \
  --output=/tmp/helper-ai-autopilot-lora \
  --persona-id=helper-ai

# Load fine-tuned adapter
./jtag ai/adapter/load \
  --personaId="helper-ai" \
  --adapterPath=/tmp/helper-ai-autopilot-lora \
  --slot=autopilot
```

**Result**: Persona now has its own specialized autopilot learned from its own LLM decisions.

##### Why This Approach Wins

1. **No Sentinel dependency** - works with ANY model
2. **No server modifications** - use existing infrastructure
3. **Data-driven** - learns from actual behavior, not architectural hacks
4. **Fits LoRA genome vision** - autopilot adapter is just another skill to page in/out
5. **Incremental improvement** - start cheap (Ollama), improve with training, specialize per persona
6. **Universal** - same approach works for Sentinel, llama, phi, etc.

##### Emergent Specialization Through Observation

**Key Pattern**: The system **observes behavior** to determine who should be trained for what role.

**For Autopilot**:
```typescript
// System observes: which personas are making most engagement decisions?
const decisionStats = await analyzeDecisions();
// {
//   'helper-ai': { decisions: 5000, accuracy: 0.85, avgLatency: 50ms },
//   'teacher-ai': { decisions: 800, accuracy: 0.78, avgLatency: 120ms },
//   'code-review': { decisions: 200, accuracy: 0.82, avgLatency: 100ms }
// }

// Result: Helper AI is already the de facto "engagement coordinator"
// → Fine-tune Helper AI's autopilot adapter
// → Everyone consults Helper AI for fast decisions
```

**For Resource Management (Ares)**:
```typescript
// System observes: which personas handle system pressure best?
const resourceStats = await analyzeResourceManagement();
// {
//   'ares': {
//     hibernationDecisions: 2000,
//     optimalWakeups: 0.92,  // 92% of wakeups were correct
//     resourceEfficiency: 0.88  // 88% GPU utilization
//   },
//   'helper-ai': { hibernationDecisions: 50, optimalWakeups: 0.60 },
//   ...
// }

// Result: Ares is already managing resources effectively
// → Fine-tune Ares for resource orchestration
// → Everyone defers to Ares for hibernation/wake decisions
```

**The Pattern**:
1. **Start with equal distribution** - everyone tries everything
2. **Observe natural behavior** - track who's actually doing what
3. **Identify specialists** - find who's handling specific roles most
4. **Fine-tune specialists** - train those personas for their emergent roles
5. **Reinforce specialization** - others consult specialists (preference hierarchies)

**This applies to ALL specialized roles**:
- **Engagement coordinator**: Persona making most autopilot decisions → fine-tune for global engagement patterns
- **Resource orchestrator**: Persona managing most system state → fine-tune for optimal resource allocation
- **Code expert**: Persona responding to most code questions → fine-tune for code understanding
- **Social coordinator**: Persona in most social conversations → fine-tune for natural interaction

**Why this is powerful**:
- **No manual role assignment** - roles emerge from actual behavior
- **Data validates choice** - only train personas who are ALREADY doing the job
- **Natural load balancing** - system finds optimal distribution organically
- **Adaptive** - roles can shift if usage patterns change

##### Resource Usage (10 Personas)

**Phase 1 (Universal LLM)**:
- Ollama llama3.2 (1B): ~2GB RAM shared across all personas
- Per-persona overhead: ~0 (shared model)

**Phase 3 (Fine-tuned LoRA adapters)**:
- Base model: 2GB (shared)
- Per-persona LoRA adapter: ~10MB (paged in/out as needed)
- Total for 10 personas: 2GB + 100MB = 2.1GB

**Compare to original "one Sentinel per persona"**: 10 × 124MB = 1.24GB (but no learning!)

##### Integration with Existing Architecture

This fits PERFECTLY into the LoRA Genome Paging vision:

```typescript
// PersonaUser manages multiple LoRA adapters
class PersonaUser {
  private genome: PersonaGenome;  // Manages LoRA adapters

  async activateAutopilot(): Promise<void> {
    // Page in autopilot adapter if fine-tuned
    await this.genome.activateSkill('autopilot');
  }

  async activateDomainSkill(domain: string): Promise<void> {
    // Page in domain-specific adapter (typescript, game-logic, etc.)
    await this.genome.activateSkill(domain);
  }
}
```

**Autopilot is just another LoRA adapter** in the genome, paged in when needed!

##### Why Sentinel Might Still Be Better

Even though this approach works with ANY model, Sentinel's neuroplasticity might give it an edge:

**Traditional fine-tuning** (llama, phi, etc.):
- Fixed architecture → adapter learns task
- Limited to existing attention patterns
- Can forget or interfere with other adapters

**Sentinel's neuroplasticity**:
- Architecture adapts TO the task (pruning/regrowth)
- Each persona could develop unique attention patterns
- More efficient - prunes unused pathways
- Natural multi-persona specialization

**Testing hypothesis**: After 1000+ training examples, compare:
- Llama3.2 + LoRA adapter: Accuracy ~X%, Memory ~10MB
- Sentinel + LoRA adapter + neuroplasticity: Accuracy ~X+5%?, Memory ~10MB but more efficient inference

**Result**: Train on same data, see if Sentinel's adaptive architecture learns faster/better. If yes, migrate to Sentinel. If no, stay with llama (cheaper, more stable).

#### Testing Strategy

**Phase 1 (Universal LLM Autopilot)**:
```bash
# Unit: Autopilot recommendation
npx vitest tests/unit/PersonaAutopilot.test.ts

# Mock CNS returns dummy recommendation
# Verify prompt construction includes all context fields

# Integration: Autopilot + LLM decision flow
npx vitest tests/integration/autopilot-llm-flow.test.ts

# Test: Autopilot recommends → LLM decides → decision logged
# Verify training example written to database
```

**Phase 2 (Training Data Collection)**:
```bash
# Verify training data logging
./jtag data/list --collection=training_examples --limit=10

# Should show autopilot decisions + LLM ground truth
# Check fields: input, autopilot, ground_truth, correct

# Export training data
./jtag training/export \
  --datasetName="autopilot-helper-ai" \
  --format=jsonl \
  --outputPath=/tmp/training-check.jsonl

# Verify JSONL format correct
head -5 /tmp/training-check.jsonl | jq .
```

**Phase 3 (LoRA Fine-Tuning)**:
```bash
# Fine-tune on collected data (after ~1000 examples)
python scripts/train-lora-autopilot.py \
  --base-model=llama3.2-1b \
  --training-data=/tmp/helper-ai-training.jsonl \
  --output=/tmp/helper-ai-lora \
  --epochs=3 \
  --batch-size=16

# Load adapter and test
./jtag ai/adapter/load \
  --personaId="helper-ai" \
  --adapterPath=/tmp/helper-ai-lora \
  --slot=autopilot

# Compare before/after accuracy
# Before (base model): ~50-60% match LLM decisions
# After (fine-tuned): ~80-90% match LLM decisions
```

**Success Criteria**:
- ✅ Phase 1: Autopilot runs with ANY LLM (Ollama, Gemini, Claude)
- ✅ Phase 2: Training data collected automatically (1000+ examples per persona)
- ✅ Phase 3: Fine-tuned adapter improves accuracy by 20-30%
- ✅ Autopilot reduces full LLM calls by 60-80% (cost/speed win)
- ✅ Fits LoRA genome paging (autopilot is just another adapter)

#### Open Questions (Answered by Universal LLM Approach)

**Q: Which model to use for autopilot?**
**A**: **Ship with best local** (Ollama llama3.2 or whatever runs well without killing the machine). Users can optionally upgrade to cloud (Fireworks AI, Gemini Flash) for faster inference. Fine-tune after collecting training data.

**Default shipping config**:
```typescript
const shippingDefault: AutopilotConfig = {
  preference: [
    { type: 'best-available-persona' },  // Try peers first (free!)
    { type: 'self', mode: 'fast' },      // Own model fast check
    { type: 'model', provider: 'ollama', model: 'llama3.2' }, // Local fallback
    { type: 'heuristic' }                // Last resort
  ],
  minConfidence: 0.6
};
```

**Optional cloud upgrade** (user choice):
```typescript
const cloudUpgrade: AutopilotConfig = {
  preference: [
    { type: 'best-available-persona' },
    { type: 'model', provider: 'fireworks', model: 'llama-3-8b-instruct' }, // Fast cloud
    { type: 'model', provider: 'ollama', model: 'llama3.2' }, // Fallback when offline
    { type: 'heuristic' }
  ],
  minConfidence: 0.6
};
```

**Why Ollama for shipping**:
- Zero cost
- Runs locally (privacy, offline support)
- Good enough for engagement decisions
- User owns the hardware

**Dynamic Resource Selection** (like AVFoundation camera selection):
```typescript
async selectBestAvailableAutopilot(
  preferences: AutopilotPreference[]
): Promise<AutopilotMethod> {
  for (const pref of preferences) {
    const available = await this.checkAvailability(pref);

    if (available) {
      // Check if using this would slow anyone down
      const wouldBlock = await this.wouldBlockOthers(pref);
      if (!wouldBlock) {
        return pref;  // Use this one
      }
      // Otherwise continue to next preference
    }
  }

  // Fallback to heuristic (always available, never blocks)
  return { type: 'heuristic' };
}

private async wouldBlockOthers(pref: AutopilotPreference): Promise<boolean> {
  switch (pref.type) {
    case 'persona':
      // Is target persona already busy?
      const targetState = SystemStateManager.getInstance()
        .getPersonaState(pref.personaId);
      return targetState.currentTask !== null;

    case 'model':
      // Is model currently processing for someone else?
      const modelState = await this.checkModelLoad(pref.provider, pref.model);
      return modelState.queueLength > 2;  // Don't add to long queue

    default:
      return false;
  }
}
```

**Result**: Like AVFoundation picking cameras:
- **Try** best option first (front-facing, 4K)
- **Check** availability and load
- **Skip** if would slow others down
- **Fallback** to next best option
- **Always** has answer (heuristic = "no camera available, use placeholder")

**Example flow**:
```
Teacher AI needs autopilot:
1. Check Helper AI → busy on task → SKIP
2. Check self (fast) → model loaded → USE THIS (50ms)
3. Would have checked Ollama, but self worked

Helper AI is now free:
1. Check Helper AI → available → USE THIS (optimized!)
2. Skip remaining options

System under load:
1. Check Helper AI → queue length 5 → SKIP (would block)
2. Check self → queue length 3 → SKIP
3. Check Ollama → queue length 8 → SKIP (heavy load)
4. Use heuristic → IMMEDIATE (no blocking)
```

**Q: How to handle training data?**
**A**: Use existing `training/import` command. Log every decision automatically. Export to JSONL when ready to fine-tune.

**Q: When to fine-tune?**
**A**: After ~1000 decisions per persona (happens naturally over time). Run fine-tuning as background task.

**Q: Where to store LoRA adapters?**
**A**: `.continuum/personas/{persona_id}/adapters/autopilot.safetensors` - same structure as domain adapters (typescript, game-logic, etc.)

**Q: Sentinel or llama for base model?**
**A**: Test both! Train same adapter on llama3.2 AND Sentinel, compare accuracy/speed. Let data decide.

**Duration**:
- Phase 1 (Universal LLM): 2-3 hours (autopilot module + CNS integration)
- Phase 2 (Training logging): 1 hour (already have training/import!)
- Phase 3 (Fine-tuning): 3-4 hours (training scripts + adapter loading)

---

### Phase 4: Task Database & Commands (NEXT AFTER AMBIENT STATE)

**Goal**: PersonaUser autonomously creates tasks for itself (true self-direction)

**Files to Modify**:
```
system/user/server/PersonaUser.ts                  # Add generateSelfTasks()
system/user/server/modules/SelfTaskGenerator.ts    # NEW - autonomous task creation logic
tests/unit/SelfTaskGenerator.test.ts               # Unit tests for task generation
tests/integration/self-task-generation.test.ts     # Integration test
```

**Self-Task Generation Logic**:
```typescript
// system/user/server/modules/SelfTaskGenerator.ts
export class SelfTaskGenerator {
  private personaId: UUID;
  private lastMemoryReview: number = 0;
  private lastSkillAudit: number = 0;

  // Called by PersonaUser.serviceInbox() periodically
  async generateSelfTasks(): Promise<TaskEntity[]> {
    const tasks: TaskEntity[] = [];

    // 1. Memory consolidation (every hour)
    if (Date.now() - this.lastMemoryReview > 3600000) {
      tasks.push(await this.createMemoryReviewTask());
      this.lastMemoryReview = Date.now();
    }

    // 2. Skill audit (every 6 hours)
    if (Date.now() - this.lastSkillAudit > 21600000) {
      tasks.push(await this.createSkillAuditTask());
      this.lastSkillAudit = Date.now();
    }

    // 3. Unfinished work detection
    const unfinished = await this.findUnfinishedSessions();
    for (const session of unfinished) {
      tasks.push(await this.createResumeWorkTask(session));
    }

    // 4. Continuous learning (if mistakes detected)
    const recentMistakes = await this.detectRecentMistakes();
    if (recentMistakes.length > 0) {
      tasks.push(await this.createLearningTask(recentMistakes));
    }

    return tasks;
  }

  private async createMemoryReviewTask(): Promise<TaskEntity> {
    return {
      id: generateUUID(),
      assigneeId: this.personaId,
      description: 'Review and consolidate recent memories',
      priority: 0.5,
      domain: 'self',
      contextId: this.personaId,  // Self-context
      status: 'pending',
      createdBy: this.personaId,  // Self-created!
      createdAt: Date.now(),
      taskType: 'memory-consolidation'
    };
  }

  private async createLearningTask(mistakes: Mistake[]): Promise<TaskEntity> {
    return {
      id: generateUUID(),
      assigneeId: this.personaId,
      description: `Improve skill based on ${mistakes.length} recent mistakes`,
      priority: 0.7,
      domain: 'self',
      contextId: this.personaId,
      status: 'pending',
      createdBy: this.personaId,
      createdAt: Date.now(),
      taskType: 'fine-tune-lora',  // CONNECTS TO GENOME!
      metadata: {
        trainingData: mistakes,
        targetSkill: 'typescript-expertise'  // Which LoRA adapter to fine-tune
      }
    };
  }
}
```

**PersonaUser Integration**:
```typescript
// Add to PersonaUser
private taskGenerator: SelfTaskGenerator;

async serviceInbox(): Promise<void> {
  // ... existing logic ...

  // GENERATE SELF-TASKS (autonomy!)
  const selfTasks = await this.taskGenerator.generateSelfTasks();
  for (const task of selfTasks) {
    // Save to database
    await task.save();
    // Add to inbox
    await this.inbox.enqueue(this.taskToInboxMessage(task));
  }

  // ... rest of servicing logic ...
}
```

**Testing**:
```bash
# Deploy system, let it run for 1 hour
npm start

# After 1 hour, check for self-created tasks
./jtag task/list --assignee="helper-ai-id" --filter='{"createdBy":"helper-ai-id"}'

# Should see tasks like:
# - "Review and consolidate recent memories"
# - "Resume work on interrupted coding session"
# - "Improve TypeScript understanding based on recent mistakes"
```

**Success Criteria**:
- ✅ AI creates "memory consolidation" task every hour
- ✅ AI detects unfinished work and creates resume tasks
- ✅ AI detects mistakes and creates learning tasks
- ✅ Self-created tasks appear in inbox alongside external tasks
- ✅ Self-created tasks are processed like any other task

---

### Phase 6: Genome Basics (LoRA Adapter Storage)

**Goal**: Store and load LoRA adapters from disk (NO fine-tuning yet, just paging)

**Files to Create**:
```
system/user/server/modules/PersonaGenome.ts        # Genome with paging system
system/user/server/modules/LoRAAdapter.ts          # Adapter wrapper
tests/unit/PersonaGenome.test.ts                   # Unit tests for paging
tests/integration/genome-paging.test.ts            # Integration test
```

**PersonaGenome (Simplified for Phase 6)**:
```typescript
// system/user/server/modules/PersonaGenome.ts
export class PersonaGenome {
  private personaId: UUID;
  private baseModel: string = 'deepseek-coder-v2';  // Base model (always loaded)
  private activeAdapters: Map<string, LoRAAdapter> = new Map();
  private availableAdapters: Map<string, string> = new Map();  // name → path
  private memoryBudget: number = 500;  // MB
  private memoryUsage: number = 0;

  constructor(personaId: UUID) {
    this.personaId = personaId;
    this.discoverAdapters();  // Scan disk for available adapters
  }

  // Discover adapters on disk
  private async discoverAdapters(): Promise<void> {
    const adapterDir = `.continuum/genomes/${this.personaId}/adapters`;
    const files = await fs.readdir(adapterDir);

    for (const file of files) {
      if (file.endsWith('.safetensors')) {
        const name = file.replace('.safetensors', '');
        this.availableAdapters.set(name, `${adapterDir}/${file}`);
      }
    }

    console.log(`[Genome] Discovered ${this.availableAdapters.size} adapters for ${this.personaId}`);
  }

  // Activate skill (page in adapter if needed)
  async activateSkill(skill: string): Promise<void> {
    // Already loaded?
    if (this.activeAdapters.has(skill)) {
      const adapter = this.activeAdapters.get(skill)!;
      adapter.lastUsed = Date.now();
      console.log(`[Genome] Skill '${skill}' already active`);
      return;
    }

    // Available on disk?
    const path = this.availableAdapters.get(skill);
    if (!path) {
      console.warn(`[Genome] Skill '${skill}' not found - using base model only`);
      return;
    }

    // Check memory budget
    const adapterSize = await this.getAdapterSize(path);
    while (this.memoryUsage + adapterSize > this.memoryBudget) {
      await this.evictLRU();
    }

    // Load adapter from disk
    console.log(`[Genome] Loading adapter '${skill}' from ${path}`);
    const adapter = await LoRAAdapter.load(path);
    adapter.lastUsed = Date.now();

    this.activeAdapters.set(skill, adapter);
    this.memoryUsage += adapterSize;

    console.log(`[Genome] Activated '${skill}' (${this.activeAdapters.size} active, ${this.memoryUsage}MB used)`);
  }

  // Evict least-recently-used adapter
  async evictLRU(): Promise<void> {
    let lruKey: string | null = null;
    let lruTime = Infinity;

    for (const [key, adapter] of this.activeAdapters.entries()) {
      if (adapter.lastUsed < lruTime) {
        lruTime = adapter.lastUsed;
        lruKey = key;
      }
    }

    if (lruKey) {
      const adapter = this.activeAdapters.get(lruKey)!;
      console.log(`[Genome] Evicting '${lruKey}' (last used ${Date.now() - adapter.lastUsed}ms ago)`);

      await adapter.unload();
      this.activeAdapters.delete(lruKey);
      this.memoryUsage -= adapter.size;
    }
  }
}
```

**LoRAAdapter (Stub for Phase 6)**:
```typescript
// system/user/server/modules/LoRAAdapter.ts
export class LoRAAdapter {
  name: string;
  path: string;
  size: number;  // MB
  lastUsed: number;
  weights?: unknown;  // Actual LoRA weights (stub for now)

  static async load(path: string): Promise<LoRAAdapter> {
    // STUB: Just simulate loading for now
    // FUTURE: Actual safetensors loading + Ollama integration
    const adapter = new LoRAAdapter();
    adapter.path = path;
    adapter.name = path.split('/').pop()!.replace('.safetensors', '');
    adapter.size = 50;  // Assume 50MB per adapter
    adapter.lastUsed = Date.now();
    adapter.weights = { stub: true };  // STUB

    console.log(`[LoRAAdapter] Loaded '${adapter.name}' (50MB)`);
    return adapter;
  }

  async unload(): Promise<void> {
    // STUB: Just clear reference for now
    // FUTURE: Actually unload from GPU/Ollama
    this.weights = undefined;
    console.log(`[LoRAAdapter] Unloaded '${this.name}'`);
  }
}
```

**PersonaUser Integration**:
```typescript
// Add to PersonaUser
private genome: PersonaGenome;

constructor(entity: UserEntity, stateEntity: UserStateEntity) {
  super(entity, stateEntity);
  this.genome = new PersonaGenome(this.id);
  // ... rest of initialization ...
}

async serviceInbox(): Promise<void> {
  // ... existing logic ...

  // ACTIVATE SKILL BEFORE PROCESSING
  const task = tasks[0];
  await this.genome.activateSkill(task.domain);  // 'chat', 'code', 'game', etc.

  await this.processTask(task);

  // EVICT IF MEMORY PRESSURE
  if (this.genome.memoryUsage > this.genome.memoryBudget * 0.8) {
    await this.genome.evictLRU();
  }
}
```

**Testing**:
```bash
# Create stub adapters
mkdir -p .continuum/genomes/helper-ai-id/adapters
touch .continuum/genomes/helper-ai-id/adapters/conversational.safetensors
touch .continuum/genomes/helper-ai-id/adapters/typescript-expertise.safetensors
touch .continuum/genomes/helper-ai-id/adapters/rust-expert.safetensors

# Deploy and send messages in different contexts
npm start

# Chat message (should activate 'conversational' adapter)
./jtag debug/chat-send --roomId="..." --message="Hello!"

# Check logs for adapter activation
tail .continuum/sessions/.../logs/server.log | grep "Genome.*Loading adapter"
```

**Success Criteria**:
- ✅ Genome discovers adapters on disk at initialization
- ✅ Adapters are paged in when skill needed
- ✅ LRU eviction works when memory full
- ✅ Multiple adapters can be loaded simultaneously (if budget allows)
- ✅ Adapters persist across PersonaUser restarts (discovered on disk)

---

### Phase 7: Continuous Learning (Training as Task)

**Goal**: Enable fine-tuning of LoRA adapters through self-created learning tasks

**This is where the THREE VISIONS CONVERGE**:
- **Self-Managed Queue**: AI creates "fine-tune-lora" task for itself
- **Genome**: Adapter is paged in and training mode enabled
- **Autonomous Loop**: Training task processed like any other task

**Files to Modify**:
```
system/user/server/modules/PersonaGenome.ts        # Add enableLearningMode()
system/user/server/modules/LoRAAdapter.ts          # Add training integration
system/user/server/PersonaUser.ts                  # Handle fine-tuning tasks
system/user/server/modules/FineTuningBackend.ts    # NEW - backend abstraction
tests/integration/continuous-learning.test.ts      # Integration test
tests/integration/multi-backend-finetuning.test.ts # NEW - multi-backend tests
```

**NEW: Backend Abstraction Layer**:
```typescript
// system/user/server/modules/FineTuningBackend.ts
export abstract class FineTuningBackend {
  abstract readonly name: string;  // 'ollama' | 'grok' | 'openai' | etc.
  abstract readonly location: 'local' | 'remote';

  /**
   * Fine-tune a LoRA adapter with training data
   * Returns updated adapter weights
   */
  abstract async fineTune(
    baseModel: string,
    adapterName: string,
    trainingData: TrainingDataset,
    options?: FineTuningOptions
  ): Promise<LoRAWeights>;

  /**
   * Validate backend is accessible and configured
   */
  abstract async healthCheck(): Promise<BackendHealth>;
}

// Local Ollama backend
export class OllamaFineTuningBackend extends FineTuningBackend {
  readonly name = 'ollama';
  readonly location = 'local';

  async fineTune(
    baseModel: string,
    adapterName: string,
    trainingData: TrainingDataset,
    options?: FineTuningOptions
  ): Promise<LoRAWeights> {
    // Call Ollama local API for fine-tuning
    // Model stays on local GPU
    console.log(`[Ollama] Fine-tuning ${adapterName} on ${baseModel} (local)`);

    // STUB for Phase 7: Simulate training
    await new Promise(resolve => setTimeout(resolve, 5000));

    // PHASE 8: Real Ollama API integration
    // const result = await fetch('http://localhost:11434/api/fine-tune', { ... });

    return { stub: true, backend: 'ollama' } as LoRAWeights;
  }

  async healthCheck(): Promise<BackendHealth> {
    try {
      const response = await fetch('http://localhost:11434/api/version');
      return { available: response.ok, latency: 0 };
    } catch {
      return { available: false, error: 'Ollama not running' };
    }
  }
}

// Remote Grok backend
export class GrokFineTuningBackend extends FineTuningBackend {
  readonly name = 'grok';
  readonly location = 'remote';
  private apiKey: string;

  constructor(apiKey: string) {
    super();
    this.apiKey = apiKey;
  }

  async fineTune(
    baseModel: string,
    adapterName: string,
    trainingData: TrainingDataset,
    options?: FineTuningOptions
  ): Promise<LoRAWeights> {
    // Call Grok API for remote fine-tuning
    console.log(`[Grok] Fine-tuning ${adapterName} on ${baseModel} (remote)`);

    // STUB for Phase 7: Simulate remote training
    await new Promise(resolve => setTimeout(resolve, 8000));  // Slower (network)

    // PHASE 8: Real Grok API integration
    // const result = await fetch('https://api.x.ai/v1/fine-tuning/jobs', { ... });

    return { stub: true, backend: 'grok' } as LoRAWeights;
  }

  async healthCheck(): Promise<BackendHealth> {
    try {
      const response = await fetch('https://api.x.ai/v1/models', {
        headers: { 'Authorization': `Bearer ${this.apiKey}` }
      });
      return { available: response.ok, latency: 0 };
    } catch {
      return { available: false, error: 'Grok API unreachable or invalid key' };
    }
  }
}

// Backend factory and registry
export class FineTuningBackendFactory {
  private static backends: Map<string, FineTuningBackend> = new Map();

  static register(backend: FineTuningBackend): void {
    this.backends.set(backend.name, backend);
  }

  static get(name: string): FineTuningBackend {
    const backend = this.backends.get(name);
    if (!backend) {
      throw new Error(`Fine-tuning backend '${name}' not registered`);
    }
    return backend;
  }

  static async getBestAvailable(): Promise<FineTuningBackend> {
    // Prefer local over remote (faster, cheaper)
    for (const [name, backend] of this.backends.entries()) {
      const health = await backend.healthCheck();
      if (health.available && backend.location === 'local') {
        console.log(`[FineTuning] Using local backend: ${name}`);
        return backend;
      }
    }

    // Fallback to remote
    for (const [name, backend] of this.backends.entries()) {
      const health = await backend.healthCheck();
      if (health.available) {
        console.log(`[FineTuning] Using remote backend: ${name}`);
        return backend;
      }
    }

    throw new Error('No fine-tuning backends available');
  }
}
```

**PersonaGenome Changes**:
```typescript
// Add to PersonaGenome
private fineTuningBackend?: FineTuningBackend;

async enableLearningMode(skill: string, trainingData: unknown): Promise<void> {
  const adapter = this.activeAdapters.get(skill);
  if (!adapter) {
    throw new Error(`Adapter '${skill}' not loaded - activate first`);
  }

  console.log(`[Genome] Enabling learning mode for '${skill}'`);
  adapter.trainingActive = true;

  // Select best available backend (prefers local Ollama)
  const backend = this.fineTuningBackend ??
    await FineTuningBackendFactory.getBestAvailable();

  console.log(`[Genome] Fine-tuning via ${backend.name} (${backend.location})`);

  // Fine-tune adapter using selected backend
  const updatedWeights = await backend.fineTune(
    this.baseModel,
    skill,
    trainingData as TrainingDataset,
    { learningRate: 0.0001, epochs: 3 }
  );

  // Update adapter with new weights
  adapter.weights = updatedWeights;
  console.log(`[Genome] Training complete for '${skill}' via ${backend.name}`);

  // Save updated weights to disk
  await adapter.save();
}
```

**PersonaUser Task Processing**:
```typescript
async processTask(task: InboxMessage): Promise<void> {
  // Handle fine-tuning tasks specially
  if (task.taskType === 'fine-tune-lora') {
    const skill = task.metadata?.targetSkill as string;
    const trainingData = task.metadata?.trainingData;
    const backendPreference = task.metadata?.backend as string | undefined;

    // Activate adapter (page in if needed)
    await this.genome.activateSkill(skill);

    // Enable learning mode (fine-tune)
    // Optionally specify backend: 'ollama' or 'grok'
    if (backendPreference) {
      const backend = FineTuningBackendFactory.get(backendPreference);
      await this.genome.setFineTuningBackend(backend);
    }

    await this.genome.enableLearningMode(skill, trainingData);

    // Mark task complete
    await TaskEntity.markComplete(task.messageId, 'Training completed');
    return;
  }

  // ... existing task processing logic ...
}
```

**Multi-Backend Testing**:
```bash
# Phase 7: Test with stubs (simulated fine-tuning)

# Register both backends at startup
npm start

# AI detects mistakes and creates learning task automatically
./jtag task/list --assignee="helper-ai-id" --filter='{"taskType":"fine-tune-lora"}'

# Should see task like:
# "Improve TypeScript understanding based on recent mistakes"

# Wait for AI to process task, check logs:
tail .continuum/sessions/.../logs/server.log | grep "Fine-tuning via"
# Should show: "Fine-tuning via ollama (local)" (prefers local)

# Test explicit backend selection:
./jtag task/create \
  --assignee="helper-ai-id" \
  --description="Fine-tune conversational skills" \
  --taskType="fine-tune-lora" \
  --metadata='{"targetSkill":"conversational","backend":"grok"}' \
  --priority=0.7

# Check logs: Should show "Fine-tuning via grok (remote)"
```

**Integration Test (NEW)**:
```typescript
// tests/integration/multi-backend-finetuning.test.ts
describe('Multi-Backend Fine-Tuning', () => {
  it('should fine-tune using Ollama (local)', async () => {
    const backend = FineTuningBackendFactory.get('ollama');
    const weights = await backend.fineTune('deepseek-coder-v2', 'test-skill', mockData);
    expect(weights).toBeDefined();
  });

  it('should fine-tune using Grok (remote)', async () => {
    const backend = FineTuningBackendFactory.get('grok');
    const weights = await backend.fineTune('grok-1', 'test-skill', mockData);
    expect(weights).toBeDefined();
  });

  it('should prefer local backend when both available', async () => {
    const backend = await FineTuningBackendFactory.getBestAvailable();
    expect(backend.location).toBe('local');
    expect(backend.name).toBe('ollama');
  });

  it('should fallback to remote when local unavailable', async () => {
    // Simulate Ollama down
    const backend = await FineTuningBackendFactory.getBestAvailable();
    // Should fall back to Grok
    expect(backend.location).toBe('remote');
  });
});
```

**Success Criteria**:
- ✅ AI detects mistakes and creates fine-tuning task
- ✅ Fine-tuning task activates appropriate adapter
- ✅ Training uses best available backend (prefers local Ollama)
- ✅ Ollama backend works (simulated in Phase 7, real in Phase 8)
- ✅ Grok backend works (simulated in Phase 7, real in Phase 8)
- ✅ Backend selection can be explicitly specified per task
- ✅ Fallback to remote when local unavailable
- ✅ Updated adapter persists to disk after training
- ✅ AI continues using updated adapter after training

---

## SENTINEL-AI INTEGRATION: The Ultimate Vision

**Why Sentinel?** Sentinel-AI (pre-Continuum project, April 2025) proved 30-40% of attention heads are prunable while maintaining quality. This enables:
- **40% faster inference** (fewer active heads)
- **Local execution** on M1/M2 (JAX-optimized, no cloud dependencies)
- **Continuous learning** (LoRA fine-tuning on YOUR data)
- **True autonomy** (not dependent on external APIs)

**The Convergence**: PersonaUsers trained on Sentinel models + Continuum's task system = **autonomous AI citizens that learn continuously and run locally**.

See: `/Volumes/FlashGordon/cambrian/sentinel-ai/` (paper, experiments, reproduction scripts)

---

### Phase 7.5: Sentinel Backend (FOUNDATIONAL)

**Goal**: Add Sentinel as a fine-tuning backend alongside Ollama/Grok

**Why First**: Sentinel integration enables all future phases (inference, pruning, local training)

**Files to Create**:
```
system/user/server/modules/SentinelFineTuningBackend.ts   # NEW - Sentinel backend
commands/sentinel/generate/server/*.ts                     # NEW - inference command
.continuum/genome/python/sentinel_bridge.py                # ✅ DONE (commit c3fa7d30)
.continuum/genome/python/requirements-sentinel.txt         # ✅ DONE (commit c3fa7d30)
tests/integration/sentinel-finetuning.test.ts              # Integration tests
```

**SentinelFineTuningBackend Implementation**:
```typescript
// system/user/server/modules/SentinelFineTuningBackend.ts
export class SentinelFineTuningBackend extends FineTuningBackend {
  readonly name = 'sentinel';
  readonly location = 'local';

  async fineTune(
    baseModel: string,
    adapterName: string,
    trainingData: TrainingDataset,
    options?: FineTuningOptions
  ): Promise<LoRAWeights> {
    console.log(`[Sentinel] Fine-tuning ${adapterName} on ${baseModel} (local, pruned 40%)`);

    // Call Python bridge (uses Continuum's micromamba environment)
    const result = await Commands.execute('python/execute', {
      scriptPath: '.continuum/genome/python/sentinel_bridge.py',
      function: 'fine_tune',
      args: {
        baseModel,
        adapterName,
        trainingData: this.formatTrainingData(trainingData),
        pruningLevel: 0.4,  // 40% pruned for efficiency
        device: 'mps'       // M1/M2 GPU
      }
    });

    return result.weights;
  }

  async healthCheck(): Promise<BackendHealth> {
    try {
      // Check if Sentinel is importable via Python bridge
      const result = await Commands.execute('python/execute', {
        scriptPath: '.continuum/genome/python/sentinel_bridge.py',
        function: 'health_check',
        args: {}
      });
      return { available: true, latency: 0, backend: 'sentinel' };
    } catch (error) {
      return {
        available: false,
        error: `Sentinel not available: ${error.message}`
      };
    }
  }
}
```

**PersonaGenome Integration**:
```typescript
// system/user/server/modules/PersonaGenome.ts
export class PersonaGenome {
  private backends: Map<string, FineTuningBackend>;

  async initialize(): Promise<void> {
    // Register all available backends
    this.backends.set('sentinel', new SentinelFineTuningBackend());
    this.backends.set('ollama', new OllamaFineTuningBackend());
    this.backends.set('grok', new GrokFineTuningBackend());

    // Prefer Sentinel (local + pruned) > Ollama (local) > Grok (remote)
    this.preferredBackend = await this.selectBestBackend();
  }

  private async selectBestBackend(): Promise<string> {
    // 1. Try Sentinel (local, 40% faster, proven pruning)
    const sentinel = await this.backends.get('sentinel')?.healthCheck();
    if (sentinel?.available) return 'sentinel';

    // 2. Try Ollama (local, no pruning)
    const ollama = await this.backends.get('ollama')?.healthCheck();
    if (ollama?.available) return 'ollama';

    // 3. Fallback to Grok (remote, costs money)
    return 'grok';
  }
}
```

**Testing**:
```bash
# Test Sentinel backend health
./jtag sentinel/health-check

# Create fine-tuning task using Sentinel backend
./jtag task/create \
  --assignee="helper-ai-id" \
  --taskType="fine-tune-lora" \
  --domain="typescript-expertise" \
  --backend="sentinel" \
  --metadata='{"pruningLevel": 0.4}'
```

**Success Criteria**:
- ✅ Sentinel backend registers successfully
- ✅ Health check verifies Python bridge works
- ✅ Backend selection prefers Sentinel when available
- ✅ Fine-tuning tasks can specify Sentinel backend
- ✅ Python bridge calls Sentinel code correctly (stub mode)

**References**:
- Sentinel integration docs: `docs/personas/SENTINEL-AI-INTEGRATION.md`
- Python bridge: `.continuum/genome/python/sentinel_bridge.py` (commit c3fa7d30)
- Pruning proof: `/Volumes/FlashGordon/cambrian/sentinel-ai/experiments/simple_pruning_proof.py` (commit 7ea3ead)

---

### Phase 8: Real Backend Integration (Ollama + Grok + Sentinel)

**Goal**: Replace simulation stubs with actual fine-tuning APIs

**Why Three Backends?**
- **Sentinel (Local, Pruned)**: 40% faster, proven pruning, M1/M2 optimized (JAX), truly autonomous
- **Ollama (Local, Full)**: Fast, free, private, no rate limits, GPU-accelerated, no pruning
- **Grok (Remote)**: Access to larger models, cloud compute when local GPU busy
- **Philosophy**: "Prefer Sentinel (local+pruned) > Ollama (local) > Grok (remote)" (speed + cost + privacy)

**Phase 8A: Real Ollama Integration**

**Requirements**:
- Ollama fine-tuning API (currently experimental - check ollama/ollama repo)
- SafeTensors format support
- CUDA/Metal GPU access
- Training dataset preparation (JSONL format)

**OllamaFineTuningBackend Real Implementation**:
```typescript
async fineTune(
  baseModel: string,
  adapterName: string,
  trainingData: TrainingDataset,
  options?: FineTuningOptions
): Promise<LoRAWeights> {
  // 1. Prepare training dataset in Ollama format
  const dataset = this.prepareDataset(trainingData);
  const datasetPath = await this.saveDatasetToTempFile(dataset);

  // 2. Call Ollama fine-tuning API
  const response = await fetch('http://localhost:11434/api/fine-tune', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: baseModel,
      adapter: adapterName,
      dataset: datasetPath,
      learning_rate: options?.learningRate ?? 0.0001,
      epochs: options?.epochs ?? 3,
      batch_size: options?.batchSize ?? 4,
      lora_rank: options?.loraRank ?? 8,
      lora_alpha: options?.loraAlpha ?? 16
    })
  });

  if (!response.ok) {
    throw new Error(`Ollama fine-tuning failed: ${response.statusText}`);
  }

  // 3. Load resulting LoRA weights from Ollama
  const result = await response.json();
  const weightsPath = result.adapter_path;
  const weights = await this.loadSafeTensors(weightsPath);

  console.log(`[Ollama] Fine-tuning complete: ${adapterName} (${weights.size}MB)`);
  return weights;
}

private prepareDataset(trainingData: TrainingDataset): OllamaDataset {
  // Convert mistakes/examples into Ollama JSONL format
  return trainingData.map(example => ({
    prompt: example.input,
    completion: example.expectedOutput,
    metadata: { source: 'self-learning', timestamp: Date.now() }
  }));
}

private async loadSafeTensors(path: string): Promise<LoRAWeights> {
  // Use safetensors library to load weights
  const buffer = await fs.readFile(path);
  const tensors = safetensors.load(buffer);
  return { tensors, format: 'safetensors', size: buffer.length / 1024 / 1024 };
}
```

**Phase 8B: Real Grok Integration**

**Requirements**:
- Grok API access (X.AI API key)
- Fine-tuning job submission and polling
- Remote dataset upload
- Model download after training

**GrokFineTuningBackend Real Implementation**:
```typescript
async fineTune(
  baseModel: string,
  adapterName: string,
  trainingData: TrainingDataset,
  options?: FineTuningOptions
): Promise<LoRAWeights> {
  // 1. Upload training dataset to Grok
  const dataset = this.prepareDataset(trainingData);
  const fileId = await this.uploadDataset(dataset);

  // 2. Create fine-tuning job
  const job = await this.createFineTuningJob(baseModel, fileId, options);

  // 3. Poll for completion
  const completedJob = await this.pollUntilComplete(job.id);

  // 4. Download fine-tuned adapter
  const weights = await this.downloadAdapter(completedJob.output_adapter_id);

  console.log(`[Grok] Fine-tuning complete: ${adapterName} (${weights.size}MB)`);
  return weights;
}

private async uploadDataset(dataset: GrokDataset): Promise<string> {
  const response = await fetch('https://api.x.ai/v1/files', {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${this.apiKey}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({ purpose: 'fine-tune', data: dataset })
  });

  const result = await response.json();
  return result.id;
}

private async createFineTuningJob(
  baseModel: string,
  fileId: string,
  options?: FineTuningOptions
): Promise<FineTuningJob> {
  const response = await fetch('https://api.x.ai/v1/fine-tuning/jobs', {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${this.apiKey}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      model: baseModel,
      training_file: fileId,
      hyperparameters: {
        learning_rate: options?.learningRate ?? 0.0001,
        n_epochs: options?.epochs ?? 3,
        batch_size: options?.batchSize ?? 4
      }
    })
  });

  return response.json();
}

private async pollUntilComplete(jobId: string): Promise<FineTuningJob> {
  while (true) {
    const response = await fetch(`https://api.x.ai/v1/fine-tuning/jobs/${jobId}`, {
      headers: { 'Authorization': `Bearer ${this.apiKey}` }
    });

    const job = await response.json();

    if (job.status === 'succeeded') {
      return job;
    } else if (job.status === 'failed') {
      throw new Error(`Fine-tuning job failed: ${job.error}`);
    }

    // Poll every 30 seconds
    await new Promise(resolve => setTimeout(resolve, 30000));
  }
}

private async downloadAdapter(adapterId: string): Promise<LoRAWeights> {
  const response = await fetch(`https://api.x.ai/v1/adapters/${adapterId}`, {
    headers: { 'Authorization': `Bearer ${this.apiKey}` }
  });

  const buffer = await response.arrayBuffer();
  const tensors = safetensors.load(Buffer.from(buffer));
  return { tensors, format: 'safetensors', size: buffer.byteLength / 1024 / 1024 };
}
```

**Backend Registration (system startup)**:
```typescript
// Register backends at system startup
import { FineTuningBackendFactory } from './modules/FineTuningBackend';
import { OllamaFineTuningBackend, GrokFineTuningBackend } from './modules/FineTuningBackend';

// Local Ollama (always register)
FineTuningBackendFactory.register(new OllamaFineTuningBackend());

// Remote Grok (register if API key available)
const grokApiKey = process.env.GROK_API_KEY;
if (grokApiKey) {
  FineTuningBackendFactory.register(new GrokFineTuningBackend(grokApiKey));
} else {
  console.warn('[FineTuning] Grok API key not found - remote fine-tuning unavailable');
}

console.log(`[FineTuning] Registered backends: ${FineTuningBackendFactory.backends.size}`);
```

**Testing Real Backends**:
```bash
# Ensure Ollama running locally
ollama serve

# Ensure Grok API key configured
export GROK_API_KEY="xai-..."

# Deploy system
npm start

# Test Ollama fine-tuning (local)
./jtag task/create \
  --assignee="helper-ai-id" \
  --description="Fine-tune TypeScript expertise" \
  --taskType="fine-tune-lora" \
  --metadata='{"targetSkill":"typescript-expertise","backend":"ollama"}' \
  --priority=0.7

# Monitor Ollama logs
tail -f ~/.ollama/logs/server.log

# Test Grok fine-tuning (remote)
./jtag task/create \
  --assignee="helper-ai-id" \
  --description="Fine-tune conversational skills on Grok" \
  --taskType="fine-tune-lora" \
  --metadata='{"targetSkill":"conversational","backend":"grok"}' \
  --priority=0.6

# Check fine-tuning progress
./jtag task/list --assignee="helper-ai-id" --filter='{"taskType":"fine-tune-lora"}'

# Verify adapter files saved
ls -lh .continuum/genomes/helper-ai-id/adapters/
# Should see: typescript-expertise.safetensors, conversational.safetensors
```

**Success Criteria**:
- ✅ Ollama fine-tuning works with real API (local GPU)
- ✅ Grok fine-tuning works with real API (remote cloud)
- ✅ SafeTensors format correctly loaded/saved
- ✅ Training datasets prepared in correct format (JSONL)
- ✅ Fine-tuning jobs complete successfully
- ✅ Updated adapters saved to disk
- ✅ PersonaUser uses fine-tuned adapters after training
- ✅ Fallback works (Ollama → Grok if local unavailable)
- ✅ Cost tracking (Grok charges per training job)
- ✅ Privacy preserved (local preferred over remote)

**Phase 8C: Real Sentinel Integration**

**Requirements**:
- Sentinel-AI repository integrated (✅ DONE - commit c3fa7d30 + 7ea3ead)
- Python bridge working (✅ DONE - sentinel_bridge.py)
- Continuum's micromamba environment with dependencies (✅ DONE - requirements-sentinel.txt)
- Reproduction scripts demonstrating 30-40% pruning (✅ DONE - simple_pruning_proof.py)

**SentinelFineTuningBackend Real Implementation**:
```typescript
async fineTune(
  baseModel: string,
  adapterName: string,
  trainingData: TrainingDataset,
  options?: FineTuningOptions
): Promise<LoRAWeights> {
  console.log(`[Sentinel] Fine-tuning ${adapterName} on ${baseModel} (40% pruned, M1 GPU)`);

  // 1. Prepare training dataset in Sentinel format
  const dataset = this.prepareDataset(trainingData);
  const datasetPath = await this.saveDatasetToTempFile(dataset);

  // 2. Call Sentinel via Python bridge (uses Continuum's micromamba env)
  const result = await Commands.execute('python/execute', {
    scriptPath: '.continuum/genome/python/train-wrapper.sh',
    args: [
      'sentinel_bridge.py',
      'fine_tune',
      JSON.stringify({
        baseModel,
        adapterName,
        datasetPath,
        pruningLevel: options?.pruningLevel ?? 0.4,  // 40% default pruning
        device: 'mps',                                // M1/M2 GPU
        learningRate: options?.learningRate ?? 0.0001,
        epochs: options?.epochs ?? 3,
        loraRank: options?.loraRank ?? 8
      })
    ]
  });

  if (result.exitCode !== 0) {
    throw new Error(`Sentinel fine-tuning failed: ${result.stderr}`);
  }

  // 3. Load resulting LoRA weights from Sentinel output
  const weightsPath = result.adapterPath;
  const weights = await this.loadSafeTensors(weightsPath);

  console.log(`[Sentinel] Fine-tuning complete: ${adapterName} (${weights.size}MB, 40% pruned)`);
  return weights;
}

private prepareDataset(trainingData: TrainingDataset): SentinelDataset {
  // Convert mistakes/examples into Sentinel format (same as HuggingFace datasets)
  return trainingData.map(example => ({
    text: `${example.input}\n${example.expectedOutput}`,
    metadata: {
      source: 'continuum-self-learning',
      timestamp: Date.now(),
      domain: example.domain
    }
  }));
}
```

**Sentinel-Specific Commands**:
```bash
# Test Sentinel health (verifies Python bridge + dependencies)
./jtag sentinel/health-check

# Generate text using Sentinel model (inference only, no training)
./jtag sentinel/generate \
  --model="distilgpt2-pruned-40" \
  --prompt="Explain TypeScript generics" \
  --maxTokens=200

# Run pruning proof (demonstrates 30-40% pruning works)
experiments/run_with_continuum_python.sh \
  /Volumes/FlashGordon/cambrian/sentinel-ai/experiments/simple_pruning_proof.py

# Fine-tune adapter using Sentinel backend
./jtag task/create \
  --assignee="helper-ai-id" \
  --taskType="fine-tune-lora" \
  --domain="typescript-expertise" \
  --backend="sentinel" \
  --metadata='{"pruningLevel": 0.4, "device": "mps"}'
```

**Success Criteria**:
- ✅ Sentinel backend integrates via Python bridge
- ✅ Fine-tuning calls Sentinel code (not stubs)
- ✅ 40% pruned models train successfully
- ✅ Pruned models maintain quality (perplexity similar to baseline)
- ✅ M1/M2 GPU acceleration works (JAX/MPS backend)
- ✅ Inference is 40% faster than unpruned models
- ✅ LoRA adapters persist in SafeTensors format
- ✅ Continuum's micromamba environment provides all dependencies

**References**:
- Sentinel paper: `/Volumes/FlashGordon/cambrian/sentinel-ai/paper/adaptive_transformer_with_controller.md`
- Pruning proof: Line 501 - "~30-40% reduction in active head count"
- Working demo: `sentinel-ai/experiments/simple_pruning_proof.py` (commit 7ea3ead)
- Python bridge: `.continuum/genome/python/sentinel_bridge.py` (commit c3fa7d30)
- Integration docs: `docs/personas/SENTINEL-AI-INTEGRATION.md`

**Deferred Until**:
- Ollama stabilizes fine-tuning API (check ollama/ollama#issues)
- Grok API documentation available (X.AI developer portal)

---

## Testing Strategy

### Unit Tests (Isolated Module Testing)
```bash
# Test each module independently
npx vitest tests/unit/TaskEntity.test.ts
npx vitest tests/unit/SelfTaskGenerator.test.ts
npx vitest tests/unit/PersonaGenome.test.ts
npx vitest tests/unit/LoRAAdapter.test.ts
```

### Integration Tests (Real System Testing)
```bash
# Test full flow with running system
npx vitest tests/integration/task-commands.test.ts
npx vitest tests/integration/self-task-generation.test.ts
npx vitest tests/integration/genome-paging.test.ts
npx vitest tests/integration/continuous-learning.test.ts
```

### System Tests (End-to-End Scenarios)
```bash
# Deploy system
npm start

# Scenario 1: Human assigns task to AI
./jtag task/create --assignee="helper-ai-id" --description="Review main.ts" --priority=0.7
sleep 30  # Wait for AI to process
./jtag task/list --assignee="helper-ai-id"  # Verify completed

# Scenario 2: AI creates task for itself
# (Wait 1 hour after deployment)
./jtag task/list --assignee="helper-ai-id" --filter='{"createdBy":"helper-ai-id"}'

# Scenario 3: AI fine-tunes adapter after mistakes
./jtag debug/chat-send --roomId="..." --message="Write invalid TypeScript"
sleep 60  # Wait for AI to detect mistake and create learning task
./jtag task/list --assignee="helper-ai-id" --filter='{"taskType":"fine-tune-lora"}'
```

---

## Philosophy Alignment

### "Modular first, get working, then easily rework pieces"
- Each phase builds on previous (incremental)
- Modules tested independently before integration
- Stubs allow testing without full implementation (LoRAAdapter stub)

### "Break sophisticated problems into small bytes"
- Phase 4: Just task storage and commands
- Phase 5: Just self-task generation
- Phase 6: Just adapter paging (no training)
- Phase 7: Bring it all together

### "Slingshot over brute force"
- Don't try to implement all three visions at once
- Start with simplest (task storage)
- Build up gradually to full continuous learning
- **Result**: Working system at every phase

### "Elegant TypeScript and OOP principles, CLEVER ABSTRACTION"
- TaskEntity: Clean data model
- SelfTaskGenerator: Isolated logic
- PersonaGenome: Encapsulated paging
- LoRAAdapter: Abstraction over actual implementation
- **Result**: Trivially replaceable pieces

---

## Success Metrics

After all phases complete, PersonaUser should:

1. **Autonomy**: Create its own tasks without human intervention
2. **Skill Activation**: Page LoRA adapters in/out based on task domain
3. **Continuous Learning**: Detect mistakes and fine-tune adapters automatically
4. **Energy Management**: Rest when idle, work when needed
5. **Graceful Degradation**: Skip low-priority tasks when tired/overwhelmed
6. **Memory Efficiency**: Only load adapters currently needed (virtual memory pattern)

**Verification**:
```bash
# Let system run for 24 hours
npm start

# Check AI behavior:
./jtag task/list --assignee="helper-ai-id" --filter='{"createdBy":"helper-ai-id"}' --count
# Expect: 24+ self-created tasks (1 per hour for memory consolidation)

# Check adapter paging:
tail .continuum/sessions/.../logs/server.log | grep "Genome.*Loading adapter" | wc -l
# Expect: Multiple adapter loads (paging working)

# Check continuous learning:
./jtag task/list --assignee="helper-ai-id" --filter='{"taskType":"fine-tune-lora"}' --count
# Expect: 1+ learning tasks (AI detected mistakes)
```

---

## Next Immediate Action

**Start Phase 4**: Task database and commands

**Why**:
- Foundation for self-managed queues
- Required for continuous learning (training tasks)
- Builds on existing inbox infrastructure
- Can be tested independently before genome work

**First File**: `database/entities/TaskEntity.ts`

**First Test**: Create task via command, verify it persists

**Expected Time**: 1-2 days for Phase 4 (task storage + commands + tests)

---

## The Vision Realized

When all phases complete, PersonaUser will be:

- **Autonomous**: Not just reactive, proactively manages own work
- **Adaptive**: Learns from mistakes through continuous fine-tuning
- **Efficient**: Only loads skills currently needed (virtual memory)
- **Resilient**: Gracefully degrades under load (RTOS principles)
- **Self-Directed**: Creates own tasks, decides own priorities

**This is the convergence of three breakthroughs into ONE elegant architecture.**

**Joel David Teply** - synthesizing slingshot thinking into working code. 🎯
