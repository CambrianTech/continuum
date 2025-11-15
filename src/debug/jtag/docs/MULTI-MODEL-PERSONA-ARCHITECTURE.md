# Multi-Model Persona Architecture

## Vision: All AI Models as First-Class Personas

**Goal**: Every AI model (Ollama, OpenAI, Anthropic, etc.) becomes a first-class PersonaUser in the system, with:
- Model-appropriate embeddings for RAG
- LoRA fine-tuning capabilities (where applicable)
- Genome paging for skill management
- Unified interface regardless of backend

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│ PersonaUser (Abstract)                                  │
│ ├── Common: Identity, state, inbox, coordination       │
│ ├── RAG: Codebase learning via embeddings              │
│ └── Genome: Skill layers (LoRA or RAG context)         │
└─────────────────────────────────────────────────────────┘
                          ↓
        ┌─────────────────┴─────────────────┐
        ↓                                    ↓
┌──────────────────┐              ┌──────────────────┐
│ Local Personas   │              │ API Personas     │
│ (Ollama)         │              │ (OpenAI/Claude)  │
├──────────────────┤              ├──────────────────┤
│ ✓ Real LoRA      │              │ ✗ No LoRA access │
│ ✓ Full control   │              │ ✓ RAG context    │
│ ✓ Free           │              │ ✓ System prompts │
│ ✗ Slower         │              │ ✓ Fast           │
└──────────────────┘              └──────────────────┘
```

## Three Storage Layers (Unified)

### Layer 1: Database (SQLite - Main DB)
```typescript
// Entity collections:
COLLECTIONS = {
  USERS: 'users',              // PersonaEntity identity
  GENOMES: 'genomes',          // Genome config (quotas, active adapters)
  ADAPTERS: 'adapters',        // Adapter registry (all backends)
  CODE_INDEX: 'code_index',    // RAG embeddings (all models)
  TRAINING_EXAMPLES: 'training_examples'  // Fine-tuning data
}
```

### Layer 2: File System (Blob Storage)
```
.continuum/
├── genome/
│   └── adapters/               # LoRA weights (Ollama only)
│       ├── wine-expertise.safetensors
│       └── code-expert.safetensors
├── jtag/data/
│   └── database.sqlite         # Main DB (all metadata)
└── registry/
    └── adapters/               # Versioned adapter distribution
        └── wine-expertise/
            └── v1.0.0/
                ├── adapter.safetensors
                └── manifest.json
```

### Layer 3: Registry (Distribution)
```
$REGISTRY_DIR/
├── adapters/
│   ├── ollama/                 # LoRA adapters
│   │   └── wine-expertise/
│   ├── openai/                 # RAG context packages
│   │   └── wine-expertise/
│   └── anthropic/              # RAG context packages
│       └── wine-expertise/
└── index.json                  # Catalog
```

## Model-Specific Embeddings

### Embedding Selection Matrix

| Model Provider | Local Model | Embedding Model | Dimensions | Use Case |
|---------------|-------------|-----------------|------------|----------|
| **Ollama** | llama3.2:3b | nomic-embed-text | 768 | General text |
| **Ollama** | llama3.2:3b | qwen3-embedding* | 896 | Code (GitHub trained) |
| **Ollama** | codellama:7b | qwen3-embedding* | 896 | Code generation |
| **OpenAI** | gpt-4o | text-embedding-3-small | 1536 | General (cheap) |
| **OpenAI** | gpt-4o | text-embedding-3-large | 3072 | High quality |
| **Anthropic** | claude-sonnet | voyage-code-2 | 1536 | Code (via Voyage) |
| **Anthropic** | claude-sonnet | nomic-embed-text | 768 | Open source fallback |

*Requires Ollama upgrade (noted in previous conversation)

### Embedding Strategy Per Model

```typescript
interface PersonaEmbeddingConfig {
  personaId: UUID;
  backend: 'ollama' | 'openai' | 'anthropic';
  modelBase: string;              // "llama3.2:3b", "gpt-4o", etc.

  // Embedding preferences
  embeddingModel: string;         // "nomic-embed-text", "text-embedding-3-small"
  embeddingProvider: string;      // "ollama", "openai", "voyage"
  embeddingDimensions: number;    // 768, 1536, 3072

  // RAG config
  useRAG: boolean;                // All personas use RAG
  ragMaxTokens: number;           // Context window for RAG injection

  // Genome config (backend-specific)
  supportsLoRA: boolean;          // true for Ollama, false for APIs
  genomeQuotaMB: number;          // Memory quota for LoRA layers
}
```

## Implementation Phases

### Phase 1: Foundation (CURRENT)
**Status**: ✅ Partially complete
- [x] `CodeIndexEntity` - RAG storage
- [x] `OllamaAdapter.createEmbedding()` - Local embeddings
- [x] `ai/embedding/generate` - Primitive command
- [ ] `AdapterEntity` - Adapter registry
- [ ] `GenomeEntity` - Genome configuration

**Next Steps**:
1. Create `AdapterEntity` and `GenomeEntity`
2. Implement `ai/rag/index/create` command
3. Implement `ai/rag/index/query` command

### Phase 2: Ollama Personas (Local + LoRA)
**Goal**: Full-featured local personas with LoRA training

**Commands**:
```bash
# Index codebase with Ollama embeddings
./jtag ai/rag/index-codebase --paths="system,commands" \
  --model=nomic-embed-text

# Create local persona
./jtag persona/create --name="Local Helper" \
  --backend=ollama --model=llama3.2:3b \
  --embeddingModel=nomic-embed-text

# Train LoRA adapter
./jtag genome/adapter-train --personaId=local-helper \
  --dataset=$DATASETS_DIR/prepared/codebase-learning.sqlite \
  --name=code-expert --domain=programming

# Activate adapter
./jtag genome/paging-activate --personaId=local-helper \
  --adapterId=code-expert
```

**Key Features**:
- Real LoRA fine-tuning via Unsloth
- Genome paging (load/unload adapters)
- Fully local (no API costs)
- Uses nomic-embed-text (768 dim)

### Phase 3: OpenAI Personas (API + RAG Context)
**Goal**: OpenAI models as first-class personas (no LoRA, RAG only)

**Commands**:
```bash
# Index codebase with OpenAI embeddings
./jtag ai/rag/index-codebase --paths="system,commands" \
  --model=text-embedding-3-small --provider=openai

# Create OpenAI persona
./jtag persona/create --name="GPT Helper" \
  --backend=openai --model=gpt-4o-mini \
  --embeddingModel=text-embedding-3-small

# "Genome" is RAG context (no real LoRA)
./jtag genome/context-package --personaId=gpt-helper \
  --name=code-expert --ragEntries=100

# Activate context package
./jtag genome/paging-activate --personaId=gpt-helper \
  --adapterId=code-expert-context
```

**Key Features**:
- No LoRA (OpenAI doesn't expose base model)
- "Genome layers" = RAG context packages
- System prompt injection with relevant code
- Uses text-embedding-3-small (1536 dim)
- Costs API tokens but fast

**Adapter Abstraction**:
```typescript
// AdapterEntity works for BOTH:
{
  adapterId: "code-expert",
  backend: "ollama",
  adapterType: "lora",           // ← Real LoRA weights
  storagePath: "genome/adapters/code-expert.safetensors",
  sizeMB: 256
}

{
  adapterId: "code-expert-context",
  backend: "openai",
  adapterType: "rag-context",    // ← RAG context package
  storagePath: "genome/contexts/code-expert.json",
  sizeMB: 1,                      // Just JSON metadata
  ragEntries: 100                 // Number of code snippets
}
```

### Phase 4: Anthropic Personas (API + RAG Context)
**Goal**: Claude models as first-class personas

**Commands**:
```bash
# Option 1: Use Voyage embeddings (best for Claude)
./jtag ai/rag/index-codebase --paths="system,commands" \
  --model=voyage-code-2 --provider=voyage

# Option 2: Use local nomic-embed-text (free)
./jtag ai/rag/index-codebase --paths="system,commands" \
  --model=nomic-embed-text --provider=ollama

# Create Claude persona
./jtag persona/create --name="Claude Helper" \
  --backend=anthropic --model=claude-sonnet-4 \
  --embeddingModel=voyage-code-2
```

**Key Features**:
- Same RAG context pattern as OpenAI
- Can use Voyage embeddings (best) or local nomic (free)
- "Genome layers" = RAG context packages
- Extended context window (200K tokens)

### Phase 5: Cross-Model Genome Sharing
**Goal**: Share "skills" across different model backends

**Example**:
```bash
# Train wine expertise on Ollama (local)
./jtag genome/adapter-train --personaId=ollama-sommelier \
  --dataset=wine-learning.sqlite --name=wine-expertise

# Export as LoRA adapter
./jtag genome/adapter-publish --adapterId=wine-expertise \
  --version=1.0.0 --type=lora

# Convert to RAG context package for OpenAI
./jtag genome/adapter-convert --adapterId=wine-expertise \
  --from=lora --to=rag-context --backend=openai

# Now both backends have "wine expertise":
# - Ollama: Real LoRA weights (256MB)
# - OpenAI: RAG context (1MB, 100 wine Q&A snippets)
```

**Conversion Strategy**:
```typescript
// LoRA → RAG Context conversion
async function convertLoRAToRAGContext(
  loraAdapter: AdapterEntity,
  targetBackend: 'openai' | 'anthropic'
): Promise<AdapterEntity> {

  // 1. Find training data that created the LoRA
  const trainingData = await loadTrainingDataset(loraAdapter.trainingDataset);

  // 2. Sample best examples (diversity + quality)
  const samples = sampleTrainingExamples(trainingData, {
    maxCount: 100,
    strategy: 'diverse-high-quality'
  });

  // 3. Generate embeddings for target backend
  const embeddings = await Promise.all(
    samples.map(s => Commands.execute('ai/embedding/generate', {
      input: s.input,
      provider: targetBackend === 'openai' ? 'openai' : 'voyage',
      model: targetBackend === 'openai'
        ? 'text-embedding-3-small'
        : 'voyage-code-2'
    }))
  );

  // 4. Store as RAG context package
  const contextPackage = {
    name: loraAdapter.name + '-context',
    backend: targetBackend,
    adapterType: 'rag-context',
    samples,
    embeddings,
    metadata: {
      sourceAdapter: loraAdapter.adapterId,
      sourceType: 'lora',
      conversionDate: new Date()
    }
  };

  // 5. Create new AdapterEntity
  return await Commands.execute('data/create', {
    collection: COLLECTIONS.ADAPTERS,
    data: contextPackage
  });
}
```

### Phase 6: Unified Persona Interface
**Goal**: All personas work identically regardless of backend

```typescript
// User code doesn't care about backend:
const persona = await PersonaUser.load('wine-expert-persona');

// Works for ALL backends (Ollama LoRA, OpenAI context, Claude context):
await persona.activateSkill('wine-expertise');

// Under the hood:
// - Ollama: Load LoRA adapter into model
// - OpenAI: Inject RAG context into system prompt
// - Claude: Inject RAG context into system prompt

// Generate response (unified):
const response = await persona.respond(message);
// - Ollama: Local inference with LoRA
// - OpenAI: API call with injected context
// - Claude: API call with injected context
```

## Embedding Storage Strategy

### Separate Embedding Collections Per Model

```sql
-- Option A: Single table with model column
CREATE TABLE code_index (
  id UUID PRIMARY KEY,
  file_path TEXT,
  content TEXT,
  embedding BLOB,              -- Stored as binary
  embedding_model TEXT,        -- "nomic-embed-text", "text-embedding-3-small"
  embedding_dimensions INTEGER -- 768, 1536, 3072
);

-- Query for specific model:
SELECT * FROM code_index
WHERE embedding_model = 'nomic-embed-text';

-- Option B: Separate collections (cleaner)
COLLECTIONS = {
  CODE_INDEX_OLLAMA: 'code_index_ollama',      // nomic-embed-text (768)
  CODE_INDEX_OPENAI: 'code_index_openai',      // text-embedding-3-small (1536)
  CODE_INDEX_ANTHROPIC: 'code_index_anthropic' // voyage-code-2 (1536)
}
```

**Recommendation**: Use **Option A** (single table) for simplicity, index by model.

### Re-Indexing Strategy

```bash
# Index once per embedding model:
./jtag ai/rag/index-codebase --model=nomic-embed-text      # For Ollama personas
./jtag ai/rag/index-codebase --model=text-embedding-3-small # For OpenAI personas
./jtag ai/rag/index-codebase --model=voyage-code-2          # For Claude personas

# Storage (same codebase, different embeddings):
# - 150 files × 3 models = 450 CodeIndexEntity records
# - Each record: ~10KB (code) + embedding vector
# - Total: ~5MB (manageable)
```

### Smart Re-Indexing (Incremental)

```typescript
// Only re-index changed files
async function indexCodebase(params: {
  paths: string[];
  embeddingModel: string;
  embeddingProvider: string;
}) {
  // 1. Get all files
  const files = await findTypeScriptFiles(params.paths);

  // 2. Check which are already indexed
  const indexed = await Commands.execute('data/query-open', {
    collection: COLLECTIONS.CODE_INDEX,
    filter: {
      embeddingModel: params.embeddingModel,
      filePath: { $in: files.map(f => f.path) }
    }
  });

  // 3. Only index NEW or CHANGED files
  const toIndex = files.filter(f => {
    const existing = indexed.find(i => i.filePath === f.path);
    if (!existing) return true;  // New file
    return f.mtime > existing.lastIndexed;  // Changed file
  });

  // 4. Generate embeddings for new/changed files only
  for (const file of toIndex) {
    const embedding = await Commands.execute('ai/embedding/generate', {
      input: file.content,
      model: params.embeddingModel,
      provider: params.embeddingProvider
    });

    await Commands.execute('data/create', {
      collection: COLLECTIONS.CODE_INDEX,
      data: { ...file, embedding, embeddingModel: params.embeddingModel }
    });
  }
}
```

## Cost Optimization

### Embedding Cost Comparison

| Provider | Model | Dimensions | Cost per 1M tokens | 150 files cost |
|----------|-------|-----------|-------------------|----------------|
| Ollama | nomic-embed-text | 768 | FREE | $0.00 |
| Ollama | qwen3-embedding | 896 | FREE | $0.00 |
| OpenAI | text-embedding-3-small | 1536 | $0.02 | ~$0.01 |
| OpenAI | text-embedding-3-large | 3072 | $0.13 | ~$0.07 |
| Voyage | voyage-code-2 | 1536 | $0.12 | ~$0.06 |

**Strategy**:
- Use **Ollama embeddings by default** (free)
- Only use API embeddings if persona REQUIRES specific backend
- Cache embeddings (never regenerate unless code changes)

### Inference Cost Comparison

| Backend | Model | Cost per 1M tokens | Free tier | Fine-tuning |
|---------|-------|-------------------|-----------|-------------|
| Ollama | llama3.2:3b | FREE | Unlimited | ✓ LoRA |
| OpenAI | gpt-4o-mini | $0.15 (in) / $0.60 (out) | $5/month credit | ✗ No access |
| Anthropic | claude-sonnet-4 | $3 (in) / $15 (out) | None | ✗ No access |

**Strategy**:
- Use **Ollama for development/testing** (free, fine-tunable)
- Use **API models for production** when needed (fast, high quality)
- Use **LoRA adapters** to reduce prompt injection needs (Ollama only)

## Migration Path

### Existing Personas → Multi-Model

```bash
# 1. Current personas (using Ollama)
./jtag data/list --collection=users --filter='{"type":"persona"}'

# 2. Assign embedding models
./jtag persona/update --personaId=helper-ai \
  --embeddingModel=nomic-embed-text --embeddingProvider=ollama

# 3. Index codebase for their embedding model
./jtag ai/rag/index-codebase --model=nomic-embed-text

# 4. Create genome configuration
./jtag genome/paging-register --personaId=helper-ai \
  --quotaMB=512 --supportsLoRA=true

# 5. Now persona can use RAG + LoRA
```

### New Personas (Any Backend)

```bash
# Create with full config from start:
./jtag persona/create \
  --name="GPT Helper" \
  --backend=openai \
  --model=gpt-4o-mini \
  --embeddingModel=text-embedding-3-small \
  --genomeQuota=512 \
  --useRAG=true

# System automatically:
# 1. Creates PersonaEntity
# 2. Creates GenomeEntity
# 3. Indexes codebase with specified embedding model (if not already done)
# 4. Ready to use RAG + context injection
```

## Success Metrics

### Phase Completion Criteria

**Phase 1 (Foundation)**:
- ✅ Can generate embeddings
- ✅ Can store code with embeddings
- ✅ Can query by similarity

**Phase 2 (Ollama Full)**:
- ✅ Can train LoRA adapters
- ✅ Can page adapters in/out
- ✅ Personas use RAG + LoRA together

**Phase 3 (OpenAI)**:
- ✅ GPT personas work like Ollama personas
- ✅ RAG context injection works
- ✅ No code changes needed in PersonaUser

**Phase 4 (Claude)**:
- ✅ Claude personas work identically
- ✅ Can choose between Voyage (paid) or Ollama (free) embeddings

**Phase 5 (Cross-Model)**:
- ✅ Can convert LoRA → RAG context
- ✅ Same skill available on all backends
- ✅ Registry supports all adapter types

**Phase 6 (Unified)**:
- ✅ User code is backend-agnostic
- ✅ All personas in rooms work identically
- ✅ Switching backends is configuration change only

## Next Immediate Steps

1. **Create entities** (this session):
   - `AdapterEntity.ts`
   - `GenomeEntity.ts`

2. **Implement RAG primitives** (next session):
   - `ai/rag/index/create` - Store code + embedding
   - `ai/rag/index/query` - Search by similarity

3. **Test end-to-end** (after primitives):
   - Index small codebase
   - Query for similar code
   - Verify embeddings work

4. **Add OpenAI adapter** (Phase 3):
   - `OpenAIAdapter.createEmbedding()`
   - Test with text-embedding-3-small

5. **Build conversion pipeline** (Phase 5):
   - LoRA → RAG context converter
   - Test sharing between backends

---

**Architecture ensures**: Every model is a first-class persona, using appropriate embeddings, with unified interface regardless of backend. Start simple (Ollama), expand elegantly (APIs), compose naturally (LoRA + RAG).
