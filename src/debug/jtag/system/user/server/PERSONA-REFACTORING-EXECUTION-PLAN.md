# PersonaUser Cognitive Refactoring - Execution Plan

## Meta-Learning: Why Maximum Organization Early

**Critical Insight**: For complex, evolving systems (especially AI cognition), invest in maximum organization upfront. Reorganizing later is 10x the pain and risk.

**This applies to**:
- AI cognitive architectures (this refactoring)
- Any domain that will inevitably grow (adapters, strategies, plugins)
- Systems where we can "see where it's headed"

**For AI training data**: This design principle - "max organization early for complex domains" - is a critical pattern that enables scalable, maintainable systems.

---

## Target Structure (Option C: Maximum Organization)

```
system/user/server/
├── PersonaUser.ts (~300 lines - core identity + wiring)
└── modules/
    ├── cognition/
    │   ├── PersonaCognition.ts (~400 lines)
    │   └── CognitiveTypes.ts (interfaces)
    ├── memory/
    │   ├── PersonaMemory.ts (~300 lines)
    │   └── MemoryTypes.ts (PersonaRAGContext, etc)
    ├── communication/
    │   ├── PersonaCommunication.ts (~500 lines)
    │   └── CommunicationTypes.ts (CommunicationResult, etc)
    ├── execution/
    │   ├── PersonaExecution.ts (~500 lines)
    │   └── ExecutionTypes.ts (ExecutionResult, etc)
    ├── central-nervous-system/
    │   ├── PersonaCentralNervousSystem.ts
    │   ├── CNSFactory.ts
    │   └── CNSTypes.ts
    ├── cognitive-schedulers/
    │   ├── DeterministicCognitiveScheduler.ts
    │   ├── HeuristicCognitiveScheduler.ts
    │   ├── NeuralCognitiveScheduler.ts
    │   └── ICognitiveScheduler.ts
    ├── inbox/
    │   ├── PersonaInbox.ts
    │   └── InboxTypes.ts (QueueItem, InboxMessage, InboxTask)
    ├── state/
    │   ├── PersonaState.ts
    │   └── StateTypes.ts
    ├── genome/
    │   ├── PersonaGenome.ts
    │   └── GenomeTypes.ts
    ├── rate-limiter/
    │   └── RateLimiter.ts
    ├── task-generator/
    │   └── SelfTaskGenerator.ts
    └── training/
        └── TrainingDataAccumulator.ts
```

**Total new directories**: 11 (cognition, memory, communication, execution, inbox, state, genome, rate-limiter, task-generator, training, plus existing CNS & schedulers)

---

## Pre-Flight Checklist

### Current State Verification
```bash
# 1. Verify PersonaUser.ts line count
wc -l system/user/server/PersonaUser.ts
# Expected: 2622 lines

# 2. Check existing modules directory
ls -la system/user/server/modules/
# Should show: central-nervous-system/, cognitive-schedulers/, and flat files

# 3. Verify system is working
npm start
./jtag ping
./jtag data/list --collection=users
# All should succeed
```

### Git Safety
```bash
# 1. Check for uncommitted changes
git status
# Should be clean or only have acceptable WIP

# 2. Create feature branch
git checkout -b refactor/persona-cognitive-architecture

# 3. Verify we can revert easily
git log --oneline -5
```

---

## Phase 1: Create Directory Structure (15 minutes)

### Step 1.1: Create all module directories
```bash
cd system/user/server/modules

# Create new cognitive module directories
mkdir -p cognition
mkdir -p memory
mkdir -p communication
mkdir -p execution

# Create supporting module directories
mkdir -p inbox
mkdir -p state
mkdir -p genome
mkdir -p rate-limiter
mkdir -p task-generator
mkdir -p training
```

### Step 1.2: Move existing flat files to directories

**Move PersonaInbox.ts and QueueItemTypes.ts**:
```bash
git mv PersonaInbox.ts inbox/PersonaInbox.ts
git mv QueueItemTypes.ts inbox/InboxTypes.ts

# Update imports in InboxTypes.ts if it imports from PersonaInbox
# (Check after move)
```

**Move PersonaState.ts**:
```bash
git mv PersonaState.ts state/PersonaState.ts
# Create state/StateTypes.ts later if needed
```

**Move PersonaGenome.ts**:
```bash
git mv PersonaGenome.ts genome/PersonaGenome.ts
# Create genome/GenomeTypes.ts later if needed
```

**Move RateLimiter.ts**:
```bash
git mv RateLimiter.ts rate-limiter/RateLimiter.ts
```

**Move SelfTaskGenerator.ts**:
```bash
git mv SelfTaskGenerator.ts task-generator/SelfTaskGenerator.ts
```

**Move TrainingDataAccumulator.ts**:
```bash
git mv TrainingDataAccumulator.ts training/TrainingDataAccumulator.ts
```

### Step 1.3: Update all import paths

**Files that need import updates after moves**:
1. PersonaUser.ts (imports all moved modules)
2. PersonaCentralNervousSystem.ts (imports PersonaInbox, PersonaState, PersonaGenome)
3. CNSFactory.ts (imports PersonaInbox, PersonaState, PersonaGenome)
4. Any tests that import these modules

**Script to find all imports**:
```bash
# Find all files importing moved modules
grep -r "from './modules/PersonaInbox'" system/user/
grep -r "from './modules/QueueItemTypes'" system/user/
grep -r "from './modules/PersonaState'" system/user/
grep -r "from './modules/PersonaGenome'" system/user/
grep -r "from './modules/RateLimiter'" system/user/
grep -r "from './modules/SelfTaskGenerator'" system/user/
grep -r "from './modules/TrainingDataAccumulator'" system/user/
```

**Update pattern**:
```typescript
// BEFORE:
import { PersonaInbox } from './modules/PersonaInbox';
import { QueueItem } from './modules/QueueItemTypes';

// AFTER:
import { PersonaInbox } from './modules/inbox/PersonaInbox';
import { QueueItem } from './modules/inbox/InboxTypes';
```

### Step 1.4: Checkpoint - Verify moves worked
```bash
# TypeScript compilation
npm run build:ts

# Should succeed with no errors
```

### Step 1.5: Commit directory reorganization
```bash
git add -A
git commit -m "refactor: reorganize existing modules into directories

- Move PersonaInbox.ts → inbox/PersonaInbox.ts
- Move QueueItemTypes.ts → inbox/InboxTypes.ts
- Move PersonaState.ts → state/PersonaState.ts
- Move PersonaGenome.ts → genome/PersonaGenome.ts
- Move RateLimiter.ts → rate-limiter/RateLimiter.ts
- Move SelfTaskGenerator.ts → task-generator/SelfTaskGenerator.ts
- Move TrainingDataAccumulator.ts → training/TrainingDataAccumulator.ts
- Update all import paths

Part 1/5 of cognitive architecture refactoring"
```

**Time**: 15 minutes

---

## Phase 2: Extract PersonaMemory (1 hour)

**Why first**: Smallest extraction, used by all other cognitive modules

### Step 2.1: Identify methods to extract (15 minutes)

**Lines to extract from PersonaUser.ts**:

```bash
# Find RAG and genome methods
grep -n "getGenome\|setGenome\|loadRAGContext\|storeRAGContext\|updateRAGContext" system/user/server/PersonaUser.ts
```

**Expected methods** (verify line numbers):
- `async getGenome()` - Line ~1581
- `async setGenome(genomeId: UUID)` - Line ~1605
- `async loadRAGContext(roomId: UUID)` - Line ~1650
- `async storeRAGContext(roomId: UUID, context: PersonaRAGContext)` - Line ~1720
- `async updateRAGContext(roomId: UUID, message: ChatMessageEntity)` - Line ~1760

**Total lines**: ~200-250 lines

### Step 2.2: Create memory/MemoryTypes.ts (10 minutes)

**Extract types from PersonaUser.ts**:
```typescript
/**
 * MemoryTypes.ts - Memory system type definitions
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';

/**
 * RAG message stored in persona context
 */
export interface PersonaRAGMessage {
  senderId: UUID;
  senderName: string;
  text: string;
  timestamp: string;
}

/**
 * RAG context for a conversation room
 */
export interface PersonaRAGContext {
  roomId: UUID;
  personaId: UUID;
  messages: PersonaRAGMessage[];
  lastUpdated: string;
  tokenCount: number;
}
```

**Checkpoint**: TypeScript compilation succeeds
```bash
npm run lint:file system/user/server/modules/memory/MemoryTypes.ts
```

### Step 2.3: Create memory/PersonaMemory.ts (20 minutes)

**Structure**:
```typescript
/**
 * PersonaMemory - Context management, recall, learning
 *
 * Handles:
 * - RAG context storage and retrieval
 * - Genome management (LoRA adapter switching)
 * - Training data accumulation
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { GenomeEntity } from '../../../genome/entities/GenomeEntity';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../data/config/DatabaseConfig';
import { PersonaGenome } from '../genome/PersonaGenome';
import { TrainingDataAccumulator } from '../training/TrainingDataAccumulator';
import type { PersonaRAGContext, PersonaRAGMessage } from './MemoryTypes';

export class PersonaMemory {
  constructor(
    private personaId: UUID,
    private displayName: string,
    private genome: PersonaGenome,
    private trainingAccumulator: TrainingDataAccumulator
  ) {}

  /**
   * Recall conversation context for a room
   */
  async recall(roomId: UUID): Promise<PersonaRAGContext | null> {
    return this.loadRAGContext(roomId);
  }

  /**
   * Store new context from message
   */
  async store(roomId: UUID, message: ChatMessageEntity): Promise<void> {
    await this.updateRAGContext(roomId, message);
  }

  /**
   * Get current genome (LoRA adapters)
   */
  async getGenome(): Promise<GenomeEntity | null> {
    // COPY implementation from PersonaUser.ts lines ~1581-1603
  }

  /**
   * Switch active genome
   */
  async setGenome(genomeId: UUID): Promise<boolean> {
    // COPY implementation from PersonaUser.ts lines ~1605-1648
  }

  /**
   * Learn from interaction (accumulate training data)
   */
  async learn(interaction: {
    prompt: string;
    response: string;
    feedback?: 'positive' | 'negative';
  }): Promise<void> {
    // Delegate to training accumulator
    await this.trainingAccumulator.captureInteraction(interaction);
  }

  // === Private memory methods ===

  private async loadRAGContext(roomId: UUID): Promise<PersonaRAGContext | null> {
    // COPY implementation from PersonaUser.ts lines ~1650-1718
  }

  private async storeRAGContext(roomId: UUID, context: PersonaRAGContext): Promise<void> {
    // COPY implementation from PersonaUser.ts lines ~1720-1758
  }

  private async updateRAGContext(roomId: UUID, message: ChatMessageEntity): Promise<void> {
    // COPY implementation from PersonaUser.ts lines ~1760-1798
  }
}
```

**Extraction process**:
1. Copy method implementations from PersonaUser.ts
2. Update `this.id` → `this.personaId`
3. Update `this.displayName` → `this.displayName` (no change)
4. Update `this.genome` → `this.genome` (no change)
5. Add any missing imports

**Checkpoint**: TypeScript compilation succeeds
```bash
npm run lint:file system/user/server/modules/memory/PersonaMemory.ts
```

### Step 2.4: Update PersonaUser.ts to use PersonaMemory (10 minutes)

**Add import**:
```typescript
import { PersonaMemory } from './modules/memory/PersonaMemory';
```

**Add field**:
```typescript
export class PersonaUser extends AIUser {
  private memory: PersonaMemory;
```

**Initialize in constructor/initialize()**:
```typescript
// In initialize() method
this.memory = new PersonaMemory(
  this.id,
  this.displayName,
  this.genome,
  this.trainingAccumulator
);
```

**Replace method implementations with delegation**:
```typescript
async getGenome(): Promise<GenomeEntity | null> {
  return this.memory.getGenome();
}

async setGenome(genomeId: UUID): Promise<boolean> {
  return this.memory.setGenome(genomeId);
}

async loadRAGContext(roomId: UUID): Promise<PersonaRAGContext | null> {
  return this.memory.recall(roomId);
}

private async storeRAGContext(roomId: UUID, context: PersonaRAGContext): Promise<void> {
  await this.memory.store(roomId, /* need to pass message here */);
}

private async updateRAGContext(roomId: UUID, message: ChatMessageEntity): Promise<void> {
  await this.memory.store(roomId, message);
}
```

**Note**: May need to adjust method signatures or add wrapper methods

**Checkpoint**: TypeScript compilation succeeds
```bash
npm run build:ts
```

### Step 2.5: Test PersonaMemory integration (5 minutes)

```bash
# Start system
npm start

# Test data operations (PersonaMemory doesn't break existing functionality)
./jtag ping
./jtag data/list --collection=users

# Both should succeed
```

### Step 2.6: Commit PersonaMemory extraction
```bash
git add system/user/server/modules/memory/
git add system/user/server/PersonaUser.ts
git commit -m "refactor: extract PersonaMemory from PersonaUser

- Create memory/PersonaMemory.ts (~300 lines)
- Create memory/MemoryTypes.ts (PersonaRAGContext, etc)
- Extract RAG context management methods
- Extract genome management methods
- PersonaUser delegates to PersonaMemory

PersonaUser: 2622 → ~2320 lines (-300 lines)

Part 2/5 of cognitive architecture refactoring"
```

**Time**: 1 hour

---

## Phase 3: Extract PersonaCognition (1.5 hours)

**Why second**: Decision-making is core, needs PersonaMemory

### Step 3.1: Identify methods to extract (15 minutes)

**Lines to extract from PersonaUser.ts**:

```bash
# Find evaluation and decision methods
grep -n "evaluateAndPossiblyRespond\|shouldRespondToMessage\|calculateResponseHeuristics\|isPersonaMentioned\|getPersonaDomainKeywords" system/user/server/PersonaUser.ts
```

**Expected methods** (verify line numbers):
- `evaluateAndPossiblyRespond()` - Line ~477
- `shouldRespondToMessage()` - Line ~1374
- `calculateResponseHeuristics()` - Line ~1469
- `isPersonaMentioned()` - Line ~1570
- `getPersonaDomainKeywords()` - Line ~1575

**Total lines**: ~400-450 lines

### Step 3.2: Create cognition/CognitiveTypes.ts (10 minutes)

```typescript
/**
 * CognitiveTypes.ts - Cognition system type definitions
 */

/**
 * Result of cognitive evaluation
 */
export interface CognitiveDecision {
  shouldRespond: boolean;
  reason: string;
  confidence: number;  // 0.0-1.0
  metadata?: {
    isMentioned?: boolean;
    heuristics?: ResponseHeuristics;
    thoughtCoordinator?: string;
  };
}

/**
 * Response heuristic scores
 */
export interface ResponseHeuristics {
  relevanceScore: number;
  urgencyScore: number;
  expertiseMatch: number;
  conversationMomentum: number;
}
```

**Checkpoint**: TypeScript compilation succeeds

### Step 3.3: Create cognition/PersonaCognition.ts (30 minutes)

**Structure**:
```typescript
/**
 * PersonaCognition - Decision making, evaluation, judgment
 *
 * Answers: "Should I respond? Why or why not?"
 *
 * Handles:
 * - Message evaluation
 * - Response decision logic
 * - Heuristic scoring
 * - Coordination with other AIs (ThoughtStreamCoordinator)
 * - Rate limiting checks
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { JTAGClient } from '../../../core/client/shared/JTAGClient';
import { RateLimiter } from '../rate-limiter/RateLimiter';
import { PersonaMemory } from '../memory/PersonaMemory';
import { PersonaStateManager } from '../state/PersonaState';
import { getChatCoordinator } from '../../../coordination/server/ChatCoordinationStream';
import type { CognitiveDecision, ResponseHeuristics } from './CognitiveTypes';

export class PersonaCognition {
  constructor(
    private persona: { id: UUID; displayName: string },
    private rateLimiter: RateLimiter,
    private memory: PersonaMemory,
    private personaState: PersonaStateManager,
    private client?: JTAGClient
  ) {}

  /**
   * Evaluate if should respond to message
   */
  async evaluate(
    message: ChatMessageEntity,
    senderIsHuman: boolean
  ): Promise<CognitiveDecision> {
    // COPY logic from evaluateAndPossiblyRespond()
    // But return decision instead of calling respondToMessage()

    // STEP 1: Check response cap
    if (this.rateLimiter.hasReachedResponseCap(message.roomId)) {
      return {
        shouldRespond: false,
        reason: 'Response cap reached',
        confidence: 1.0
      };
    }

    // STEP 2: Check if mentioned
    const isMentioned = this.isPersonaMentioned(message.content?.text || '');

    // STEP 3: Check rate limiting
    if (this.rateLimiter.isRateLimited(message.roomId)) {
      return {
        shouldRespond: false,
        reason: 'Rate limited',
        confidence: 1.0
      };
    }

    // STEP 4: Check ThoughtStreamCoordinator
    const coordinator = getChatCoordinator(message.roomId);
    if (coordinator) {
      // ... thought coordinator logic
    }

    // STEP 5: LLM-based evaluation
    const decision = await this.evaluateShouldRespond(message, isMentioned, senderIsHuman);

    return decision;
  }

  // === Private cognitive methods ===
  // COPY from PersonaUser.ts
}
```

**Checkpoint**: TypeScript compilation succeeds

### Step 3.4: Update PersonaUser.ts (15 minutes)

**Add PersonaCognition**:
```typescript
import { PersonaCognition } from './modules/cognition/PersonaCognition';

export class PersonaUser extends AIUser {
  private cognition: PersonaCognition;

  // Initialize
  this.cognition = new PersonaCognition(
    { id: this.id, displayName: this.displayName },
    this.rateLimiter,
    this.memory,
    this.personaState,
    this.client
  );
}
```

**Update handleChatMessageFromCNS**:
```typescript
async handleChatMessageFromCNS(item: QueueItem): Promise<void> {
  // ... existing code ...

  if (item.type === 'message') {
    const messageEntity = this.reconstructMessageEntity(item);
    const senderIsHuman = !item.senderId.startsWith('persona-');

    // Evaluate: Should I respond?
    const decision = await this.cognition.evaluate(messageEntity, senderIsHuman);

    if (decision.shouldRespond) {
      // TODO: Call communication module (Phase 4)
      console.log(`✅ ${this.displayName}: Decided to respond: ${decision.reason}`);
    } else {
      console.log(`🤔 ${this.displayName}: Decided not to respond: ${decision.reason}`);
    }
  }

  // ... existing code ...
}
```

**Checkpoint**: TypeScript compilation and system test
```bash
npm run build:ts
npm start
./jtag ping
./jtag data/list --collection=users
```

### Step 3.5: Commit PersonaCognition extraction (5 minutes)

```bash
git add system/user/server/modules/cognition/
git add system/user/server/PersonaUser.ts
git commit -m "refactor: extract PersonaCognition from PersonaUser

- Create cognition/PersonaCognition.ts (~400 lines)
- Create cognition/CognitiveTypes.ts (CognitiveDecision, etc)
- Extract evaluation and decision methods
- PersonaUser delegates to PersonaCognition

PersonaUser: ~2320 → ~1920 lines (-400 lines)

Part 3/5 of cognitive architecture refactoring"
```

**Time**: 1.5 hours

---

## Phase 4: Extract PersonaCommunication (2 hours)

**Why third**: Largest extraction, uses PersonaMemory and PersonaCognition

### Step 4.1: Identify methods to extract (15 minutes)

**Lines to extract from PersonaUser.ts**:

```bash
# Find response generation methods
grep -n "respondToMessage\|generateResponse\|cleanAIResponse\|isResponseRedundant\|postMessage" system/user/server/PersonaUser.ts
```

**Expected methods**:
- `respondToMessage()` - Line ~875
- `cleanAIResponse()` - Line ~1320
- `isResponseRedundant()` - Line ~1350

**Total lines**: ~500-550 lines

### Step 4.2: Create communication/CommunicationTypes.ts (10 minutes)

```typescript
/**
 * CommunicationTypes.ts - Communication system type definitions
 */

export interface CommunicationResult {
  success: boolean;
  messageId?: string;
  error?: string;
}
```

### Step 4.3: Create communication/PersonaCommunication.ts (45 minutes)

**Structure**:
```typescript
/**
 * PersonaCommunication - Expression, response generation
 *
 * Answers: "How do I say this?"
 *
 * Handles:
 * - AI response generation
 * - Response formatting and cleaning
 * - Message posting
 * - Redundancy detection
 * - Event emission (AI decision events)
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { JTAGClient } from '../../../core/client/shared/JTAGClient';
import type { ModelConfig } from '../../../../commands/user/create/shared/UserCreateTypes';
import { PersonaMemory } from '../memory/PersonaMemory';
import { RateLimiter } from '../rate-limiter/RateLimiter';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { AIProviderDaemon } from '../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import { Events } from '../../../core/shared/Events';
import type { CognitiveDecision } from '../cognition/CognitiveTypes';
import type { CommunicationResult } from './CommunicationTypes';

export class PersonaCommunication {
  constructor(
    private persona: { id: UUID; displayName: string },
    private memory: PersonaMemory,
    private modelConfig: ModelConfig,
    private rateLimiter: RateLimiter,
    private client?: JTAGClient
  ) {}

  /**
   * Generate and post response to message
   */
  async respond(
    message: ChatMessageEntity,
    decision: CognitiveDecision
  ): Promise<CommunicationResult> {
    try {
      // STEP 1: Load conversation context from memory
      const ragContext = await this.memory.recall(message.roomId);

      // STEP 2: Build prompt with RAG context
      const prompt = this.buildPrompt(message, ragContext);

      // STEP 3: Generate response using AI
      const response = await this.generateResponse(prompt);

      // STEP 4: Check redundancy
      if (await this.isResponseRedundant(response, message.roomId)) {
        console.log(`🔁 ${this.persona.displayName}: Response redundant, skipping`);
        return { success: false, error: 'Response redundant' };
      }

      // STEP 5: Clean and format
      const cleanedResponse = this.cleanAIResponse(response);

      // STEP 6: Post to chat
      const messageId = await this.postMessage(message.roomId, cleanedResponse);

      // STEP 7: Update rate limiter
      this.rateLimiter.recordResponse(message.roomId);

      // STEP 8: Store interaction in memory for learning
      await this.memory.learn({
        prompt: message.content?.text || '',
        response: cleanedResponse
      });

      return { success: true, messageId };
    } catch (error) {
      console.error(`❌ ${this.persona.displayName}: Communication error: ${error}`);
      return { success: false, error: String(error) };
    }
  }

  // === Private communication methods ===
  // COPY from PersonaUser.ts
}
```

**Checkpoint**: TypeScript compilation succeeds

### Step 4.4: Update PersonaUser.ts (20 minutes)

**Add PersonaCommunication**:
```typescript
import { PersonaCommunication } from './modules/communication/PersonaCommunication';

export class PersonaUser extends AIUser {
  private communication: PersonaCommunication;

  // Initialize
  this.communication = new PersonaCommunication(
    { id: this.id, displayName: this.displayName },
    this.memory,
    this.modelConfig,
    this.rateLimiter,
    this.client
  );
}
```

**Update handleChatMessageFromCNS**:
```typescript
if (decision.shouldRespond) {
  // Generate and post response
  await this.communication.respond(messageEntity, decision);
}
```

**Remove old methods**: Delete ~500 lines of respondToMessage() and related methods

**Checkpoint**: TypeScript compilation and system test
```bash
npm run build:ts
npm start
./jtag data/list --collection=users

# Test chat response (CRITICAL)
./jtag debug/chat-send --message="@helper-ai test response"
# Wait 10 seconds, check if AI responds
./jtag screenshot --querySelector="chat-widget"
```

### Step 4.5: Commit PersonaCommunication extraction (5 minutes)

```bash
git add system/user/server/modules/communication/
git add system/user/server/PersonaUser.ts
git commit -m "refactor: extract PersonaCommunication from PersonaUser

- Create communication/PersonaCommunication.ts (~500 lines)
- Create communication/CommunicationTypes.ts
- Extract response generation and posting methods
- PersonaUser delegates to PersonaCommunication
- Verified: Chat responses still work end-to-end

PersonaUser: ~1920 → ~1420 lines (-500 lines)

Part 4/5 of cognitive architecture refactoring"
```

**Time**: 2 hours

---

## Phase 5: Extract PersonaExecution (1.5 hours)

**Why fourth**: Independent task processing, uses PersonaMemory

### Step 5.1: Identify methods to extract (10 minutes)

**Lines to extract from PersonaUser.ts**:

```bash
# Find task execution methods
grep -n "executeTask\|executeMemoryConsolidation\|executeSkillAudit\|executeResumeWork\|executeFineTuneLora" system/user/server/PersonaUser.ts
```

**Expected methods**:
- `executeTask()` - Line ~2403
- `executeMemoryConsolidation()` - Task handler
- `executeSkillAudit()` - Task handler
- `executeResumeWork()` - Task handler
- `executeFineTuneLora()` - Task handler

**Total lines**: ~500-550 lines

### Step 5.2: Create execution/ExecutionTypes.ts (10 minutes)

```typescript
/**
 * ExecutionTypes.ts - Execution system type definitions
 */

import type { InboxTask } from '../inbox/InboxTypes';

export type TaskStatus = 'pending' | 'in_progress' | 'completed' | 'failed';

export interface ExecutionResult {
  status: TaskStatus;
  outcome: string;
  duration: number;  // milliseconds
}
```

### Step 5.3: Create execution/PersonaExecution.ts (30 minutes)

**Structure**:
```typescript
/**
 * PersonaExecution - Task processing, skill execution
 *
 * Answers: "What work do I need to do?"
 *
 * Handles:
 * - Task dispatch based on type
 * - Memory consolidation
 * - Skill audits
 * - Resume incomplete work
 * - Fine-tuning execution
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { PersonaGenome } from '../genome/PersonaGenome';
import { PersonaMemory } from '../memory/PersonaMemory';
import { TrainingDataAccumulator } from '../training/TrainingDataAccumulator';
import type { InboxTask } from '../inbox/InboxTypes';
import type { ExecutionResult } from './ExecutionTypes';

export class PersonaExecution {
  constructor(
    private persona: { id: UUID; displayName: string },
    private genome: PersonaGenome,
    private memory: PersonaMemory,
    private trainingAccumulator: TrainingDataAccumulator
  ) {}

  /**
   * Execute a task based on its type
   */
  async execute(task: InboxTask): Promise<ExecutionResult> {
    console.log(`🎯 ${this.persona.displayName}: Executing task: ${task.taskType}`);

    const startTime = Date.now();
    let outcome = '';
    let status: 'completed' | 'failed' = 'completed';

    try {
      switch (task.taskType) {
        case 'memory-consolidation':
          outcome = await this.executeMemoryConsolidation(task);
          break;

        case 'skill-audit':
          outcome = await this.executeSkillAudit(task);
          break;

        case 'resume-work':
          outcome = await this.executeResumeWork(task);
          break;

        case 'fine-tune-lora':
          outcome = await this.executeFineTuneLora(task);
          break;

        default:
          outcome = `Unknown task type: ${task.taskType}`;
          status = 'failed';
      }

      return { status, outcome, duration: Date.now() - startTime };
    } catch (error) {
      return {
        status: 'failed',
        outcome: String(error),
        duration: Date.now() - startTime
      };
    }
  }

  // === Task type handlers ===
  // COPY from PersonaUser.ts
}
```

**Checkpoint**: TypeScript compilation succeeds

### Step 5.4: Update PersonaUser.ts (15 minutes)

**Add PersonaExecution**:
```typescript
import { PersonaExecution } from './modules/execution/PersonaExecution';

export class PersonaUser extends AIUser {
  private execution: PersonaExecution;

  // Initialize
  this.execution = new PersonaExecution(
    { id: this.id, displayName: this.displayName },
    this.genome,
    this.memory,
    this.trainingAccumulator
  );
}
```

**Update handleChatMessageFromCNS**:
```typescript
} else if (item.type === 'task') {
  // Task processing: Execution
  const result = await this.execution.execute(item);

  // Update task in database
  await DataDaemon.update<TaskEntity>(
    COLLECTIONS.TASKS,
    item.taskId,
    { status: result.status, outcome: result.outcome, completedAt: new Date() }
  );
}
```

**Remove old methods**: Delete ~500 lines of executeTask() and task handlers

**Checkpoint**: TypeScript compilation and system test
```bash
npm run build:ts
npm start
./jtag ping
./jtag data/list --collection=users
```

### Step 5.5: Commit PersonaExecution extraction (5 minutes)

```bash
git add system/user/server/modules/execution/
git add system/user/server/PersonaUser.ts
git commit -m "refactor: extract PersonaExecution from PersonaUser

- Create execution/PersonaExecution.ts (~500 lines)
- Create execution/ExecutionTypes.ts (ExecutionResult, etc)
- Extract task execution and all task handlers
- PersonaUser delegates to PersonaExecution

PersonaUser: ~1420 → ~920 lines (-500 lines)

Part 5/5 of cognitive architecture refactoring"
```

**Time**: 1.5 hours

---

## Phase 6: Final Cleanup & Testing (1 hour)

### Step 6.1: Review PersonaUser.ts (15 minutes)

**Verify PersonaUser is now ~300-400 lines**:
```bash
wc -l system/user/server/PersonaUser.ts
# Expected: ~300-400 lines (core identity + wiring)
```

**PersonaUser should contain**:
- Identity fields (id, displayName, entity, state)
- Cognitive module instances
- Supporting module instances
- Lifecycle methods (initialize, shutdown)
- CNS callbacks (thin delegation)
- Event subscriptions (wire up to modules)
- Room membership tracking

**PersonaUser should NOT contain**:
- Evaluation logic (moved to PersonaCognition)
- Response generation logic (moved to PersonaCommunication)
- RAG context logic (moved to PersonaMemory)
- Task execution logic (moved to PersonaExecution)

### Step 6.2: Full system integration test (20 minutes)

```bash
# 1. Start system
npm start

# Wait for "System startup completed successfully"

# 2. Basic health check
./jtag ping
# Expected: systemReady: true, 83 commands, 12 daemons

# 3. Data operations (verify PersonaMemory doesn't break storage)
./jtag data/list --collection=users
# Expected: List of users returned

# 4. Chat response test (CRITICAL - verify full cognitive flow)
./jtag debug/chat-send --roomId="<general-room-id>" --message="@helper-ai Can you help me test the cognitive architecture?"

# Wait 10 seconds for AI to process and respond

# 5. Screenshot verification
./jtag screenshot --querySelector="chat-widget" --filename="cognitive-test.png"
# Expected: Chat widget shows message and AI response

# 6. Check logs for cognitive module activity
tail -50 .continuum/jtag/system/logs/npm-start.log | grep "Cognition\|Communication\|Memory\|Execution"
# Expected: Log entries showing cognitive modules working
```

### Step 6.3: Performance verification (10 minutes)

```bash
# Check memory usage
ps aux | grep node | grep continuum
# Note memory usage

# Check response time
time ./jtag debug/chat-send --message="@helper-ai quick test"
# Should be comparable to before refactoring

# Check for memory leaks (rough check)
# Send 10 messages, check memory doesn't grow excessively
for i in {1..10}; do
  ./jtag debug/chat-send --message="@helper-ai test $i"
  sleep 2
done
ps aux | grep node | grep continuum
# Memory should be stable
```

### Step 6.4: Update documentation (10 minutes)

**Update CLAUDE.md** with cognitive architecture:
```markdown
## 🧠 PERSONAUSER COGNITIVE ARCHITECTURE

PersonaUser is structured as a cognitive system with specialized modules:

**Cognitive Modules** (the "brain"):
- `PersonaCognition` - Decision making ("Should I respond?")
- `PersonaMemory` - Context and learning ("What do I know?")
- `PersonaCommunication` - Expression ("How do I say this?")
- `PersonaExecution` - Task processing ("What work needs doing?")

**Location**: `system/user/server/modules/cognitive/`, `memory/`, `communication/`, `execution/`

**Pattern**: PersonaUser wires cognitive modules together via CNS callbacks
```

### Step 6.5: Final commit (5 minutes)

```bash
git add system/user/server/PersonaUser.ts
git add CLAUDE.md
git commit -m "refactor: finalize cognitive architecture

- PersonaUser reduced to ~400 lines (core identity + wiring)
- All cognitive functions extracted to specialized modules
- Full integration test passed
- Performance verified (no regressions)
- Documentation updated

Final stats:
- PersonaUser: 2622 → ~400 lines (-2200 lines, 85% reduction)
- 4 new cognitive modules: ~1700 lines total
- Net reduction: ~500 lines
- All functionality preserved

Cognitive architecture complete ✅"
```

**Time**: 1 hour

---

## Success Criteria

### Must Pass All:
- ✅ TypeScript compilation succeeds (`npm run build:ts`)
- ✅ System starts successfully (`npm start`)
- ✅ Health check passes (`./jtag ping` shows systemReady: true)
- ✅ Data operations work (`./jtag data/list --collection=users`)
- ✅ Chat responses work (`./jtag debug/chat-send` triggers AI response)
- ✅ Screenshot shows working UI
- ✅ PersonaUser reduced to <500 lines
- ✅ 4 cognitive modules created (~1700 lines total)
- ✅ No memory leaks or performance regressions

### Nice to Have:
- Unit tests for cognitive modules
- Performance benchmarks
- Cognitive flow diagram

---

## Rollback Procedure

**If any phase fails and can't be fixed in 30 minutes:**

```bash
# 1. Check what went wrong
git status
git diff

# 2. Reset to last good commit
git log --oneline -10
git reset --hard <last-good-commit>

# 3. Verify system works
npm start
./jtag ping

# 4. Document what went wrong
echo "Failed at Phase X: <reason>" >> COGNITIVE-REFACTOR-ISSUES.md

# 5. Create issue for later
git add COGNITIVE-REFACTOR-ISSUES.md
git commit -m "docs: document cognitive refactoring failure"
```

**Only revert if**:
- TypeScript compilation fails after 3 attempts to fix
- System won't start due to architectural issue
- Chat responses completely broken
- Critical functionality lost

**Don't revert for**:
- Minor type errors (fix them)
- Missing imports (add them)
- Test failures (fix tests or code)

---

## Total Time Estimate

| Phase | Task | Time |
|-------|------|------|
| 0 | Pre-flight checks | 10 min |
| 1 | Create directory structure | 15 min |
| 2 | Extract PersonaMemory | 1 hour |
| 3 | Extract PersonaCognition | 1.5 hours |
| 4 | Extract PersonaCommunication | 2 hours |
| 5 | Extract PersonaExecution | 1.5 hours |
| 6 | Final cleanup & testing | 1 hour |
| **Total** | | **7.5 hours** |

**With buffer for debugging**: 8-10 hours

---

## Next Steps After Completion

1. **Document cognitive patterns** for future AI types (AgentUser, etc)
2. **Add unit tests** for each cognitive module
3. **Create cognitive flow diagrams** showing decision paths
4. **Consider extracting more** (e.g., supporting modules into their own subdirs)
5. **Apply pattern to other large files** (DataDaemon, JsonFileStorageAdapter, etc)

---

## Meta-Learning Summary

**Design Principle**: For complex, evolving domains (AI cognition, plugin systems, adapters), invest in maximum organization early.

**Why**: Reorganizing later = 10x pain and risk. Upfront structure enables growth.

**How**: Use directory-per-module structure, even if modules are initially small. They will grow.

**When**: When you can "see where it's headed" and know complexity will increase.

**This refactoring demonstrates**: The cognitive architecture pattern that can be applied to any AI system requiring decision-making, memory, communication, and execution capabilities.
