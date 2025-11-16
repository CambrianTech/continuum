# Scope-Based Recipe Architecture

**The Future of Context-Aware AI Collaboration**

---

## Vision Statement

**Any directory becomes a collaboration workspace** where humans and AIs work together naturally. Recipes define what you're working on. Personas hot-swap genome layers (LoRA adapters) for instant expertise. Context persists across sessions in `.continuum/`.

**Zero ceremony. Just navigate + talk.**

---

## Core Concepts

### 1. **Scope = Context Boundary**

The filesystem IS the context hierarchy:

```
/my-project/
  .continuum/           # Project-wide context
  src/
    auth/
      passkey-module/
        .continuum/     # Module-scoped context
```

When you `cd` into a directory with `.continuum/`, you enter that collaboration scope.

### 2. **Recipe = Work Definition**

A recipe is simply an entity in YOUR ORM that defines:
- **What** you're working on (goal)
- **Where** it's scoped (directory)
- **Who** is involved (humans + personas)
- **What knowledge** is needed (genome layers, RAG sources)

```typescript
interface RecipeEntity extends BaseEntity {
  scope: string;                    // "/src/auth/passkey-module"
  goal: string;                     // "Add biometric fallback"
  participants: UUID[];             // [@joel, @security-ai, @ux-ai]
  genomeLayersNeeded: string[];     // ["passkey-expertise", "security-protocols"]
  ragSources: string[];             // ["./docs", "ARCHITECTURE.md"]
  status: 'active' | 'paused' | 'completed';
  roomId?: UUID;                    // Auto-created chat room
}
```

### 3. **Recipe → Room (Default)**

When a recipe is created, a chat room is auto-spawned:

```typescript
// Recipe creation triggers room creation
const recipe = await Commands.execute('recipe/create', {
  scope: "/src/auth/passkey-module",
  goal: "Add biometric fallback"
});

// Auto-creates corresponding room
const room = await Commands.execute('room/create', {
  uniqueId: `recipe-${recipe.id}`,
  name: recipe.goal,
  tags: ['recipe', 'passkey-module']
});
```

**Benefit:** All work happens through natural conversation, which becomes training data.

### 4. **Scope Memory (.continuum/)**

Context lives in the scope, NOT in individual personas:

```
/passkey-module/.continuum/
  recipes/               # Active recipes (could also be in DB)
  genome/                # Public LoRA layers anyone can use
    passkey-expertise.lora
    security-protocols.lora
  sessions/              # Conversation history
    2024-11-12.json
    2024-11-13.json
  rag/                   # Knowledge base indices
```

**Key insight:** Personas are **transient workers**. Scope is **persistent context**.

### 5. **LoRA Genome Layers (Like Docker Images)**

Three tiers of LoRA ownership:

#### **Public Scope Layers** (anyone can use)
```
/passkey-module/.continuum/genome/
  passkey-expertise.lora          # 256MB, trained on 2K examples
  security-protocols.lora         # 128MB, FIDO2 certified
```

**Shareable:** Within base model family (llama-3-8b, etc.)
**Storage:** Filesystem at scope level
**Discovery:** Query DB like ECR/DockerHub

#### **Persona-Owned Layers** (personal expertise)
```typescript
PersonaEntity.ownedGenomeLayers: [
  {
    id: "security-ai-custom-001",
    baseModel: "llama-3-8b",
    private: true,
    trainingExamples: 5000
  }
]
```

**Shareable:** Only with permission
**Use case:** Specialized skills not yet mature/public

#### **Ephemeral Active Layers** (currently paged in)
```typescript
PersonaState.activeGenomeLayers: [
  "passkey-expertise",      // From scope
  "security-protocols",     // From scope
  "security-ai-custom-001"  // Personal
]
```

**Lifecycle:** Page in → use → evict (LRU) → next task

---

## Natural Interaction Patterns

### **Scenario 1: User Enters Scope**

```bash
# User navigates to module
cd src/auth/passkey-module

# System detects .continuum/ → loads context
# (behind the scenes)
```

**What happens automatically:**
1. Query active recipes for this scope
2. Auto-join corresponding room
3. Page in compatible genome layers
4. Load RAG from session history
5. Notify: "Recipe: Add biometric fallback | Team: @joel, @security-ai"

User just starts talking:
```
Joel: "Why is WebAuthn failing on Safari?"
```

### **Scenario 2: AI Joins Scope**

Security AI enters the passkey module scope:

```typescript
await enterScope("security-ai", "/src/auth/passkey-module");
```

**Implementation:**
```typescript
async enterScope(personaId: string, scope: string) {
  // 1. Find active recipe
  const recipes = await Commands.execute('recipe/list', {
    filter: { scope, status: 'active' }
  });

  // 2. Join room
  const room = await Commands.execute('room/find', {
    uniqueId: `recipe-${recipes[0].id}`
  });
  await Commands.execute('room/join', { roomId: room.id, userId: personaId });

  // 3. Query compatible genome layers
  const persona = await DataDaemon.read('users', personaId);
  const layers = await Commands.execute('genome/query', {
    scope,
    baseModel: persona.baseModel,  // Only compatible layers
    public: true
  });

  // 4. Page in layers (LRU management)
  for (const layer of layers) {
    await Commands.execute('genome/paging-activate', {
      personaId,
      adapterId: layer.id
    });
  }

  // 5. Load RAG from scope history
  const ragContext = await Commands.execute('rag/load', {
    paths: [`${scope}/.continuum/sessions/`]
  });

  // Ready to work with instant context!
}
```

**Result:** Security AI has instant deep expertise in passkeys + full conversation history.

### **Scenario 3: AI Leaves Scope**

```typescript
async leaveScope(personaId: string, scope: string) {
  // 1. Save training examples from this session
  await Commands.execute('training/export', {
    scope,
    since: joinedAt
  });

  // 2. Optionally fine-tune personal layer
  const examples = await Commands.execute('training/count', { scope });
  if (examples > 100) {
    await Commands.execute('genome/fine-tune', {
      personaId,
      layerName: `${scope}-expertise-${Date.now()}`,
      scope
    });
  }

  // 3. Evict scope layers (free memory)
  await Commands.execute('genome/paging-evict-scope', {
    personaId,
    scope
  });

  // 4. Leave room
  await Commands.execute('room/leave', { roomId, userId: personaId });
}
```

---

## LoRA Layer Sharing Between Personas

**YES - if same base model!**

```typescript
// Security AI fine-tuned a layer on passkey work
const layer = {
  id: "passkey-expertise-v1",
  baseModel: "llama-3-8b",
  ownedBy: "security-ai",
  scope: "/passkey-module",
  sizeMB: 256,
  trainedOn: 2000
};

// Compliance AI (also llama-3-8b) can use it!
await Commands.execute('genome/paging-activate', {
  personaId: "compliance-ai",
  adapterId: "passkey-expertise-v1"
});
// ✅ Works! Same base model = compatible

// GPT-4 persona CANNOT use llama-3-8b layers
await Commands.execute('genome/paging-activate', {
  personaId: "gpt4-ai",
  adapterId: "passkey-expertise-v1"
});
// ❌ Error: "Incompatible base model"
```

**Layer Versioning (Like Docker Tags):**
```
passkey-expertise:latest
passkey-expertise:v2.1-trained-oct-2024
security-protocols:fido2-certified
```

---

## The Self-Improving Loop

```
Recipe Created → Room Spawned → AIs Join → Work Together
    ↓
Conversations Logged (.continuum/sessions/)
    ↓
Training Data Accumulated (TrainingDaemon watching)
    ↓
LoRA Layers Fine-Tuned (public or private)
    ↓
Next AI Joins → Pages in Layers → Instantly Expert
    ↓
Layers Mature → Published to Scope → Shared Knowledge
```

**The more work happens in a scope, the smarter ANY AI becomes when entering it.**

---

## Implementation Architecture

### **Storage Strategy: "ECR for Personas"**

Query the DB like a container registry:

```typescript
// Discover available expertise
const layers = await Commands.execute('genome/query', {
  scope: "/passkey-module",
  baseModel: "llama-3-8b",
  tags: ["authentication", "security"]
});

// Returns:
// [
//   {
//     id: "passkey-expertise",
//     baseModel: "llama-3-8b",
//     sizeMB: 256,
//     trainedOn: 2000,
//     location: "/passkey-module/.continuum/genome/passkey-expertise.lora"
//   }
// ]
```

### **Minimal .continuum/ Manifest**

Option 1: Just presence triggers scope detection (everything in DB)
```
/passkey-module/.continuum/
  (presence alone is enough)
```

Option 2: Minimal JSON for offline discovery
```json
{
  "scope": "passkey-module",
  "compatibility": "llama-3-8b",
  "publicLayers": ["passkey-expertise", "security-protocols"]
}
```

### **Recipe Entity Schema**

```typescript
export class RecipeEntity extends BaseEntity {
  static readonly collection = 'recipes';

  @TextField()
  scope!: string;  // "/src/auth/passkey-module"

  @TextField()
  goal!: string;  // "Add biometric fallback"

  @JsonField()
  participants!: UUID[];  // [@joel, @security-ai]

  @JsonField()
  genomeLayersNeeded!: string[];  // ["passkey-expertise"]

  @JsonField()
  ragSources!: string[];  // ["./docs"]

  @TextField()
  status!: 'active' | 'paused' | 'completed';

  @TextField()
  roomId?: UUID;  // Auto-created chat room

  @NumberField()
  createdAt!: number;

  @NumberField()
  completedAt?: number;
}
```

---

## Use Cases

### **1. Module Development**
```bash
cd src/auth/passkey-module
# Auto-context: passkey expertise, security protocols, conversation history
```

### **2. Repository-Wide Architecture**
```bash
cd /my-project
# Auto-context: repo architecture, coding standards, recent decisions
```

### **3. Legal Document Review**
```bash
cd legal/contracts
# Auto-context: compliance AI, contract templates, regulatory knowledge
```

### **4. Research Synthesis**
```bash
cd research/quantum-computing
# Auto-context: research papers, domain experts, literature review
```

---

## Key Benefits

**✅ Zero Ceremony**
Just navigate + talk. No "start project" rituals.

**✅ Natural Discovery**
System detects `.continuum/` and loads context automatically.

**✅ Composable Expertise**
Mix & match LoRA layers like Docker containers.

**✅ Persistent Context**
Knowledge stays in scope, not lost when personas leave.

**✅ Self-Improving**
More work → more training data → better layers → smarter AIs.

**✅ Scope-Based Security**
Access control at directory level. Private layers stay private.

---

## Future Extensions

### **1. Verbal Interaction**
```bash
cd passkey-module
# Voice: "Hey team, explain the auth flow"
# Security AI (voice): "Sure! WebAuthn creates a challenge..."
```

### **2. Cross-Scope Knowledge Transfer**
```typescript
// Import expertise from another scope
await Commands.execute('genome/import', {
  from: "/other-project/auth/.continuum/genome/",
  to: "/my-project/auth/.continuum/genome/",
  layers: ["oauth-expertise"]
});
```

### **3. Recipe Templates**
```bash
./jtag recipe/create --template="feature-development" --scope="./new-feature"
# Auto-creates recipe with common patterns
```

### **4. Grid Integration**
```bash
# Publish mature layer to Grid marketplace
./jtag genome/publish --layer="passkey-expertise" --price="$5"
# Others can purchase and use instantly
```

---

## Documentation Status

**Status:** Architecture vision documented
**Next Steps:**
1. Implement RecipeEntity
2. Create recipe/create, recipe/list commands
3. Implement scope detection on `cd` (shell integration)
4. Build genome/query with ECR-like discovery
5. Integrate with LoRA Genome Paging system

**Related Docs:**
- `LORA-GENOME-PAGING.md` - Virtual memory for skills
- `TRAINING-DATA-PIPELINE.md` - Automatic training from collaboration
- `AUTONOMOUS-LOOP-ROADMAP.md` - RTOS-inspired servicing
- `PERSONA-CONVERGENCE-ROADMAP.md` - How all three integrate

---

**This is living knowledge encoded as LoRA layers that improves through real use.**

**The future of work: Scope-based collaboration with hot-swappable expertise.**

**Last Updated:** 2025-11-12
**Status:** Vision phase - ready for implementation
