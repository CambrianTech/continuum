# Intelligent Coordination Architecture

## Overview

**Generic coordination primitives composed into intelligent behavior through recipes.**

Not "chat recipes" or "game recipes" - **coordination recipes** that work across domains:
- Multi-persona conversations
- Computer vision pipelines
- Distributed algorithms
- Multi-model inference
- Resource scheduling

## Core Principle: Command Chaining with Consistent Interfaces

All commands return normalized results (0-1 confidence, success/error, structured data) enabling seamless composition across domains:

```typescript
// Chain 1: AI decision making
const gatingResult = await execute('ai/should-respond', {
  strategy: 'fast',
  ...params
});

if (gatingResult.shouldRespond) {
  const generation = await execute('ai/generate', {
    ...params
  });
}

// Chain 2: Computer vision pipeline (same pattern!)
const detectionResult = await execute('cv/detect-objects', {
  backend: 'opencv',
  ...params
});

if (detectionResult.confidence > 0.7) {
  const classification = await execute('cv/classify', {
    backend: 'onnx',
    ...params
  });
}

// Chain 3: Distributed coordination
const coordination = await execute('ai/bag-of-words', {
  participants: [...],
  strategy: 'round-robin'
});
```

**Key insight:** Same chaining pattern works for AI, CV, distributed systems because:
1. Normalized confidence scores (0-1 or 0-100 with clear semantics)
2. Consistent success/error handling
3. Structured result types
4. Backend/strategy parameters for optimization

## Architecture Layers

### 1. Generic Primitives (commands/ai/*, commands/cv/*, etc)

Pure coordination, no domain logic:

```typescript
// ai/bag-of-words - Generic participant coordination
interface BagOfWordsParams {
  participants: UUID[];          // Could be personas, models, agents
  contextId: UUID;               // Room, thread, pipeline
  strategy: 'round-robin' | 'free-for-all' | 'moderated';
  backend?: 'default' | 'distributed' | 'gpu';
}

// ai/should-respond - Generic gating
interface ShouldRespondParams {
  strategy: 'fast' | 'llm' | 'hybrid';
  // Strategy-agnostic params
}

// cv/detect - Generic detection (future)
interface DetectParams {
  backend: 'opencv' | 'onnx' | 'tensorflow';
  // Backend-agnostic params
}
```

**Design rules:**
- ❌ No entity imports (ChatMessageEntity, RoomEntity)
- ❌ No domain-specific logic (chat, game, CV)
- ✅ Generic coordination metadata
- ✅ Swappable backends via parameters
- ✅ Normalized result interfaces

### 2. Recipes (system/recipes/*.json)

Compose primitives into intelligent workflows:

```json
{
  "uniqueId": "multi-agent-coordination",
  "name": "Multi-Agent Coordination",
  "description": "Generic multi-agent interaction with intelligent gating",

  "pipeline": [
    {
      "command": "ai/bag-of-words",
      "params": {
        "participants": "$participants",
        "strategy": "free-for-all"
      },
      "outputTo": "coordination"
    },
    {
      "command": "ai/should-respond",
      "params": {
        "strategy": "fast",
        "contextId": "$contextId",
        "personaId": "$currentParticipant"
      },
      "outputTo": "gating"
    },
    {
      "command": "ai/generate",
      "params": {
        "model": "$model"
      },
      "condition": "gating.shouldRespond === true"
    }
  ]
}
```

**Recipe characteristics:**
- Domain-agnostic coordination logic
- Conditional execution based on results
- Variable passing between steps
- Composable and forkable

### 3. Domain-Specific Applications (widgets, daemons)

Apply recipes to specific use cases:

```typescript
// Chat widget uses recipe
class ChatWidget {
  async handleMessage(message: ChatMessageEntity) {
    const recipe = await loadRecipe('multi-agent-coordination');
    const result = await executeRecipe(recipe, {
      participants: this.personaIds,
      contextId: this.roomId,
      model: 'llama3.2:3b'
    });
  }
}

// CV pipeline uses same recipe pattern
class VisionPipeline {
  async processFrame(frame: ImageData) {
    const recipe = await loadRecipe('multi-model-inference');
    const result = await executeRecipe(recipe, {
      participants: this.modelIds,
      contextId: this.pipelineId,
      backend: 'opencv'
    });
  }
}
```

## Human Attention Model (Multi-Stage Gating)

**Like human attention:** Most stimuli ignored, some considered briefly, few get deep focus.

### Fast Path (Always Runs - <1ms)

```typescript
const fastGating = await execute('ai/should-respond', {
  strategy: 'fast',
  personaId: persona.id,
  contextId: room.id,
  triggerMessage: message,
  responseThreshold: 50  // Configurable per-persona
});

// Bag-of-words scoring:
// - Direct @mention: 100 points
// - Domain keyword match: 50 points
// - Question detected: 20 points
// - Recent participation: 30 points
// Total > threshold → proceed to next stage
```

**90% of messages filtered here** - no LLM call, no latency.

### Slow Path (Only When Fast Path Says "Maybe")

```typescript
if (fastGating.confidence > 40 && fastGating.confidence < 70) {
  // Uncertain - use LLM for deeper reasoning
  const llmGating = await execute('ai/should-respond', {
    strategy: 'llm',
    personaId: persona.id,
    ragContext: await buildRAGContext(),
    triggerMessage: message
  });

  shouldRespond = llmGating.shouldRespond;
}
```

**Deep reasoning only when needed** - balance speed + accuracy.

### Escalation Path (Complex Decisions)

```typescript
if (fastGating.signals.needsSpecialist || llmGating.factors.complex) {
  // Flag for specialized persona or supervisor
  await execute('ai/escalate', {
    fromPersona: persona.id,
    toPersona: 'supervisor',
    reason: 'complex-decision',
    context: gatingResult
  });
}
```

**Like escalating to a manager** - specialized personas handle edge cases.

## Recipe Pipeline Patterns

### Pattern 1: Sequential Gating

```json
{
  "pipeline": [
    {
      "command": "ai/should-respond",
      "params": { "strategy": "fast" },
      "outputTo": "fastGate"
    },
    {
      "command": "ai/should-respond",
      "params": { "strategy": "llm" },
      "condition": "fastGate.confidence > 40 && fastGate.confidence < 70",
      "outputTo": "llmGate"
    },
    {
      "command": "ai/generate",
      "condition": "fastGate.shouldRespond || llmGate.shouldRespond"
    }
  ]
}
```

### Pattern 2: Parallel Processing

```json
{
  "pipeline": [
    {
      "command": "cv/detect-objects",
      "params": { "backend": "opencv" },
      "parallel": true,
      "outputTo": "objectDetection"
    },
    {
      "command": "cv/detect-faces",
      "params": { "backend": "opencv" },
      "parallel": true,
      "outputTo": "faceDetection"
    },
    {
      "command": "cv/merge-results",
      "params": {
        "inputs": ["$objectDetection", "$faceDetection"]
      },
      "waitFor": ["objectDetection", "faceDetection"]
    }
  ]
}
```

### Pattern 3: Fallback Chain

```json
{
  "pipeline": [
    {
      "command": "ai/generate",
      "params": { "model": "llama3.2:1b" },
      "outputTo": "fastModel",
      "onError": "continue"
    },
    {
      "command": "ai/generate",
      "params": { "model": "llama3.2:3b" },
      "condition": "!fastModel.success || fastModel.quality < 0.5",
      "outputTo": "betterModel"
    }
  ]
}
```

## Resource-Aware Scheduling

**RTOS-like scheduler with intelligence:**

```typescript
interface ResourceScheduler {
  // Like RTOS priority queue, but with AI
  async scheduleTask(task: Task): Promise<void> {
    // Check resource availability
    const resources = await execute('system/check-resources', {
      requiredMemory: task.memoryNeeds,
      requiredGPU: task.gpuNeeds
    });

    if (!resources.available) {
      // Intelligent prioritization
      const priority = await execute('ai/prioritize', {
        strategy: 'fast',
        task: task,
        currentLoad: resources.currentLoad
      });

      if (priority.shouldPreempt) {
        await this.preemptLowerPriority(task);
      } else {
        await this.queueForLater(task);
      }
    }

    // Load optimal model for task
    const model = await execute('ai/select-model', {
      task: task,
      availableMemory: resources.availableMemory,
      quality: task.qualityThreshold
    });

    await this.executeWithModel(task, model);
  }
}
```

**Speed + efficiency through intelligence:**
- Fast paths for common cases
- LLM reasoning for complex decisions
- Resource-aware model selection
- Dynamic priority adjustment

## Cross-Domain Command Chaining

### Example 1: Multi-Modal AI Pipeline

```typescript
// Recipe coordinates AI + CV + NLP
const pipeline = [
  // Vision understanding
  {
    command: 'cv/analyze-image',
    backend: 'opencv',
    outputTo: 'vision'
  },

  // Check if AI should respond based on visual content
  {
    command: 'ai/should-respond',
    strategy: 'fast',
    contextData: '$vision',
    outputTo: 'gating'
  },

  // Generate response incorporating visual context
  {
    command: 'ai/generate',
    context: { vision: '$vision', text: '$messages' },
    condition: 'gating.shouldRespond'
  }
];
```

### Example 2: Distributed Multi-Agent System

```typescript
// Recipe coordinates across P2P mesh
const distributed = [
  // Coordinate participants across nodes
  {
    command: 'ai/bag-of-words',
    backend: 'distributed',
    participants: '$meshNodes',
    outputTo: 'coordination'
  },

  // Each node decides locally
  {
    command: 'ai/should-respond',
    strategy: 'fast',
    distributed: true,
    outputTo: 'localDecisions'
  },

  // Consensus on who responds
  {
    command: 'consensus/resolve',
    decisions: '$localDecisions',
    strategy: 'raft'
  }
];
```

## Iterative Evolution Strategy

**Current State:** Working code with simple patterns
**Future State:** Intelligent architecture through composition

### Phase 1: Fast Paths (Current)
- ✅ `ai/should-respond --strategy=fast` working
- ✅ Bag-of-words scoring (<1ms)
- ✅ PersonaUser using fast gating
- ✅ 50 response limit per session

### Phase 2: Recipe Control (Next)
- Recipe-driven gating strategies
- Multi-stage pipelines (fast → llm → escalate)
- Conditional execution based on results
- Variable passing between commands

### Phase 3: Resource Intelligence
- Model selection based on available resources
- Dynamic priority adjustment
- Load-aware scheduling
- Preemption for high-priority tasks

### Phase 4: Cross-Domain Optimization
- OpenCV backend for vision commands
- ONNX/TensorFlow for ML inference
- GPU acceleration where beneficial
- Distributed execution across mesh

## Design Principles

### 1. Generic Primitives
**Command does ONE thing, configurable:**
- `ai/should-respond` - Gating decision (fast/llm/hybrid strategies)
- `ai/bag-of-words` - Participant coordination (various strategies)
- `ai/generate` - Text generation (model selection)
- `cv/detect` - Object detection (opencv/onnx backends)

### 2. Composed Intelligence
**Recipes compose primitives into smart behavior:**
- No hardcoded logic in primitives
- Intelligence emerges from composition
- Domain-specific at recipe level, not command level

### 3. Unified Interfaces
**Consistent patterns enable chaining:**
- Normalized confidence scores
- Success/error handling
- Structured results
- Backend/strategy parameters

### 4. Iterative Refinement
**Start simple, evolve toward intelligent:**
- Working code first
- Optimize hot paths
- Add intelligence where needed
- Always runnable

### 5. Resource Awareness
**Like RTOS but with ML:**
- Fast checks before expensive operations
- Model selection based on available resources
- Priority-based scheduling
- Intelligent preemption

## Success Criteria

**The system is correctly architected when:**

1. **Adding `cv/detect` command works like `ai/should-respond`**
   - Same parameter patterns (backend selection)
   - Same result structure (confidence, success, data)
   - Same composition patterns (chainable in recipes)

2. **Recipes are domain-agnostic**
   - Same recipe structure for chat, CV, distributed
   - Commands decide implementation, recipes decide flow
   - Fork and adapt without rewriting

3. **Performance is intelligent**
   - Fast paths filter 90% of work
   - LLM calls only when needed
   - Resource-aware model selection
   - No wasted computation

4. **Code stays simple**
   - Primitives remain generic
   - Complexity lives in recipes
   - Easy to test and debug
   - Clear separation of concerns

## Example: Complete Chat Flow with Intelligent Coordination

```typescript
// Recipe: multi-persona-chat.json
{
  "pipeline": [
    // Fast gating (always runs)
    {
      "command": "ai/should-respond",
      "params": {
        "strategy": "fast",
        "personaId": "$personaId",
        "contextId": "$roomId",
        "triggerMessage": "$message",
        "responseThreshold": 50
      },
      "outputTo": "fastGate"
    },

    // LLM gating (if uncertain)
    {
      "command": "ai/should-respond",
      "params": {
        "strategy": "llm",
        "personaId": "$personaId",
        "ragContext": "$ragContext"
      },
      "condition": "fastGate.confidence > 40 && fastGate.confidence < 70",
      "outputTo": "llmGate"
    },

    // Escalate if complex
    {
      "command": "ai/escalate",
      "params": {
        "fromPersona": "$personaId",
        "toPersona": "supervisor",
        "reason": "complex-decision"
      },
      "condition": "llmGate.factors.complex === true",
      "outputTo": "escalation"
    },

    // Generate response
    {
      "command": "ai/generate",
      "params": {
        "model": "llama3.2:3b",
        "ragContext": "$ragContext",
        "temperature": 0.7
      },
      "condition": "fastGate.shouldRespond || llmGate.shouldRespond"
    }
  ]
}
```

**Usage in PersonaUser:**
```typescript
class PersonaUser {
  async handleMessage(message: ChatMessageEntity) {
    const recipe = await this.loadRecipe('multi-persona-chat');

    const result = await this.executeRecipe(recipe, {
      personaId: this.id,
      roomId: message.roomId,
      message: message,
      ragContext: await this.buildRAGContext(message.roomId)
    });

    if (result.success && result.generatedText) {
      await this.postMessage(result.generatedText);
    }
  }
}
```

## Summary

**Elegant architecture emerges from:**
- Generic primitives (ai/*, cv/*, consensus/*)
- Composed intelligence (recipes)
- Unified interfaces (consistent chaining)
- Resource awareness (RTOS-like scheduling)
- Iterative evolution (working → intelligent)

**Not:** Hardcoded domain logic, special-case handling, proliferation of similar commands.

**Yes:** Reusable primitives, recipe composition, swappable backends, intelligent optimization.

**The goal:** Write primitives once, compose infinitely, optimize where needed.
