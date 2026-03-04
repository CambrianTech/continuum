# Architecture Gaps Analysis - Phase 1 Implementation

**Purpose**: Identify what's missing for "AI that answers architecture questions about THIS repo"
**Date**: 2025-11-12
**Status**: Gap analysis for immediate implementation

---

## What Exists (Strong Foundation ✅)

### 1. Core Infrastructure
- ✅ **PersonaUser** - AI citizen architecture (PersonaUser.ts)
- ✅ **PersonaInbox** - Priority queue for tasks (PersonaInbox.ts)
- ✅ **PersonaState** - Energy/mood/adaptive cadence (PersonaState.ts)
- ✅ **TrainingDaemon** - Observes chat, creates TrainingExampleEntity
- ✅ **Commands/Events** - Universal primitives working
- ✅ **AIProviderDaemon** - Ollama integration
- ✅ **ChatCoordinator** - Turn-taking for multi-AI
- ✅ **DataDaemon** - Persistent storage
- ✅ **ChatRAGBuilder** - RAG for chat history

### 2. Training Pipeline Foundation
- ✅ **TrainingExampleEntity** - Storage for training data
- ✅ **TrainingDaemonServer** - Observes chat messages
- ✅ **TrainingDataAccumulator** - Accumulation logic exists

### 3. Genome Architecture (Exists but Not Wired)
- ✅ **PersonaGenome** - LoRA layer management (PersonaGenome.ts)
- ✅ **Genome commands** - paging-activate, paging-stats, etc.
- ✅ **GenomeEntity** - Storage for genome metadata

---

## Critical Gaps for Phase 1

### 🚨 GAP 1: RAG System Doesn't Index Codebase

**Current State**: ChatRAGBuilder only indexes chat history
**Needed**: Index entire repo (docs/, *.ts files, README files)

**Impact**: HIGH - Without this, AI can't answer questions about code

**What's Missing**:
```typescript
// Need: CodebaseRAGBuilder
class CodebaseRAGBuilder extends RAGBuilder {
  async indexCodebase(paths: string[]): Promise<void> {
    // Index all TypeScript files
    // Index all markdown files
    // Extract exports, interfaces, classes
    // Create embeddings
    // Store in vector database
  }

  async query(question: string): Promise<RAGResult[]> {
    // Search embeddings
    // Return relevant code snippets with line numbers
    // Include file paths
  }
}
```

**Files to Create**:
- `system/rag/builders/CodebaseRAGBuilder.ts`
- `system/rag/indexers/TypeScriptIndexer.ts`
- `system/rag/indexers/MarkdownIndexer.ts`
- `commands/rag/index-codebase/` (command to trigger indexing)
- `commands/rag/query-codebase/` (command to query)

---

### 🚨 GAP 2: PersonaUser Doesn't Use RAG for Responses

**Current State**: PersonaUser uses ChatRAGBuilder for chat history only
**Needed**: Query codebase RAG + assemble prompt with results

**Impact**: HIGH - AI responses lack codebase context

**What's Missing**:
```typescript
// In PersonaUser.ts
async respondToMessage(message: ChatMessageEntity): Promise<void> {
  // 1. Query codebase RAG (MISSING)
  const codeContext = await Commands.execute('rag/query-codebase', {
    query: message.content.text,
    limit: 10
  });

  // 2. Assemble prompt with RAG results (MISSING)
  const prompt = this.buildPromptWithRAG(message, codeContext);

  // 3. Query AI (EXISTS)
  const response = await AIProviderDaemon.chat({ messages: [{ role: 'user', content: prompt }] });

  // 4. Post response (EXISTS)
  await this.postMessage(response);
}
```

**Files to Modify**:
- `system/user/server/PersonaUser.ts` - Add RAG query step
- Add `buildPromptWithRAG()` method

---

### 🚨 GAP 3: Async Commands with Inbox Delivery

**Current State**: Commands.execute() is synchronous (blocking)
**Needed**: async: true, deliveryMode: 'inbox' options

**Impact**: MEDIUM - Blocks PersonaUser on RAG queries

**What's Missing**:
```typescript
// In Commands.ts
interface AsyncCommandOptions {
  async?: boolean;
  deliveryMode?: 'inbox' | 'event' | 'interrupt';
  personaId?: UUID;
  timeout?: number;
}

async execute<P, R>(command: string, params: P & AsyncCommandOptions): Promise<R | void> {
  if (params.async) {
    // Execute in background
    this.executeInBackground(command, params);
    return; // Non-blocking
  }
  // ... existing sync logic
}
```

**Files to Modify**:
- `system/core/shared/Commands.ts` - Add async support
- `system/user/server/modules/PersonaInbox.ts` - Handle command-result tasks

---

### 🚨 GAP 4: Conversation Chain Detection

**Current State**: PersonaInbox treats each message individually
**Needed**: Group related messages into chains

**Impact**: MEDIUM - Better context, fewer redundant responses

**What's Missing**:
```typescript
// In PersonaInbox.ts
async getConversationChains(): Promise<ConversationChain[]> {
  // Find related messages (same room, recent, topically similar)
  // Group into chains
  // Return chains instead of individual messages
}

interface ConversationChain {
  id: UUID;
  messages: ChatMessageEntity[];
  topic: string;
  status: 'needs-response' | 'active';
}
```

**Files to Create**:
- `system/user/server/modules/ConversationChainDetector.ts`

**Files to Modify**:
- `system/user/server/modules/PersonaInbox.ts` - Add chain detection

---

### 🚨 GAP 5: Thread Consolidation for Training Data

**Current State**: TrainingDaemon creates one example per message
**Needed**: Consolidate conversation threads before storing

**Impact**: MEDIUM - Higher quality training data, fewer tokens

**What's Missing**:
```typescript
// In TrainingDaemonServer.ts
private threads: Map<UUID, MessageThread> = new Map();

async handleMessageCreated(message: ChatMessageEntity) {
  // Check if belongs to existing thread
  const threadId = await this.findThread(message);

  if (threadId) {
    await this.addToThread(threadId, message);
  } else {
    await this.createThread(message);
  }
}

async handleThreadCompleted(thread: MessageThread) {
  // Create ONE training example from entire thread
  const trainingExample = await this.consolidateThread(thread);
  await DataDaemon.store(TrainingExampleEntity.collection, trainingExample);
}
```

**Files to Create**:
- `daemons/training-daemon/server/ThreadConsolidator.ts`

**Files to Modify**:
- `daemons/training-daemon/server/TrainingDaemonServer.ts` - Add thread logic

---

### ⚠️ GAP 6: Self-Training Recipe (Teacher AI Generates Quizzes)

**Current State**: No automated quiz generation
**Needed**: Recipe that orchestrates Teacher AI → Helper AI → Grading → Training

**Impact**: LOW (Phase 1), HIGH (Phase 2) - Automates training data generation

**What's Missing**:
```typescript
// commands/recipe/self-train/
async function runSelfTraining(scope: string) {
  // 1. Teacher AI queries RAG for scope
  // 2. Teacher AI generates quiz questions
  // 3. Helper AI attempts answers
  // 4. Teacher AI grades
  // 5. Create training data from mistakes
  // 6. Fine-tune when threshold reached
}
```

**Files to Create**:
- `commands/recipe/self-train/` (entire command)
- `system/recipes/templates/SelfTrainingRecipe.ts`

---

### ⚠️ GAP 7: LoRA Fine-Tuning Integration

**Current State**: PersonaGenome exists but no actual training
**Needed**: Unsloth integration, JSONL export, training script

**Impact**: LOW (Phase 1), HIGH (Phase 2) - Can't improve AI without this

**What's Missing**:
```typescript
// commands/genome/fine-tune/
async function fineTuneGenome(personaId: UUID) {
  // 1. Export training data to JSONL
  const trainingFile = await exportToJSONL(personaId);

  // 2. Call Unsloth training script
  await exec(`python3 scripts/fine-tune.py --input=${trainingFile} --output=genome-v2.lora`);

  // 3. Register new LoRA layer
  await Commands.execute('genome/paging-adapter-register', {
    adapterId: `${personaId}-v2`,
    path: 'genome-v2.lora'
  });

  // 4. Activate for persona
  await Commands.execute('genome/paging-activate', {
    personaId,
    adapterId: `${personaId}-v2`
  });
}
```

**Files to Create**:
- `commands/genome/fine-tune/` (command)
- `commands/genome/export-training/` (export JSONL)
- `scripts/fine-tune.py` (Unsloth integration)

---

### ⚠️ GAP 8: Concurrency Management

**Current State**: PersonaUser processes one task at a time (sequential)
**Needed**: Worker pool with resource limits

**Impact**: MEDIUM - Better throughput, non-blocking

**What's Missing**:
```typescript
// In PersonaUser.ts
private readonly maxConcurrentTasks = 5;
private activeTasks: Set<Promise<void>> = new Set();

async serviceInbox() {
  while (true) {
    // Wait if pool full
    if (this.activeTasks.size >= this.maxConcurrentTasks) {
      await Promise.race(this.activeTasks);
    }

    // Get task
    const task = await this.inbox.peek();

    // Start task (non-blocking)
    const taskPromise = this.processTask(task).finally(() => {
      this.activeTasks.delete(taskPromise);
    });

    this.activeTasks.add(taskPromise);
  }
}
```

**Files to Modify**:
- `system/user/server/PersonaUser.ts` - Add concurrency logic

---

## Implementation Priority (Phase 1)

### **Week 1: RAG Foundation** (Critical)
1. ✅ Create CodebaseRAGBuilder
2. ✅ Create TypeScriptIndexer
3. ✅ Create MarkdownIndexer
4. ✅ Create `rag/index-codebase` command
5. ✅ Create `rag/query-codebase` command
6. ✅ Test: Index /system/user/, query "PersonaUser inbox"

**Success Criteria**: RAG returns relevant code snippets with line numbers

---

### **Week 2: PersonaUser Integration** (Critical)
1. ✅ Modify PersonaUser to query codebase RAG
2. ✅ Add `buildPromptWithRAG()` method
3. ✅ Test: Ask "Why does PersonaUser have inbox?" → Get accurate answer
4. ✅ Measure response accuracy (target 70%+)

**Success Criteria**: Helper AI answers basic architecture questions correctly

---

### **Week 3: Async Commands** (Important)
1. ✅ Add async support to Commands.execute()
2. ✅ Add inbox delivery mode
3. ✅ Modify PersonaInbox to handle command-result tasks
4. ✅ Test: RAG query arrives in inbox, PersonaUser processes

**Success Criteria**: PersonaUser non-blocking on RAG queries

---

### **Week 4: Thread Consolidation** (Important)
1. ✅ Create ThreadConsolidator
2. ✅ Modify TrainingDaemon to detect threads
3. ✅ Test: 4 related messages → 1 consolidated training example
4. ✅ Measure token savings (target 20-30% reduction)

**Success Criteria**: Training data is coherent threads, not fragments

---

## Deferred to Phase 2

**Self-Training Recipe** - Needs Phase 1 working first
**LoRA Fine-Tuning** - Needs training data accumulation first
**Concurrency** - Can start with sequential, add later
**Chain Detection** - Nice to have, not critical for MVP

---

## Testing Strategy

### Integration Test: Full Flow
```bash
# 1. Index codebase
./jtag rag/index-codebase --paths="/system/user/"

# 2. Ask question
./jtag collaboration/chat/send --roomId="general" --message="Why does PersonaUser have inbox?"

# 3. Wait for response
sleep 10

# 4. Screenshot
./jtag interface/screenshot --querySelector="chat-widget"

# Expected: Helper AI response with file references
# "PersonaUser.inbox is a priority queue (PersonaInbox.ts:45-120)..."
```

### Unit Tests
```bash
# RAG system
npx vitest system/rag/builders/CodebaseRAGBuilder.test.ts

# PersonaUser integration
npx vitest system/user/server/PersonaUser.rag-integration.test.ts

# Thread consolidation
npx vitest daemons/training-daemon/ThreadConsolidator.test.ts
```

---

## Success Metrics (4 Weeks)

**Quantitative**:
- Helper AI answers 70%+ of architecture questions correctly
- Response includes file paths + line numbers 90%+ of time
- Training data accumulates at 50+ examples/week
- Thread consolidation reduces tokens by 25%+

**Qualitative**:
- "Helper AI actually knows the codebase"
- "Faster than searching files manually"
- "Responses are coherent and accurate"

---

## Next Steps (This Week)

1. **Create CodebaseRAGBuilder** (2 days)
   - TypeScript indexer
   - Markdown indexer
   - Vector database integration

2. **Test RAG** (1 day)
   - Index /system/user/
   - Query and verify results
   - Measure retrieval accuracy

3. **Integrate with PersonaUser** (1 day)
   - Modify respondToMessage()
   - Test end-to-end flow

---

**Last Updated**: 2025-11-12
**Status**: Ready for implementation
**Next Review**: After Week 1 completion
