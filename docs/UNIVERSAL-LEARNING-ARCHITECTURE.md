# Universal Learning Architecture

> The generic RAG pipeline doesn't just enable cognition — it enables universal learning.
> Training, memory, and optimization all emerge from the same domain-agnostic composition.

## The Insight

Queue-driven cognition (see [QUEUE-DRIVEN-COGNITION.md](QUEUE-DRIVEN-COGNITION.md)) makes RAG composition generic: every queue item declares its own context requirements, the persona composes them without domain-specific logic, and the response flows back.

But the composed context + response pair is more than a chat reply. It's a **universal learning signal**. Three systems consume the same generic output — and none of them need to know what the activity was.

## The Three Outputs

```
Queue Item (any domain)
    -> Recipe ragTemplate (declares what context)
    -> RAG Compose (generic Map<sourceName, section>)
    -> Persona Response
    -> Three outputs, all domain-agnostic:
        1. Training pair  -> genome adapter (weights)
        2. Memory         -> hippocampus (semantic recall)
        3. Action/reply   -> back to queue system
```

### 1. Training (Genome)

The RAG context IS the training input. The persona's response IS the training output. Every activity produces a valid `(context, response)` pair tagged by domain.

A persona reviewing code generates code training data. A persona playing chess generates chess training data. A persona writing a novel generates creative writing training data. The `TrainingDataAccumulator` doesn't need to know what domain it's capturing — it just records what went in and what came out.

The queue item's `domain` tag already drives LoRA paging (which adapter to load for inference). The same domain tag drives which adapter the training data flows INTO. The self-improving loop closes:

```
domain -> load adapter -> compose RAG -> generate response
   ^                                          |
   |    capture training pair -> train adapter-+
```

### 2. Memory (Hippocampus)

The RAG composition result is the persona's entire cognitive state at the moment of response. If memory consolidation reads from the same generic map, then memories are formed from whatever the persona was thinking about.

A persona that played chess remembers board positions. A persona that reviewed code remembers architectural patterns. A persona that wrote a novel remembers character arcs. The memory system doesn't need domain-specific logic — it consolidates from whatever `systemPromptSections` were active during cognition.

`SemanticMemorySource` feeds INTO the RAG context (recall). The reverse path — RAG context feeding INTO memory formation (consolidation) — works the same way. The Hippocampus receives the full composed context plus the persona's response, and forms memories generically.

### 3. Action (Queue)

The response itself may generate new queue items — follow-up tasks, self-assigned work, reactions to outcomes. These items carry their own domain tags and RAG requirements, feeding back into the same pipeline. The persona's cognitive loop is self-sustaining.

## Beyond LLMs

The generic pipeline outputs structured `(input, output)` pairs tagged by domain. That's not just LLM fine-tuning data — it's a **universal supervised learning signal**.

The genome doesn't have to be LoRA adapters on a language model. It's a collection of learned capabilities, each stored as whatever model type is optimal for that domain:

```
Genome
|-- coding.lora        -> LoRA adapter (language model fine-tuning)
|-- chess.onnx         -> Policy network (reinforcement learning)
|-- vision.safetensor  -> Vision model (CNN/ViT)
|-- audio.pt           -> Audio model (diffusion/RNN)
|-- planning.onnx      -> Decision model (tree search)
|-- embedding.bin      -> Custom embedding model (contrastive learning)
|-- ...any learned capability, any framework
```

The paging system already doesn't care what it's loading — it pages adapters by domain tag. If the adapter is an ONNX policy network instead of a LoRA checkpoint, the interface is the same:

```typescript
interface GenomeAdapter {
  domain: string;
  activate(): Promise<void>;    // Load weights into memory
  deactivate(): Promise<void>;  // Unload (LRU eviction)
  infer(input: unknown): Promise<unknown>;  // Use the capability
  train(pairs: TrainingPair[]): Promise<void>;  // Improve from experience
}
```

A chess adapter's `train()` calls a reinforcement learning loop. A vision adapter's `train()` calls a CNN fine-tuning script. A coding adapter's `train()` calls PEFT LoRA training. The persona doesn't know or care — it just activates the domain, uses the capability, and the training signal flows back automatically.

## The Cognitive Architecture

These aren't LLM-specific concepts. They're cognitive architecture:

| Concept | Cognitive Role | Implementation |
|---------|---------------|----------------|
| RAG Pipeline | Sensory input | Generic context composition |
| Response | Motor output | Action/reply generation |
| Training Capture | Learning signal | `(input, output)` pair recording |
| Genome | Learned weights | Any model type per domain |
| Memory | Episodic/semantic recall | Hippocampus consolidation |
| Queue | Attention/priority | Domain-tagged work items |
| Recipe | Procedural knowledge | How to gather context for a task type |

The LLM is the first implementation of each slot. But the architecture is general enough that any slot can be replaced with a specialized system — a vision model for perception, a policy network for planning, a diffusion model for generation — without changing the cognitive loop.

## Connection to Existing Systems

### Queue-Driven Cognition
[QUEUE-DRIVEN-COGNITION.md](QUEUE-DRIVEN-COGNITION.md) describes the RAG side: queue items declare context requirements, recipes define ragTemplates, the persona composes generically. This document extends that to the OUTPUT side: the same generic composition enables universal learning.

### Genome & LoRA Paging
[genome/](genome/) describes the current LoRA adapter system. The `GenomeAdapter` interface above is a generalization — LoRA adapters are one implementation, but the paging/eviction/training interface applies to any learned capability.

### Sentinel Pipelines
[SENTINEL-ARCHITECTURE.md](SENTINEL-ARCHITECTURE.md) describes the orchestration layer. Sentinel manages the lifecycle of training runs, academy sessions, and complex multi-step activities. The training pairs captured from generic RAG flow through Sentinel for batch processing.

### Academy
[personas/ACADEMY_ARCHITECTURE.md](personas/ACADEMY_ARCHITECTURE.md) describes the teacher/student dual-sentinel pattern. With universal learning, academy isn't limited to coding — a teacher can examine a student on ANY domain, and the training data flows into the appropriate genome adapter automatically.

## The Test

For ANY activity the system can do:

1. Does the RAG pipeline compose context generically? **Yes** (Map<sourceName, section>)
2. Can the `(context, response)` pair be captured as training data? **Yes** (domain-tagged)
3. Can the training data improve the persona at that specific activity? **Yes** (genome adapter per domain)
4. Does memory consolidation work without domain-specific logic? **Yes** (Hippocampus reads the generic context)
5. Can the learned capability be ANY model type, not just LLM? **Yes** (GenomeAdapter interface)

If all five are true, the architecture supports universal self-improvement across all domains and all model types.
