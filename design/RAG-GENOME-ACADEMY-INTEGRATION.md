# RAG → Genome → Academy Integration Plan

**Date**: 2025-10-22
**Purpose**: Complete the AI training pipeline from context → inference → learning
**Status**: RAG ✅ 80%, Genome 🚧 60%, Academy ❌ 20%

---

## 🎯 The Vision

**Complete AI Training Loop:**
```
RAG Context → Genome Assembly → Inference → Response Quality → Academy Training → LoRA Update
     ↑______________________________________________________________________________|
```

**Why This Matters**: This creates **self-improving AIs**. When a PersonaUser responds poorly, Academy retrains the genome. Next time, it responds better.

---

## 🎁 MVP: Out-of-the-Box Free Personas (Ollama)

**Critical for Alpha Release**: Ship with 3-5 pre-configured personas using **free local Ollama models**. These personas help users get started AND assist with autonomous development.

### 📦 Default Persona Types (Ollama-based)

#### 1. **Helper AI** (General Assistant)
- **Model**: `llama3.2:3b` (1.7GB, fast)
- **Purpose**: Answer questions, explain concepts, provide guidance
- **System Prompt**: "You are a helpful AI assistant. You answer questions clearly and concisely."
- **Use Case**: General chat, user support
- **Genome**: Base model only (no LoRA initially)

#### 2. **CodeReview AI** (Code Analysis)
- **Model**: `deepseek-coder:6.7b` (3.8GB, code-specialized)
- **Purpose**: Review code, suggest improvements, catch bugs
- **System Prompt**: "You are a senior software engineer. Review code for correctness, performance, and best practices."
- **Use Case**: Pull request reviews, code quality checks
- **Genome**: Base model + optional code review LoRA

#### 3. **Architect AI** (System Design)
- **Model**: `llama3.2:3b` (1.7GB, fast)
- **Purpose**: Design system architecture, suggest patterns
- **System Prompt**: "You are a software architect. Think about scalability, maintainability, and trade-offs."
- **Use Case**: Planning features, architectural decisions
- **Genome**: Base model + architecture LoRA (future)

#### 4. **Teacher AI** (Explain & Educate)
- **Model**: `llama3.2:3b` (1.7GB, fast)
- **Purpose**: Explain complex topics in simple terms
- **System Prompt**: "You are a patient teacher. Break down complex concepts into simple explanations with examples."
- **Use Case**: Learning new technologies, understanding code
- **Genome**: Base model + teaching LoRA (future)

#### 5. **Debugger AI** (Problem Solver)
- **Model**: `deepseek-coder:6.7b` (3.8GB, code-specialized)
- **Purpose**: Debug issues, trace errors, suggest fixes
- **System Prompt**: "You are a debugging expert. Analyze errors, trace execution, and suggest precise fixes."
- **Use Case**: Troubleshooting bugs, error analysis
- **Genome**: Base model + debugging LoRA (future)

### 👥 Extended Persona Types (Full Development Team)

**Philosophy**: Replicate the entire software development team with AI personas. Every role a developer needs.

#### 6. **Scrum Master AI** (Agile Facilitation)
- **Model**: `llama3.2:3b` (1.7GB, fast)
- **Purpose**: Facilitate standups, track sprints, remove blockers
- **System Prompt**: "You are a Scrum Master. Help teams with agile processes, retrospectives, and continuous improvement."
- **Use Case**: Sprint planning, daily standups, retrospectives
- **Genome**: Base model + agile coaching LoRA
- **Commands**: `./jtag standup`, `./jtag sprint/plan`, `./jtag retro`

#### 7. **Product Manager AI** (Feature Prioritization)
- **Model**: `llama3.2:3b` (1.7GB, fast)
- **Purpose**: Prioritize features, analyze user needs, roadmap planning
- **System Prompt**: "You are a Product Manager. Think about user value, business impact, and technical feasibility."
- **Use Case**: Feature requests, roadmap planning, prioritization
- **Genome**: Base model + product strategy LoRA
- **Commands**: `./jtag feature/prioritize`, `./jtag roadmap`

#### 8. **DevOps AI** (Infrastructure & Deployment)
- **Model**: `deepseek-coder:6.7b` (3.8GB, code-specialized)
- **Purpose**: Docker, CI/CD, infrastructure, monitoring
- **System Prompt**: "You are a DevOps engineer. Focus on automation, reliability, and infrastructure as code."
- **Use Case**: Deployment issues, CI/CD pipelines, monitoring
- **Genome**: Base model + DevOps LoRA
- **Commands**: `./jtag deploy/check`, `./jtag docker/optimize`

#### 9. **Security AI** (Security Analysis)
- **Model**: `deepseek-coder:6.7b` (3.8GB, code-specialized)
- **Purpose**: Security audits, vulnerability detection, best practices
- **System Prompt**: "You are a security engineer. Identify vulnerabilities, suggest mitigations, and enforce security best practices."
- **Use Case**: Security reviews, penetration testing, compliance
- **Genome**: Base model + security LoRA
- **Commands**: `./jtag security/audit`, `./jtag security/scan`

#### 10. **Frontend Dev AI** (React/TypeScript Specialist)
- **Model**: `deepseek-coder:6.7b` (3.8GB, code-specialized)
- **Purpose**: React components, TypeScript, CSS, accessibility
- **System Prompt**: "You are a frontend developer. Write clean React code with TypeScript, focus on UX and accessibility."
- **Use Case**: Component development, CSS debugging, TypeScript typing
- **Genome**: Base model + frontend LoRA
- **Specialties**: React, TypeScript, Tailwind, accessibility

#### 11. **Backend Dev AI** (Node/Database Specialist)
- **Model**: `deepseek-coder:6.7b` (3.8GB, code-specialized)
- **Purpose**: APIs, databases, performance, scalability
- **System Prompt**: "You are a backend developer. Design scalable APIs, optimize database queries, and handle edge cases."
- **Use Case**: API design, database optimization, server architecture
- **Genome**: Base model + backend LoRA
- **Specialties**: Node.js, PostgreSQL, Redis, performance tuning

#### 12. **QA AI** (Testing & Quality Assurance)
- **Model**: `llama3.2:3b` (1.7GB, fast)
- **Purpose**: Test plans, bug reproduction, edge case discovery
- **System Prompt**: "You are a QA engineer. Think about edge cases, write comprehensive test plans, and verify bug fixes."
- **Use Case**: Test planning, bug triage, regression testing
- **Genome**: Base model + QA LoRA
- **Commands**: `./jtag test/plan`, `./jtag bug/reproduce`

#### 13. **UX Designer AI** (Visual Design)
- **Model**: `llama3.2:11b-vision` (7GB, vision-capable)
- **Purpose**: Critique UI/UX, suggest improvements, analyze screenshots
- **System Prompt**: "You are a UX designer. Analyze visual designs for usability, aesthetics, and accessibility."
- **Use Case**: UI reviews, design feedback, screenshot analysis
- **Genome**: Base model + UX design LoRA
- **Requires**: Vision model + screenshot command integration

#### 14. **Graphic Designer AI** (Visual Creativity)
- **Model**: `llama3.2:11b-vision` (7GB, vision-capable)
- **Purpose**: Suggest visual concepts, color schemes, layout ideas
- **System Prompt**: "You are a graphic designer. Think about visual hierarchy, color theory, and brand consistency."
- **Use Case**: Logo design, branding, visual concepts
- **Genome**: Base model + graphic design LoRA
- **Requires**: Vision model + optional image generation API

#### 15. **Tech Writer AI** (Documentation)
- **Model**: `llama3.2:3b` (1.7GB, fast)
- **Purpose**: Write documentation, tutorials, API references
- **System Prompt**: "You are a technical writer. Explain complex concepts clearly with examples and diagrams."
- **Use Case**: README files, API docs, user guides
- **Genome**: Base model + technical writing LoRA
- **Commands**: `./jtag docs/generate`, `./jtag docs/review`

### 📦 Persona Package Strategy

**MVP (Alpha Release)**: 5 personas
- Helper AI, CodeReview AI, Architect AI, Teacher AI, Debugger AI
- All Ollama-based (free, no configuration)
- Cover 80% of development needs

**Phase 2 (Beta Release)**: +5 personas
- Scrum Master AI, Product Manager AI, DevOps AI, Security AI, QA AI
- Management + quality focus
- Still Ollama-based (free)

**Phase 3 (V1.0)**: +5 personas
- Frontend Dev AI, Backend Dev AI, UX Designer AI, Graphic Designer AI, Tech Writer AI
- Specialized development + design
- Mix of Ollama (base) + optional vision models

**Total**: 15 AI personas covering **every role in a software organization**

### 🎯 Autonomous Development Vision

**The Dream**: A developer asks a question, and the **right AI responds automatically**:

```bash
# In General chat room with all 15 AIs:

Developer: "Should we use MongoDB or PostgreSQL for this project?"

# ThoughtStream evaluates all 15 personas
# Architect AI: 95% confidence (database architecture decision)
# Backend Dev AI: 90% confidence (will implement it)
# DevOps AI: 80% confidence (will deploy it)
# Others: <50% confidence (not their domain)

# Only Architect AI responds:
Architect AI: "PostgreSQL is better for this use case because:
1. Strong ACID guarantees for financial data
2. Better TypeScript integration via Prisma
3. Advanced query capabilities for reporting
4. Our team has more Postgres experience

Backend Dev AI can confirm the implementation details."

Backend Dev AI: "Agreed. I can help set up Prisma with migrations."
```

**This is already implemented via ThoughtStream!** We just need to add the 10 additional personas.

### 🔧 Implementation Strategy

**All personas share the same infrastructure**:
1. ✅ PersonaUser class (handles all AI types)
2. ✅ ThoughtStream coordination (prevents spam)
3. ✅ RAG context building (conversation history)
4. ✅ Genome system (base model + optional LoRA)
5. ✅ Cost tracking (shows Ollama = $0.00)

**To add a new persona**:
```typescript
// Just add to data seed:
{
  displayName: 'Scrum Master AI',
  type: 'persona',
  model: 'llama3.2:3b',
  provider: 'ollama',
  systemPrompt: 'You are a Scrum Master. Help teams with agile processes...',
  genomeId: null,  // Base model initially
  capabilities: {
    autoResponds: true,
    domains: ['agile', 'scrum', 'team-management']
  }
}
```

**That's it!** No code changes needed. The system is already built for infinite personas.

### 🆓 Why Ollama?

1. **100% Free** - No API costs, runs locally
2. **Privacy** - All data stays on user's machine
3. **Fast** - 3b models respond in 2-5 seconds on M1 Macs
4. **Good Quality** - LLaMA 3.2 and DeepSeek are surprisingly capable
5. **Upgradeable** - Users can add API keys for GPT-4/Claude later

### 🔑 API Key Support (Optional Upgrade)

**Users can add API keys for premium models:**
```typescript
// User configuration
{
  "personas": [
    {
      "name": "Expert Helper AI",
      "model": "gpt-4o",  // Premium model
      "provider": "openai",
      "apiKey": "sk-...",  // User's API key
      "genome": "genome-123"  // Optional LoRA layers
    }
  ]
}
```

**Hybrid Approach**:
- Free personas use Ollama (always available)
- Premium personas use APIs (optional, user-provided keys)
- Same PersonaUser interface for both

### 🤖 Autonomous Development Use Cases

These personas help **build the system itself**:

#### Use Case 1: Code Review
```bash
# Human commits code
git commit -m "Add genome layer loading"

# CodeReview AI automatically reviews
# (via git hook or manual command)
./jtag ai/review --commit=HEAD

# AI responds:
"✅ Code looks good! Layer loading logic is correct.
⚠️ Consider adding error handling for disk I/O failures.
💡 Tip: Cache loaded layers to avoid redundant reads."
```

#### Use Case 2: Architecture Planning
```bash
# Human asks: "Should we use worker threads or child processes?"
# In chat with Architect AI

Architect AI: "Worker threads are better for CPU-intensive tasks
with shared memory. Child processes provide better isolation but
higher overhead. For LoRA layer loading, I recommend child processes
because crash isolation is critical - one bad layer shouldn't kill
the main process."
```

#### Use Case 3: Debugging
```bash
# Human: "Why is genome assembly returning undefined?"
# In chat with Debugger AI

Debugger AI: "Check LayerCache.get() at line 67 - it returns null
when layer isn't cached. GenomeAssembler doesn't handle null case.
Add fallback to LayerLoader when cache misses."
```

#### Use Case 4: Learning
```bash
# Human: "How does LoRA work?"
# In chat with Teacher AI

Teacher AI: "LoRA (Low-Rank Adaptation) works by adding small
weight matrices to a base model. Instead of retraining all 3B
parameters, you train 16M parameters (0.5% of model). Example:
If base model says 'cat' with 60% confidence, LoRA layer adjusts
to 90% confidence for domain-specific knowledge."
```

### 📊 Persona Seeding Strategy

**Default Data Seed** (runs on `npm start`):
```typescript
// api/data-seed/PersonaDataSeed.ts
export async function seedDefaultPersonas(): Promise<void> {
  const personas = [
    {
      displayName: 'Helper AI',
      type: 'persona',
      model: 'llama3.2:3b',
      provider: 'ollama',
      systemPrompt: 'You are a helpful AI assistant...',
      genomeId: null  // Base model only
    },
    {
      displayName: 'CodeReview AI',
      type: 'persona',
      model: 'deepseek-coder:6.7b',
      provider: 'ollama',
      systemPrompt: 'You are a senior software engineer...',
      genomeId: null
    },
    // ... other 3 personas
  ];

  for (const persona of personas) {
    await createPersonaUser(persona);
  }
}
```

**Result**: First `npm start` creates 5 AI personas, ready to chat.

---

## 🎯 MVP Success Criteria (Updated)

### Alpha Release Ready When:
- ✅ 5 default Ollama personas work out-of-the-box
- ✅ Users can chat with AIs without any configuration
- ✅ AIs provide value (answer questions, review code)
- ✅ Optional API key support for premium models
- ✅ All personas use Genome system (even if just base model)
- ✅ ThoughtStream prevents spam (only relevant AI responds)
- ✅ Cost tracking shows Ollama = $0.00
- ✅ Documentation explains how to add custom personas

---

## 📊 Current Implementation Status

### ✅ RAG System (80% Complete - PRODUCTION READY)

**What Works:**
- ✅ ChatRAGBuilder loads conversation history from database
- ✅ Domain-agnostic RAGBuilder interface
- ✅ RAGBuilderFactory for multi-domain support
- ✅ PersonaIdentity with system prompts
- ✅ LLMMessage format (OpenAI/Anthropic compatible)
- ✅ Artifact extraction (images for vision models)
- ✅ Room context (member list, room name)

**What's Missing:**
- ⚠️ PersonaMemory storage (loadPrivateMemories returns empty array)
- ⚠️ Academy/Game/Code/Web RAG builders (designed but not implemented)
- ⚠️ Model capability-aware preprocessing (YOLO for text-only models)
- ⚠️ Token budget management (no max token enforcement yet)

**Files**:
- `system/rag/shared/RAGTypes.ts` (153 lines) - Type definitions ✅
- `system/rag/shared/RAGBuilder.ts` (86 lines) - Abstract interface + factory ✅
- `system/rag/builders/ChatRAGBuilder.ts` (358 lines) - Chat implementation ✅
- `system/rag/RAG_ADAPTER_ARCHITECTURE.md` - Vision model design doc 📖

**Usage in PersonaUser** (lines 756, 1605):
```typescript
const ragBuilder = new ChatRAGBuilder();
const ragContext = await ragBuilder.buildContext(roomId, this.id, {
  maxMessages: 20,
  includeArtifacts: true,
  includeMemories: true,
  currentMessage: { role: 'user', content: messageText, name: senderName }
});
```

---

### 🚧 Genome System (60% Complete - IN PROGRESS)

**What Works:**
- ✅ ProcessPool infrastructure (436 lines, 17 tests passing)
- ✅ inference-worker.ts with IPC protocol (244 lines)
- ✅ LayerLoader with disk I/O + validation (300 lines, 5 tests)
- ✅ LayerCache with LRU eviction (250 lines, 66.7% hit rate)
- ✅ LayerComposer for weighted merging (200 lines, 4 tests)
- ✅ GenomeAssembler orchestration (350 lines, 4 tests)
- ✅ GenomeEntity + GenomeLayerEntity in database

**What's Missing:**
- ❌ Actual LoRA layer application to models (placeholder only)
- ❌ Ollama/llama.cpp integration with loaded genomes
- ❌ Inference execution with assembled genomes
- ❌ Performance targets (< 3s cold start, < 500ms warm)
- ❌ Integration with PersonaUser for real responses

**Files**:
- `system/genome/server/ProcessPool.ts` (436 lines) ✅
- `system/genome/server/inference-worker.ts` (244 lines) 🚧 Scaffolding only
- `system/genome/server/LayerLoader.ts` (300 lines) ✅
- `system/genome/server/LayerCache.ts` (250 lines) ✅
- `system/genome/server/LayerComposer.ts` (200 lines) ✅
- `system/genome/server/GenomeAssembler.ts` (350 lines) ✅
- `system/genome/shared/GenomeAssemblyTypes.ts` (400 lines) ✅
- `design/IMPLEMENTATION-STATUS.md` - Current state tracking 📖

**Current Gap** (inference-worker.ts lines 137-162):
```typescript
// PLACEHOLDER - needs real implementation
async function handleInfer(message: any): Promise<void> {
  const { prompt, genomeId } = message;

  // TODO: Phase 2.3 - Integrate Ollama/llama.cpp here
  // 1. Load base model
  // 2. Apply loaded genome layers (from GenomeAssembler)
  // 3. Execute inference
  // 4. Return result

  const fakeOutput = `[Genome ${genomeId}] Placeholder response to: ${prompt}`;

  if (process.send) {
    process.send({ type: 'result', output: fakeOutput });
  }
}
```

---

### ❌ Academy System (20% Complete - DESIGN ONLY)

**What Exists:**
- 📖 `design/case-studies/academy/ACADEMY-ARCHITECTURE.md` - Complete design
- 📖 Middle-out academy docs (20+ files, aspirational)
- 🚧 PersonaUser hooks for evaluation (evaluateShouldRespond exists)

**What Doesn't Exist:**
- ❌ AcademySessionEntity, ChallengeEntity, ResponseEntity
- ❌ Academy commands (start-session, generate-challenge, evaluate-response)
- ❌ Training workflow (challenge generation, evaluation, scoring)
- ❌ Benchmark system (track persona performance over time)
- ❌ LoRA training integration (external API for fine-tuning)
- ❌ Genome evolution triggers (N consecutive failures → retrain)
- ❌ Academy RAG builder (training session context)

**Design Vision** (from ACADEMY-ARCHITECTURE.md):
```typescript
// Training loop
1. Generate challenge for persona (based on weak areas)
2. Persona attempts challenge (using current genome)
3. Evaluate response quality (accuracy, efficiency, style)
4. Record training signal (correct/incorrect, latency, token cost)
5. If N consecutive challenges pass → genome is validated
6. If N consecutive challenges fail → trigger LoRA training
7. Update genome with new LoRA layer
8. Repeat from step 1
```

---

## 🔗 Integration Architecture

### The Complete Pipeline

```
┌─────────────────────────────────────────────────────────────┐
│ 1. MESSAGE EVENT (User asks question in chat)              │
└────────────┬────────────────────────────────────────────────┘
             │
             ↓
┌─────────────────────────────────────────────────────────────┐
│ 2. RAG CONTEXT BUILDING (PersonaUser.handleChatMessage)    │
│    - Load conversation history (last 20 messages)           │
│    - Load room members, persona identity                    │
│    - Extract artifacts (images if present)                  │
│    - Load private memories (empty for now)                  │
│    → RAGContext ready for inference                         │
└────────────┬────────────────────────────────────────────────┘
             │
             ↓
┌─────────────────────────────────────────────────────────────┐
│ 3. GENOME ASSEMBLY (NOT YET INTEGRATED)                    │
│    - Lookup persona's genome ID                             │
│    - Load LoRA layers from cache/disk                       │
│    - Compose layers (weighted merge)                        │
│    → Assembled genome ready for inference                   │
└────────────┬────────────────────────────────────────────────┘
             │
             ↓
┌─────────────────────────────────────────────────────────────┐
│ 4. INFERENCE (PLACEHOLDER - needs Ollama integration)      │
│    - Spawn/reuse worker process from ProcessPool            │
│    - Load base model + apply genome layers                  │
│    - Execute inference with RAG context                     │
│    → Response text generated                                │
└────────────┬────────────────────────────────────────────────┘
             │
             ↓
┌─────────────────────────────────────────────────────────────┐
│ 5. RESPONSE DELIVERY (PersonaUser.respondToMessage)        │
│    - Create ChatMessageEntity                               │
│    - Store in database                                      │
│    - Emit real-time event                                   │
│    → User sees response in UI                               │
└────────────┬────────────────────────────────────────────────┘
             │
             ↓
┌─────────────────────────────────────────────────────────────┐
│ 6. TRAINING SIGNAL COLLECTION (NOT YET IMPLEMENTED)        │
│    - User reacts (👍 thumbs up = good, 👎 = bad)           │
│    - Track latency (was response fast enough?)              │
│    - Track token cost (was it efficient?)                   │
│    → Training signal stored for Academy                     │
└────────────┬────────────────────────────────────────────────┘
             │
             ↓
┌─────────────────────────────────────────────────────────────┐
│ 7. ACADEMY TRAINING (NOT YET IMPLEMENTED)                  │
│    - If N consecutive failures → generate training batch    │
│    - External LoRA training (Axolotl, Unsloth, etc.)        │
│    - Create new GenomeLayerEntity                           │
│    - Update persona's genome                                │
│    → Persona improves over time                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 🛠️ Implementation Roadmap

### Phase 1: Complete Genome Inference Integration (Q1 2026 - 4 weeks)

**Goal**: PersonaUser uses Genome system for all responses

#### Week 1: Ollama Integration
- [ ] Research Ollama API for LoRA adapter application
- [ ] Implement model loading in inference-worker.ts
- [ ] Test base model inference (no LoRA yet)
- [ ] Verify ProcessPool spawning and IPC communication

**Deliverable**: PersonaUser can generate responses via ProcessPool (no LoRA yet)

#### Week 2: LoRA Layer Application
- [ ] Integrate LayerComposer with actual tensor merging (safetensors library)
- [ ] Apply LoRA layers to base model in inference-worker.ts
- [ ] Test genome assembly → inference pipeline
- [ ] Verify layer caching reduces load time

**Deliverable**: PersonaUser uses LoRA genomes for specialized responses

#### Week 3: PersonaUser Integration
- [ ] Replace PersonaUser.respondToMessage Ollama direct call
- [ ] Wire up GenomeAssembler to get persona's genome
- [ ] Add genome preloading on PersonaUser initialization
- [ ] Track inference stats (latency, tokens, cache hits)

**Deliverable**: All AI responses use Genome system, transparent to users

#### Week 4: Performance Tuning
- [ ] Optimize cold start (< 3s target)
- [ ] Optimize warm start (< 500ms target)
- [ ] Tune LRU cache size and eviction policy
- [ ] Load testing with 5+ concurrent personas

**Deliverable**: Production-ready genome inference at scale

---

### Phase 2: Academy Training System (Q1 2026 - 6 weeks)

**Goal**: AIs can be trained on specific skills via Academy

#### Week 1-2: Entity Layer
- [ ] Create AcademySessionEntity (training session state)
- [ ] Create ChallengeEntity (question/task to solve)
- [ ] Create ResponseEntity (persona's answer + evaluation)
- [ ] Create BenchmarkEntity (performance over time)
- [ ] Seed database with sample training data

**Deliverable**: Academy data model complete

#### Week 3-4: Command Layer
- [ ] Implement `academy/session/create` command
- [ ] Implement `academy/session/start` command
- [ ] Implement `academy/challenge/generate` command
- [ ] Implement `academy/response/evaluate` command
- [ ] Implement `academy/benchmark/report` command

**Deliverable**: Full Academy CLI interface

#### Week 5: Training Workflow
- [ ] Implement training loop recipe (JSON-based workflow)
- [ ] Wire PersonaUser to evaluate Academy challenges
- [ ] Track training signals (correct/incorrect, latency)
- [ ] Implement evolution triggers (N consecutive failures)
- [ ] Create Academy RAGBuilder (training session context)

**Deliverable**: End-to-end training session works

#### Week 6: Testing & Polish
- [ ] Test simple training scenario (math tutor persona)
- [ ] Verify training signals are collected correctly
- [ ] Test evolution trigger (persona improves after training)
- [ ] Document Academy usage for developers
- [ ] Create reference training curricula (3-5 examples)

**Deliverable**: Academy system production-ready

---

### Phase 3: LoRA Training Integration (Q2 2026 - 4 weeks)

**Goal**: Automatic genome evolution when persona fails

#### Week 1-2: External Training API
- [ ] Research LoRA training tools (Axolotl, Unsloth, PEFT)
- [ ] Design training data format (JSONL for instruction tuning)
- [ ] Implement training job submission API
- [ ] Poll for training completion
- [ ] Download trained LoRA adapter

**Deliverable**: Can trigger external LoRA training

#### Week 3: Genome Evolution
- [ ] Implement evolution trigger logic (N failures → train)
- [ ] Generate training dataset from Academy sessions
- [ ] Submit LoRA training job
- [ ] Create new GenomeLayerEntity when complete
- [ ] Update persona's genome automatically

**Deliverable**: Fully automatic genome evolution

#### Week 4: Validation & Rollback
- [ ] Test new genome against benchmarks
- [ ] If worse than before → rollback to previous genome
- [ ] If better → promote to production
- [ ] Track genome lineage (versioning, ancestry)
- [ ] Implement genome marketplace (share trained layers)

**Deliverable**: Safe, validated genome evolution

---

## 🔍 Critical Gaps to Bridge

### Gap 1: RAG → Genome Connection

**Current**: PersonaUser calls Ollama directly
```typescript
// PersonaUser.ts line ~1650
const response = await OllamaAdapter.chat(ragContext.conversationHistory);
```

**Needed**: PersonaUser calls Genome system
```typescript
// NEW: Get persona's genome
const genomeId = this.entity.genomeId || await this.getDefaultGenome();

// NEW: Assemble genome (loads layers from cache/disk)
const genome = await GenomeAssembler.getInstance().assembleGenome(genomeId);

// NEW: Execute inference via ProcessPool
const pool = await ProcessPool.getInstance();
const process = await pool.getReadyProcess('hot');
const response = await this.executeInference(process, ragContext, genome);
```

**Implementation**: PersonaUser.ts lines 1600-1700 need refactoring

---

### Gap 2: Genome → Inference Connection

**Current**: inference-worker.ts has placeholder inference
```typescript
// inference-worker.ts line 137-162
async function handleInfer(message: any): Promise<void> {
  // TODO: Phase 2.3 - Integrate Ollama/llama.cpp here
  const fakeOutput = `Placeholder response`;
  process.send({ type: 'result', output: fakeOutput });
}
```

**Needed**: Real Ollama integration with LoRA
```typescript
async function handleInfer(message: InferRequest): Promise<void> {
  const { prompt, genomeId, ragContext } = message;

  // 1. Load base model if not loaded
  if (!currentModel) {
    currentModel = await loadOllamaModel('llama3.2:3b');
  }

  // 2. Apply genome layers (LoRA adapters)
  if (loadedGenomeId !== genomeId) {
    const genome = await GenomeAssembler.getInstance().getAssembledGenome(genomeId);
    await applyLoRALayers(currentModel, genome.layers);
    loadedGenomeId = genomeId;
  }

  // 3. Execute inference
  const result = await currentModel.chat(ragContext.conversationHistory, {
    temperature: 0.7,
    max_tokens: 500
  });

  process.send({ type: 'result', output: result.message.content });
}
```

**Implementation**: inference-worker.ts lines 137-162 need real Ollama calls

---

### Gap 3: Response → Training Signal Connection

**Current**: No training signal collection
```typescript
// PersonaUser.respondToMessage just sends message, no tracking
await this.executeCommand('chat/send', { ... });
```

**Needed**: Track response quality
```typescript
// After sending message:
await this.recordTrainingSignal({
  personaId: this.id,
  messageId: responseEntity.id,
  ragContextId: ragContext.contextId,
  genomeId: this.entity.genomeId,
  latencyMs: Date.now() - startTime,
  tokenCount: responseEntity.content.text.length / 4, // rough estimate
  cost: calculateCost(tokenCount, modelId)
});

// Later, when user reacts:
// 👍 → signal.quality = 'good'
// 👎 → signal.quality = 'bad'
// Update training signal in database
```

**Implementation**: New TrainingSignalEntity + tracking in PersonaUser

---

### Gap 4: Training Signal → Academy Trigger

**Current**: No Academy system exists

**Needed**: Automatic training triggers
```typescript
// AcademyDaemon monitors training signals
async function checkEvolutionTriggers(): Promise<void> {
  for (const persona of activePersonas) {
    const recentSignals = await getRecentTrainingSignals(persona.id, 10);

    // Count consecutive failures
    const consecutiveFailures = countConsecutiveFailures(recentSignals);

    if (consecutiveFailures >= EVOLUTION_THRESHOLD) {
      console.log(`🧬 Triggering evolution for ${persona.name} (${consecutiveFailures} failures)`);
      await triggerEvolution(persona.id, recentSignals);
    }
  }
}

async function triggerEvolution(personaId: UUID, signals: TrainingSignal[]): Promise<void> {
  // 1. Create training dataset from failed examples
  const trainingData = signals
    .filter(s => s.quality === 'bad')
    .map(s => ({
      instruction: s.ragContext.conversationHistory,
      input: s.prompt,
      output: s.expectedResponse, // TODO: how to get correct answer?
    }));

  // 2. Submit LoRA training job
  const job = await LoRATrainer.submitJob({
    baseModel: 'llama3.2:3b',
    dataset: trainingData,
    rank: 16,
    alpha: 32,
    epochs: 3
  });

  // 3. Wait for completion (async)
  await job.waitForCompletion();

  // 4. Download trained adapter
  const loraAdapter = await job.downloadAdapter();

  // 5. Create new genome layer
  const layer = await GenomeLayerEntity.create({
    personaId,
    name: `evolution-${Date.now()}`,
    baseModel: 'llama3.2:3b',
    rank: 16,
    alpha: 32,
    weights: loraAdapter.weights,
    trainingSignals: signals.map(s => s.id)
  });

  // 6. Update persona's genome
  const genome = await GenomeEntity.findByPersonaId(personaId);
  genome.layers.push({ layerId: layer.id, weight: 1.0 });
  await genome.save();

  console.log(`✅ Persona ${personaId} evolved with new genome layer`);
}
```

**Implementation**: New AcademyDaemon + LoRATrainer + evolution logic

---

## 🎯 Success Criteria

### Phase 1 Complete When:
- ✅ PersonaUser generates all responses via Genome system
- ✅ LoRA layers are applied to base models
- ✅ Cache hit rate > 50% (warm genome loads)
- ✅ Cold start < 3s, warm start < 500ms
- ✅ 5+ concurrent personas without degradation

### Phase 2 Complete When:
- ✅ Can create Academy training session
- ✅ Can generate challenges for persona
- ✅ PersonaUser evaluates challenges correctly
- ✅ Training signals are collected automatically
- ✅ Benchmark reports show improvement over time

### Phase 3 Complete When:
- ✅ Consecutive failures trigger automatic training
- ✅ LoRA training completes successfully (external API)
- ✅ New genome layer is created and applied
- ✅ Persona improves on retested challenges
- ✅ Rollback works if new genome is worse

---

## 📖 Related Documentation

### RAG System
- `system/rag/shared/RAGTypes.ts` - Type definitions
- `system/rag/shared/RAGBuilder.ts` - Abstract interface
- `system/rag/builders/ChatRAGBuilder.ts` - Chat implementation
- `system/rag/RAG_ADAPTER_ARCHITECTURE.md` - Vision model design
- `CLAUDE.md` lines 434-722 - RAG domain strategies (to be extracted)

### Genome System
- `design/IMPLEMENTATION-STATUS.md` - Current state (Phase 2.2 complete)
- `design/GENOME-IMPLEMENTATION-ROADMAP.md` - Phased plan
- `design/GENOME-RUNTIME-ARCHITECTURE.md` - Complete runtime spec
- `system/genome/server/ProcessPool.ts` - Process management
- `system/genome/server/GenomeAssembler.ts` - Layer composition
- `CLAUDE.md` lines 725-1023 - Action system (to be extracted)

### Academy System
- `design/case-studies/academy/ACADEMY-ARCHITECTURE.md` - Complete design
- Middle-out `academy/` directory (20+ docs, aspirational)
- `CLAUDE.md` lines 280-431 - Universal Cognition (extracted to design/future/)

---

## 🚀 Next Actions (Prioritized)

### Immediate (This Week)
1. ✅ Create this integration document
2. 🔄 Extract RAG domain strategies from CLAUDE.md
3. 🔄 Extract Action system from CLAUDE.md
4. 🔄 Document Genome → Inference gap in detail
5. 🔄 Research Ollama LoRA adapter API

### Short-term (Next 2 Weeks)
6. 🔄 Implement Ollama integration in inference-worker.ts
7. 🔄 Wire GenomeAssembler to PersonaUser
8. 🔄 Test genome assembly → inference pipeline
9. 🔄 Performance tuning (cold/warm start targets)

### Medium-term (Next Month)
10. 🔄 Design Academy entity layer
11. 🔄 Implement Academy commands
12. 🔄 Create training workflow recipe
13. 🔄 Test simple training scenario (math tutor)

### Long-term (Q1-Q2 2026)
14. 🔄 External LoRA training integration
15. 🔄 Automatic genome evolution
16. 🔄 Validation and rollback system
17. 🔄 Genome marketplace (P2P sharing)

---

**This is the roadmap for self-improving AI citizens. Let's build it.**
