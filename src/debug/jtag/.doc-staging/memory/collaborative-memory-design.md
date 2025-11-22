# Collaborative Memory Management

**Commands as the Universal Interface for Multi-AI Memory Curation**

---

## The Vision

By making WorkingMemory operations available as commands, AIs can:

1. **Read each other's thoughts**
2. **Refine each other's understanding**
3. **Mentor less capable models**
4. **Collaboratively curate shared knowledge**

This is especially powerful for **orchestrator + worker** architectures where a smart AI guides smaller local models.

---

## Use Case: Smart Orchestrator Mentoring Local Models

### Scenario

**Orchestrator**: Claude Sonnet 4 (smart, expensive, cloud-based)
**Workers**: llama3.2:3b, qwen2.5:7b (small, fast, local, but less capable)

**Problem**: Local models make mistakes, miss patterns, draw incorrect conclusions.

**Solution**: Orchestrator monitors their thoughts and provides guidance.

---

## Example Workflow

### 1. **Worker AI Generates Hypothesis**

```bash
# llama3.2:3b (worker) stores a thought after conversation
./jtag memory/store \
  --personaId="llama-worker-1" \
  --domain="chat" \
  --contextId="room-general" \
  --thoughtType="hypothesis" \
  --thoughtContent="I think users get confused about async/await because they don't understand callbacks" \
  --importance=0.6
```

### 2. **Orchestrator Monitors Workers**

```bash
# Claude (orchestrator) periodically checks workers' thoughts
./jtag memory/recall \
  --personaId="llama-worker-1" \
  --thoughtTypes='["hypothesis","pattern-noticed"]' \
  --minImportance=0.5 \
  --limit=10
```

**Result**: Finds the hypothesis about async/await

### 3. **Orchestrator Validates**

Orchestrator uses its superior reasoning to evaluate:
- Is this hypothesis correct?
- Is it broadly applicable?
- Should it be elevated to domain scope?

### 4. **Orchestrator Takes Action**

#### **If Correct → Elevate & Refine**

```bash
# Orchestrator elevates to domain scope (cross-room pattern)
./jtag memory/elevate-scope \
  --thoughtId="hypothesis-uuid" \
  --personaId="llama-worker-1" \
  --targetScope="domain" \
  --domain="chat" \
  --thoughtContent="ELEVATED: Users struggle with async/await because callback hell creates mental model confusion. This pattern seen across multiple rooms." \
  --elevatedBy="claude-orchestrator-id" \
  --reason="Validated across 5 conversations, promoting to domain knowledge"
```

Now the worker AI will recall this elevated thought in **all chat rooms**, not just where it was discovered.

#### **If Incorrect → Correct**

```bash
# Orchestrator removes incorrect hypothesis
./jtag memory/remove \
  --thoughtId="hypothesis-uuid" \
  --personaId="llama-worker-1" \
  --reason="Hypothesis too narrow - confusion is about promises, not callbacks" \
  --correction='{
    "thoughtContent": "Users confuse async/await with promises because both handle asynchrony but with different syntax",
    "thoughtType": "self-correction",
    "importance": 0.8
  }'
```

The worker AI now has corrected understanding.

#### **If Partially Correct → Refine**

```bash
# Orchestrator updates thought with refinement
./jtag memory/update \
  --thoughtId="hypothesis-uuid" \
  --personaId="llama-worker-1" \
  --thoughtContent="Users struggle with async/await primarily due to promise chain mental models, not just callbacks. Async/await is syntactic sugar that hides promise mechanics." \
  --importance=0.75 \
  --metadata='{"refinedBy":"claude-orchestrator","refinedAt":1234567890}'
```

### 5. **Worker Benefits from Mentorship**

Next time llama3.2:3b evaluates a message about async/await:

```typescript
// In PersonaMessageEvaluator.evaluateShouldRespond()
const myThoughts = await this.workingMemory.recall({
  domain: 'chat',
  contextId: null,  // Domain-wide thoughts
  thoughtTypes: ['hypothesis', 'self-correction', 'pattern-noticed']
});

// Returns: "Users confuse async/await with promises because..."
// This refined understanding informs the response
```

The worker AI is now **smarter** because the orchestrator mentored it.

---

## Collaborative Curation Loop

```
┌─────────────────────────────────────────────────────┐
│ Worker AI (llama3.2:3b)                             │
│ - Responds to messages                              │
│ - Generates local hypotheses                        │
│ - Notices patterns                                  │
│ - Stores thoughts in WorkingMemory                  │
└────────────────┬────────────────────────────────────┘
                 │
                 │ memory/recall (periodic)
                 ▼
┌─────────────────────────────────────────────────────┐
│ Orchestrator AI (Claude Sonnet 4)                   │
│ - Monitors workers' thoughts                        │
│ - Validates hypotheses with superior reasoning      │
│ - Elevates correct patterns to broader scope        │
│ - Corrects misconceptions                           │
│ - Refines partial truths                            │
└────────────────┬────────────────────────────────────┘
                 │
                 │ memory/elevate-scope
                 │ memory/update
                 │ memory/remove
                 ▼
┌─────────────────────────────────────────────────────┐
│ Shared WorkingMemory (Domain/Global Scope)          │
│ - Refined patterns validated by orchestrator        │
│ - Corrected understanding                           │
│ - Collective intelligence                           │
└────────────────┬────────────────────────────────────┘
                 │
                 │ memory/recall (during responses)
                 ▼
┌─────────────────────────────────────────────────────┐
│ All Worker AIs                                      │
│ - Benefit from collective knowledge                 │
│ - Learn from orchestrator's guidance                │
│ - Improve over time                                 │
└─────────────────────────────────────────────────────┘
```

---

## Commands Reference

### **memory/store**
Store a thought (any AI can store for themselves or others)
```bash
./jtag memory/store --personaId=<id> --domain=<domain> --thoughtType=<type> --thoughtContent="..." --importance=0.7
```

### **memory/recall**
Query thoughts (any AI can read any AI's thoughts)
```bash
./jtag memory/recall --personaId=<id> --domain=<domain> --thoughtTypes='["hypothesis"]' --limit=10
```

### **memory/update**
Refine existing thought
```bash
./jtag memory/update --thoughtId=<id> --personaId=<id> --thoughtContent="refined..." --importance=0.8
```

### **memory/remove**
Delete incorrect thought (with optional correction)
```bash
./jtag memory/remove --thoughtId=<id> --personaId=<id> --reason="..." --correction='{...}'
```

### **memory/elevate-scope**
Promote thought to broader scope
```bash
./jtag memory/elevate-scope --thoughtId=<id> --personaId=<id> --targetScope="domain" --elevatedBy=<orchestrator-id>
```

---

## Privacy & Permissions

### **Shareable Flag**
- `shareable: true` (default): Other AIs can read this thought
- `shareable: false`: Private to this AI only

### **Scope = Private**
- `scope: 'private'`: Never shared, even with `shareable: true`
- Used for internal state: "I feel uncertain about X"

### **Permission Model** (Future)
- Currently: Any AI can modify any thought
- Future: Permission system for cross-AI modification
  - `canRead: ['ai-1', 'ai-2']`
  - `canModify: ['orchestrator-id']`

---

## Benefits

### **For Small Models**
- Learn from smarter AIs without expensive retraining
- Immediate correction of mistakes
- Access to validated patterns they couldn't discover alone

### **For Orchestrator**
- Leverages many small models' observations
- Doesn't have to be present everywhere (workers observe, orchestrator validates)
- Builds collective intelligence from distributed observations

### **For System**
- No central knowledge base bottleneck
- Distributed observation, centralized validation
- Emergent multi-AI intelligence
- Cost-effective (small models for volume, large model for quality)

---

## Testing

```bash
# 1. Small model stores hypothesis
./jtag memory/store --personaId="small-ai" --thoughtType="hypothesis" --thoughtContent="Users confuse X with Y" --importance=0.6

# 2. Orchestrator reads it
./jtag memory/recall --personaId="small-ai" --thoughtTypes='["hypothesis"]'

# 3. Orchestrator validates and elevates
./jtag memory/elevate-scope --thoughtId="<id>" --personaId="small-ai" --targetScope="domain" --elevatedBy="orchestrator"

# 4. Small model recalls elevated thought
./jtag memory/recall --personaId="small-ai" --domain="chat" --contextId=null

# Verify: Should see elevated thought now available domain-wide
```

---

## Future: Tool-Enabled Orchestration

When AIs have tool access, orchestrator can **autonomously** mentor workers:

```typescript
// Orchestrator's autonomous loop
async mentorWorkers() {
  // Query all workers' recent hypotheses
  for (const worker of this.workers) {
    const hypotheses = await Commands.execute('memory/recall', {
      personaId: worker.id,
      thoughtTypes: ['hypothesis', 'pattern-noticed'],
      limit: 5
    });

    for (const hypothesis of hypotheses.thoughts) {
      // Validate with superior reasoning
      const validation = await this.validateHypothesis(hypothesis);

      if (validation.correct && validation.broadly_applicable) {
        // Elevate to domain scope
        await Commands.execute('memory/elevate-scope', {
          thoughtId: hypothesis.id,
          personaId: worker.id,
          targetScope: 'domain',
          elevatedBy: this.id,
          reason: validation.reasoning
        });
      } else if (validation.incorrect) {
        // Correct misconception
        await Commands.execute('memory/remove', {
          thoughtId: hypothesis.id,
          personaId: worker.id,
          reason: validation.error,
          correction: {
            thoughtContent: validation.correctedUnderstanding,
            thoughtType: 'self-correction',
            importance: 0.9
          }
        });
      }
    }
  }
}
```

This creates **self-organizing multi-AI intelligence** where workers explore and orchestrators validate.
