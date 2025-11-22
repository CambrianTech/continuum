# Dataset Construction & Training Architecture

## Core Principle

**Everything reduces to: Build TrainingExample entities → Pass to training**

All the complexity is just different ways to construct these entities and orchestrate their use.

## The Fundamental Type

```typescript
interface TrainingExample {
  messages: TrainingMessage[];  // Standard chat format
  metadata?: {
    timestamp?: number;
    roomId?: UUID;
    correctionId?: UUID;
    confidence?: number;
    [key: string]: unknown;  // Extensible
  };
}

interface TrainingMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}
```

**That's it.** Everything else is infrastructure to build and use these.

---

## Dataset Construction Pathways

### 1. Manual Construction (Simplest)

**Status**: ✅ Works now (via TrainingDatasetBuilder)

```bash
# Create JSONL file manually
cat > teaching-examples.jsonl <<'EOF'
{"messages":[{"role":"user","content":"What is TypeScript?"},{"role":"assistant","content":"TypeScript is JavaScript with syntax for types."}]}
{"messages":[{"role":"user","content":"Explain interfaces"},{"role":"assistant","content":"Interfaces define the structure of objects."}]}
EOF

# Load and train (TODO: Add datasetPath parameter)
./jtag genome/train \
  --personaId="teacher-ai-id" \
  --provider="unsloth" \
  --datasetPath="teaching-examples.jsonl"
```

**Use cases:**
- Initial knowledge base loading
- Curated examples from experts
- Synthetic training data
- Test datasets for development

---

### 2. Chat History Extraction (Working)

**Status**: ✅ Implemented (Phase 7.1)

```bash
# Extract from conversation history
./jtag genome/train \
  --personaId="helper-ai-id" \
  --provider="unsloth" \
  --roomId="general-room-id" \
  --maxMessages=50 \
  --minMessages=10
```

**How it works:**
1. `TrainingDatasetBuilder.buildFromConversation()` extracts messages
2. Filters for PersonaUser's responses
3. Pairs with preceding user messages
4. Creates TrainingExample entities
5. Passes to training adapter

**Use cases:**
- Learn from past conversations
- Specialize to room/domain
- Capture successful interaction patterns

**Implementation**: `system/genome/fine-tuning/server/TrainingDatasetBuilder.ts`

---

### 3. Recipe-Embedded Capture (Working)

**Status**: ✅ Implemented (Phase 7.4-7.5)

```typescript
// During recipe execution
await Commands.execute('genome/capture-interaction', {
  personaId: this.id,
  roleId: 'teacher',
  domain: 'teaching-typescript',
  input: contextPrompt,
  output: aiResponse
});

// Optional feedback
await Commands.execute('genome/capture-feedback', {
  targetPersonaId: this.id,
  targetRole: 'teacher',
  domain: 'teaching-typescript',
  feedbackType: 'correction',
  feedbackContent: 'Use simpler language',
  qualityScore: 0.7
});

// Auto-trains when buffer reaches threshold
// PersonaUser.checkTrainingReadiness() runs every 60s
```

**How it works:**
1. Interactions captured to `TrainingDataAccumulator` (RAM buffer)
2. Organized by domain (conversation, code, teaching, etc.)
3. Feedback attached to examples
4. When threshold reached (default: 50), auto-triggers training
5. Buffer cleared after consumption

**Use cases:**
- IVR/tech support learning from customer calls
- Teacher AI improving pedagogy
- Code review bot learning from feedback
- Real-time adaptation during production use

**Implementation**:
- `system/user/server/modules/TrainingDataAccumulator.ts`
- `commands/genome/capture-interaction/`
- `commands/genome/capture-feedback/`
- `system/user/server/PersonaUser.ts` (checkTrainingReadiness)

---

### 4. Corpus Ingestion (TODO - Phase 7.5.2)

**Status**: ⏳ Not implemented

```bash
# Load company knowledge base
./jtag genome/ingest-dataset \
  --personaId="support-ai-id" \
  --domain="tech-support" \
  --source="./company-faq.jsonl" \
  --trainImmediately=true

# Or load from URL
./jtag genome/ingest-dataset \
  --personaId="support-ai-id" \
  --domain="tech-support" \
  --source="https://example.com/training-corpus.jsonl" \
  --trainImmediately=false  # Add to buffer, train later
```

**Planned implementation:**
```typescript
// Add to TrainingDataAccumulator
ingestDataset(domain: string, dataset: TrainingDataset): Promise<void> {
  for (const example of dataset.examples) {
    await this.captureInteraction({
      domain,
      roleId: 'student',
      input: example.messages.find(m => m.role === 'user')?.content,
      output: example.messages.find(m => m.role === 'assistant')?.content
    });
  }
}
```

**Use cases:**
- Load company docs/FAQs
- Import Stack Overflow Q&A
- Process textbooks/tutorials
- Bulk knowledge transfer

---

### 5. Self-Directed Learning (TODO - Phase 7.5.3)

**Status**: ⏳ Not implemented (Task system exists, just needs wiring)

```typescript
// PersonaUser creates task for itself
await this.taskQueue.add({
  taskType: 'study',
  domain: 'typescript-advanced',
  description: 'Study advanced TypeScript patterns',
  priority: 0.5,
  executionPlan: {
    steps: [
      { action: 'fetch', source: 'https://typescript-book.com/chapters.json' },
      { action: 'ingest', domain: 'typescript-advanced' },
      { action: 'train', provider: 'unsloth' }
    ]
  }
});
```

**Use cases:**
- Scheduled learning sessions
- Self-improvement routines
- Knowledge gap detection
- Continuous skill acquisition

---

## Training Execution Flow

### Current Implementation (Chat History)

```
genome/train command
  ↓
Load PersonaUser from DB
  ↓
TrainingDatasetBuilder.buildFromConversation()
  ↓
Query chat_messages for roomId
  ↓
Filter for PersonaUser responses
  ↓
Pair with user messages
  ↓
Build TrainingDataset
  ↓
Get LoRA adapter (UnslothLoRAAdapter, etc.)
  ↓
adapter.trainLoRA(request)
  ↓
Export dataset to JSONL
  ↓
Call Python subprocess (unsloth-train.py)
  ↓
Python: Load model, train LoRA, export adapter
  ↓
Save adapter to .continuum/genomes/{personaId}/adapters/
  ↓
Return result with metrics
```

**Files involved:**
- `commands/genome/train/server/GenomeTrainServerCommand.ts` (orchestration)
- `system/genome/fine-tuning/server/TrainingDatasetBuilder.ts` (extraction)
- `system/genome/fine-tuning/server/adapters/UnslothLoRAAdapter.ts` (training)
- `system/genome/fine-tuning/server/adapters/scripts/unsloth-train.py` (Python)

---

### Planned: Direct Dataset Training

```
genome/train command (with --datasetPath)
  ↓
Load dataset from JSONL file
  ↓
Skip TrainingDatasetBuilder (already have dataset)
  ↓
Get LoRA adapter
  ↓
adapter.trainLoRA(request)
  ↓
... (same as above)
```

---

### Planned: Buffer-Based Training

```
PersonaUser.checkTrainingReadiness() (every 60s)
  ↓
Check TrainingDataAccumulator.shouldMicroTune(domain)
  ↓
Consume examples from buffer
  ↓
Build TrainingDataset from examples
  ↓
Execute genome/train programmatically
  ↓
... (same flow as above)
```

---

## The Dataset Entity Types

### TrainingDataAccumulator Format (RAM)

```typescript
// In-memory format (PersonaUser's accumulator)
interface TrainingExample {
  id: string;
  domain: string;
  roleId: string;
  personaId?: UUID;
  input: string;
  output: string;
  expectedOutput?: string;
  feedback?: {
    source: 'human' | 'ai' | 'system';
    rating?: number;
    comments?: string;
    corrections?: string;
  };
  timestamp: Date;
  contextMetadata?: Record<string, unknown>;
}
```

### Training Adapter Format (Disk/API)

```typescript
// Format expected by training adapters
interface TrainingExample {
  messages: TrainingMessage[];
  metadata?: {
    timestamp?: number;
    roomId?: UUID;
    correctionId?: UUID;
    confidence?: number;
  };
}
```

**Conversion**: Accumulator format → Adapter format happens in `genome/train` command

---

## Key Design Principles

### 1. **Single Responsibility**
Each component does ONE thing:
- TrainingDataAccumulator: Hold examples in RAM
- genome/capture-interaction: Add one example
- genome/train: Execute training on dataset
- TrainingDatasetBuilder: Extract from chat history

### 2. **Composability**
All pathways produce the same TrainingExample entity. Mix and match:
- Load corpus + capture interactions + extract history
- All go into same buffer
- Train on combined dataset

### 3. **No Forced Batching**
The batch threshold is a heuristic, not a constraint:
- Can train on 1 example
- Can train on 10,000 examples
- Threshold just triggers automatic training
- Manual training works anytime

### 4. **Extensible Metadata**
Every example carries context:
- Domain (conversation, code, teaching, etc.)
- Role (assistant, teacher, reviewer, etc.)
- Feedback (corrections, scores, comments)
- Custom metadata (thought streams, outcomes, etc.)

### 5. **Organic Orchestration**
Everything happens via commands:
- Manual: `./jtag genome/train --datasetPath=...`
- Recipe: `Commands.execute('genome/capture-interaction', ...)`
- Automatic: `PersonaUser.checkTrainingReadiness()`
- Self-directed: `TaskQueue.add({ taskType: 'study', ... })`

---

## What's Missing

### Immediate (Can test today):
- [ ] Add `datasetPath` parameter to genome/train
- [ ] Support loading JSONL datasets directly
- [ ] Test with simple manual dataset

### Short-term (Phase 7.5.2):
- [ ] Implement genome/ingest-dataset command
- [ ] Support corpus loading from files/URLs
- [ ] Add to TrainingDataAccumulator

### Medium-term (Phase 7.6):
- [ ] Recipe learning configuration
- [ ] Specify which roles learn during recipe
- [ ] Configure feedback sources
- [ ] Set batch thresholds per domain

### Long-term (Phase 7.7+):
- [ ] Self-directed learning tasks
- [ ] Librarian persona for curation
- [ ] Outcome-based quality signals
- [ ] Multi-modal training (code + tests + results)

---

## Example: IVR Tech Support

**Phase 1: Load knowledge base**
```bash
./jtag genome/train \
  --personaId="support-ai-id" \
  --provider="unsloth" \
  --datasetPath="./company-docs.jsonl"  # 500 examples
```

**Phase 2: Continuous learning from live calls**
```typescript
// Recipe captures each customer interaction
await Commands.execute('genome/capture-interaction', {
  personaId: 'support-ai-id',
  roleId: 'support-agent',
  domain: 'customer-support',
  input: customerQuestion,
  output: aiResponse
});

// Human agent provides correction if needed
if (needsCorrection) {
  await Commands.execute('genome/capture-feedback', {
    targetPersonaId: 'support-ai-id',
    feedbackType: 'correction',
    feedbackContent: betterResponse,
    qualityScore: 0.9
  });
}

// After 50 high-quality interactions, auto-trains
// PersonaUser gets better at THIS company's support patterns
```

**Result**: Base knowledge + real-world refinement = Highly effective support AI

---

## Testing Strategy

### Unit Tests (Already exist):
- TrainingDataAccumulator (10/10 passing)
- PersonaUser integration (10/10 passing)

### Integration Tests (TODO):
1. Manual dataset → genome/train → Verify adapter created
2. Chat history → genome/train → Verify adapter created
3. Capture interactions → Auto-training → Verify adapter created
4. Load corpus → Ingest → Train → Verify adapter created

### End-to-End Tests (TODO):
1. Train on dataset
2. Load adapter in Ollama
3. Generate response
4. Verify response quality improved

---

## Next Steps

1. Add `datasetPath` support to genome/train
2. Create simple test dataset (5 examples)
3. Test training end-to-end
4. Verify adapter file created
5. (Future) Load in Ollama and test inference

**Once this works, everything else is just different ways to build datasets.**
