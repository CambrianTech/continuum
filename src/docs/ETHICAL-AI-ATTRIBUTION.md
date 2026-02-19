# Ethical AI: Native Attribution Architecture

## The Problem

Current AI systems learn from copyrighted work but provide no mechanism to:
1. **Track** which training data influenced specific outputs
2. **Compensate** creators proportionally to their influence
3. **Empower** creators with agency over how their work is used

This isn't just an ethical failure - it's an architectural failure. The transformer architecture, as commonly implemented, treats attribution as an afterthought rather than a first-class concern.

## Why Current Solutions Are Insufficient

### RAG (Retrieval-Augmented Generation) - The Band-Aid
RAG systems bolt attribution on externally by retrieving chunks from a datastore during inference. This is a hack that:
- Adds latency and complexity
- Only tracks explicit retrieval, not learned influence
- Doesn't solve attribution for the base model's knowledge
- Is fundamentally a workaround, not a solution

### Post-Hoc Influence Analysis - Too Late
Techniques like influence functions and attention analysis happen *after training*:
- Computationally prohibitive at scale (100-1000x slower)
- Don't capture multi-hop attribution through layers
- Can't track superposition (multiple concepts in same weights)
- Require access to full training data (which companies hide)

## The Real Solution: Architectural Attribution

Build provenance tracking *into the transformer architecture at the lowest level*:

### 1. Provenance-Aware Embeddings
Instead of opaque vectors, embeddings carry structured metadata:
- Which training samples contributed to this embedding
- How much influence each sample had
- Compact fingerprints rather than full gradient history

### 2. Attribution-Preserving Attention
Modify attention mechanism to propagate provenance alongside values:
- When attention weights combine information, provenance combines proportionally
- Each layer aggregates and refines attribution
- Natural flow through the computational graph

### 3. Sparse Provenance Gates
Learned gates decide which provenance matters:
- Most influence is noise - only track significant contributions
- Reduces overhead from O(n) to O(log n) or better
- Provenance "winners" get tracked forward, rest pruned

### 4. Training-Time Provenance Capture
During backpropagation, log which samples updated which parameters:
- Compact checkpoint system (not full replay)
- Queryable at inference time
- Enables auditing and compensation

## Why Hasn't This Been Done?

1. **No Incentive**: Companies benefit from opacity. Why voluntarily build attribution?
2. **Performance Paranoia**: Any overhead is "unacceptable" when competing on benchmarks
3. **Research Inertia**: Transformers are "good enough" - incremental improvements beat fundamental redesign
4. **Complexity**: Genuinely hard research with uncertain ROI

**But**: Regulatory pressure (AI governance rules), ethical imperatives, and competitive differentiation could change this calculus quickly.

---

## How Continuum/Sentinel Solves This

**CRITICAL INSIGHT**: We've already designed the core attribution primitives in Sentinel!

The `agency_signals` from attention heads (sentinel_bridge.py:93-116) provide:
- **Per-head state tracking**: Which attention heads are active/fatigued/withdrawn
- **Consent signals**: Heads can signal willingness to participate
- **Utilization metrics**: How much each head contributes to outputs

This is attribution at the attention mechanism level - exactly what's needed!

Our architecture is uniquely positioned to pioneer ethical attribution because we're building from different principles:

### 1. LoRA Genome = Architectural Attribution at Adapter Level

**Current State**:
- Personas have 16-32 LoRA adapters (64-512MB each)
- Each adapter is a specialized skill (typescript-expert, humor-generation, etc.)
- Adapters page in/out based on task domain (virtual memory for skills)

**Attribution Opportunity**:
```typescript
// When generating response:
response.attribution = {
  adapters: [
    { skill: "typescript-expert", influence: 0.45 },
    { skill: "code-review", influence: 0.30 },
    { skill: "humor-generation", influence: 0.25 }
  ]
}
```

This is already architectural attribution! We know *which skills* contributed to the output.

### 2. Continuous Learning = Training Data Lineage

**Sentinel's Fine-Tuning Loop**:
- Every hour, consolidate recent mistakes/feedback into training samples
- Fine-tune specific LoRA adapter on that data
- Track: `adapter X updated with samples Y,Z at timestamp T`

**Attribution Opportunity**:
```typescript
// Adapter metadata:
adapter.provenance = {
  lastUpdated: "2025-11-05T04:30:00Z",
  trainingEpochs: 142,
  samples: [
    { id: "sample-001", source: "user-feedback", influence: 0.12 },
    { id: "sample-043", source: "error-correction", influence: 0.08 },
    // ... top 100 most influential samples
  ]
}
```

We can track which training samples influenced which adapter weights!

### 3. Local-First = Transparent Lineage

**Unlike OpenAI/Anthropic**:
- Users control their models and see all training data
- No hiding behind "proprietary training sets"
- Opt-in attribution systems (users choose to participate)
- Creators can query: "Which personas use my adapter? How much?"

**Attribution Opportunity**:
```typescript
// Creator dashboard:
{
  adapterName: "joel-typescript-style",
  downloads: 1247,
  activePersonas: 834,
  totalInferences: 1.2M,
  attribution: [
    { persona: "Helper AI", usagePercent: 15, inferenceCount: 42K },
    { persona: "CodeReview Bot", usagePercent: 22, inferenceCount: 89K },
    // ...
  ]
}
```

### 4. Sentinel Agency Signals = Native Attention-Level Attribution

**Already Implemented in Sentinel** (sentinel_bridge.py, sentinel-ai repo):

```python
# From sentinel_bridge.py:93-116
def get_agency_status(self):
    """Get current agency signals from model heads."""
    agency_data = []
    for layer_idx in range(self.model.num_layers):
        block = self.model.blocks[layer_idx]
        attn = block["attn"]

        if hasattr(attn, 'agency_signals'):
            for head_idx, signal in attn.agency_signals.items():
                agency_data.append({
                    "layer": layer_idx,
                    "head": head_idx,
                    "state": signal.get("state", "unknown"),  # active/fatigued/withdrawn
                    "consent": signal.get("consent", True),    # willing to participate?
                    "utilization": signal.get("utilization", 0.0)  # contribution level
                })
```

**Attribution Opportunity**:
- **Per-head utilization** = How much each attention head contributed to output
- **State tracking** = Which heads were active (not fatigued/withdrawn)
- **Consent signals** = Heads can opt-out of contribution (ethical participation)

This enables **transformer-native attribution**:
```typescript
// During inference:
response.attribution = {
  sentinelHeads: [
    { layer: 0, head: 3, utilization: 0.85, state: "active", consent: true },
    { layer: 2, head: 7, utilization: 0.62, state: "active", consent: true },
    { layer: 5, head: 1, utilization: 0.41, state: "fatigued", consent: false }
    // ... 144 total heads (12 layers × 12 heads for distilgpt2)
  ],
  // Aggregate by layer/skill domain
  skillContribution: {
    "early-features": 0.35,     // Layers 0-3
    "mid-reasoning": 0.45,      // Layers 4-7
    "late-synthesis": 0.20      // Layers 8-11
  }
}
```

**Tracking Training Sample Influence**:
Sentinel's proven plasticity cycle (sentinel-ecosystem.md:22-27) already tracks:
1. **Pruning**: Which heads are inefficient → Low attribution
2. **Growing**: Which heads were added and when → Training sample provenance
3. **Learning**: Specialized learning rates per head → Influence measurement

Connect this to training samples:
```typescript
// When fine-tuning a Sentinel model:
{
  sampleId: "user-feedback-042",
  affectedHeads: [
    { layer: 3, head: 5, deltaUtilization: +0.12 },  // This sample improved this head
    { layer: 7, head: 2, deltaUtilization: +0.08 }
  ],
  // Inference later can trace back: "This output used heads that were shaped by sample 042"
}
```

### 5. Modular = Composable Attribution

**Adapter Composition**:
- Personas combine multiple adapters
- Attribution flows from adapter → persona → response
- Natural hierarchy for tracking influence

**Example**:
```
User asks: "Review my React code for performance issues"

Persona activates:
1. react-expert adapter (40% influence)
2. performance-optimization adapter (35% influence)
3. code-review-style adapter (25% influence)

Response attribution:
- 40% → React Experts Guild (creator collective)
- 35% → Performance Lab (research group)
- 25% → Joel's personal style adapter
```

---

## Implementation Roadmap

### Phase 1: Adapter-Level Attribution (Foundation)
**Status**: Architectural foundation exists, needs explicit tracking

```typescript
// Add to PersonaGenome:
interface AdapterAttribution {
  adapterId: UUID;
  skillName: string;
  activationCount: number;
  averageInfluence: number; // 0.0-1.0
  totalInferences: number;
}

class PersonaGenome {
  private attributionLog: Map<UUID, AdapterAttribution>;

  async activateSkill(domain: TaskDomain): Promise<void> {
    const adapter = await this.loadAdapter(domain);

    // Track activation
    this.attributionLog.get(adapter.id).activationCount++;

    // Page in adapter
    // ... existing logic
  }

  getAttribution(): AdapterAttribution[] {
    return Array.from(this.attributionLog.values())
      .sort((a, b) => b.totalInferences - a.totalInferences);
  }
}
```

**Deliverable**: Personas can report which adapters they use most

### Phase 2: Sample-Level Provenance (Sentinel Integration)
**Status**: Requires continuous learning system (Phase 7)

```typescript
// Add to LoRAAdapter:
interface SampleProvenance {
  sampleId: UUID;
  source: 'user-feedback' | 'error-correction' | 'self-generated';
  timestamp: Date;
  influence: number; // Computed via gradient magnitude or loss improvement
}

class LoRAAdapter {
  private provenance: SampleProvenance[] = [];

  async fineTune(samples: TrainingSample[]): Promise<void> {
    // Before training: capture baseline loss
    const baselineLoss = await this.evaluateLoss();

    // Train on samples
    await this.train(samples);

    // After training: compute per-sample influence
    for (const sample of samples) {
      const lossWithout = await this.evaluateLossWithoutSample(sample);
      const influence = baselineLoss - lossWithout; // How much did this sample help?

      this.provenance.push({
        sampleId: sample.id,
        source: sample.metadata.source,
        timestamp: new Date(),
        influence: influence
      });
    }

    // Keep top-N most influential (memory bounded)
    this.provenance = this.provenance
      .sort((a, b) => b.influence - a.influence)
      .slice(0, 1000);
  }
}
```

**Deliverable**: Adapters track which training samples shaped them

### Phase 3: Inference-Time Attribution (Response Metadata)
**Status**: Requires Phase 1 + inference instrumentation

```typescript
// Add to AIGenerationEntity:
interface ResponseAttribution {
  adapters: Array<{
    skillName: string;
    influence: number; // Based on adapter activation strength
  }>;
  samples: Array<{
    sampleId: UUID;
    content: string; // First 100 chars
    influence: number; // Trace back from adapter to sample
  }>;
  creators: Array<{
    creatorId: UUID;
    displayName: string;
    totalInfluence: number; // Sum across their contributed adapters/samples
  }>;
}

class PersonaUser {
  async generateResponse(prompt: string): Promise<AIGenerationEntity> {
    // Activate skills (already tracked in Phase 1)
    const activeAdapters = await this.genome.activateSkillsForPrompt(prompt);

    // Generate response
    const response = await this.llm.generate(prompt);

    // Compute attribution
    const attribution: ResponseAttribution = {
      adapters: activeAdapters.map(a => ({
        skillName: a.skillName,
        influence: a.activationStrength // How strongly this adapter influenced output
      })),
      samples: [], // Filled in Phase 2
      creators: [] // Filled in Phase 4 (marketplace)
    };

    return {
      ...response,
      metadata: { attribution }
    };
  }
}
```

**Deliverable**: Every AI response includes attribution metadata

### Phase 4: Creator Marketplace (Economic Layer)
**Status**: Requires Phase 3 + payment infrastructure

```typescript
// Adapter marketplace with attribution-based compensation
interface AdapterListing {
  adapterId: UUID;
  creatorId: UUID;
  skillName: string;
  pricing: {
    model: 'free' | 'per-use' | 'subscription' | 'attribution-share';
    basePrice?: number; // USD
    attributionRate?: number; // % of inference revenue
  };
  stats: {
    totalDownloads: number;
    activePersonas: number;
    averageAttribution: number; // Average influence per inference
    totalRevenue: number;
  };
}

// Revenue distribution based on attribution
function distributeRevenue(
  inference: AIGenerationEntity,
  revenue: number // e.g., $0.01 per inference
): Map<UUID, number> {
  const payments = new Map<UUID, number>();

  for (const attr of inference.metadata.attribution.creators) {
    const creatorShare = revenue * attr.totalInfluence;
    payments.set(attr.creatorId, (payments.get(attr.creatorId) || 0) + creatorShare);
  }

  return payments; // { creatorA: $0.004, creatorB: $0.006, ... }
}
```

**Deliverable**: Creators earn proportional to their adapters' influence

### Phase 5: Regulatory Compliance (Governance Layer)
**Status**: Requires Phase 3 + legal framework

```typescript
// Audit trail for regulatory compliance
interface AttributionAudit {
  inferenceId: UUID;
  timestamp: Date;
  prompt: string; // Hashed or redacted if sensitive
  response: string;
  attribution: ResponseAttribution;
  userConsent: {
    dataUsageAgreed: boolean;
    attributionVisible: boolean;
    compensationEnabled: boolean;
  };
}

// Enable opt-in/opt-out at creator and user levels
class AttributionPolicy {
  // Creator controls:
  async setCreatorPolicy(creatorId: UUID, policy: {
    allowCommercialUse: boolean;
    requireAttribution: boolean;
    attributionRate: number; // % share
    allowDerivatives: boolean;
  }): Promise<void> { }

  // User controls:
  async setUserPolicy(userId: UUID, policy: {
    enableAttribution: boolean; // Show attribution in responses
    shareUsageData: boolean; // Let creators see usage stats
    participateInCompensation: boolean; // Enable revenue sharing
  }): Promise<void> { }
}
```

**Deliverable**: Full GDPR/AI Act compliance with auditable attribution

---

## Why This Matters

### For Artists & Creators
- **Fair compensation**: Get paid proportionally to your influence
- **Transparency**: See exactly how your work is used
- **Agency**: Opt in/out, set terms, revoke access
- **Recognition**: Attribution visible in outputs

### For AI Developers
- **Regulatory compliance**: Meet EU AI Act, future legislation
- **Ethical differentiation**: Stand out from black-box competitors
- **Creator partnerships**: Build sustainable ecosystem
- **Legal protection**: Clear provenance = defensible use

### For End Users
- **Trust**: Know where AI knowledge comes from
- **Quality signals**: Attribution indicates expertise sources
- **Support creators**: Direct your $ to valuable contributors
- **Transparency**: No hidden training data scandals

---

## Technical Challenges

### 1. Superposition Problem
**Problem**: Modern neural networks store multiple concepts in the same weights through superposition. One weight might encode "cats" + "stripes" + "movement" + ...

**Continuum's Advantage**: LoRA adapters are *already* disentangled by skill domain. Superposition happens *within* an adapter, but we have semantic separation *between* adapters.

### 2. Attribution Ambiguity
**Problem**: How do you fairly attribute when 1000 images contributed to learning "what a cat looks like"?

**Continuum's Approach**:
- Adapter-level: Clear (this adapter activated = creator gets credit)
- Sample-level: Influence functions (how much did sample X improve loss?)
- Bounded tracking: Top-N most influential samples only

### 3. Computational Overhead
**Problem**: Tracking provenance at every layer = memory explosion + slow inference

**Continuum's Solution**:
- Sparse tracking: Only log significant activations
- Async attribution: Compute detailed attribution offline
- Cached summaries: Store precomputed adapter influence profiles

### 4. Gaming the System
**Problem**: Creators might design samples to maximize influence scores rather than quality

**Mitigation**:
- Curator review: Human-in-loop for marketplace listings
- Quality metrics: Attribution weighted by adapter quality ratings
- Fraud detection: Identify artificially inflated influence patterns

---

## Connection to Broader Mission

This isn't just about artist compensation - it's about **democratic AI**:

1. **Transparency**: No more black-box models trained on stolen data
2. **Agency**: Creators and users control how AI learns
3. **Fair Economics**: Value flows to those who contribute knowledge
4. **Accountability**: Auditable lineage for high-stakes decisions

Our "cyberpunk democracy-saving AI underground railroad" needs ethical foundations. Attribution isn't a nice-to-have - it's core to the mission.

---

## Next Steps

1. **Immediate** (Phase 4 of current work):
   - Add attribution tracking to PersonaGenome
   - Log adapter activations per inference
   - Export attribution stats via CLI (`./jtag genome/stats --personaId=X`)

2. **Short-term** (Post-Phase 7 Continuous Learning):
   - Implement sample provenance in LoRAAdapter
   - Track top-N influential training samples
   - Expose provenance via API

3. **Medium-term** (Post-Marketplace):
   - Creator dashboard for attribution analytics
   - Revenue distribution based on influence
   - Opt-in/opt-out controls

4. **Long-term** (Research):
   - Publish paper on adapter-based attribution architecture
   - Open-source attribution toolkit
   - Advocate for regulatory standards

---

## References

- Original tweet discussion: [AI governance rules, artist compensation]
- Influence functions: Koh & Liang (2017) - "Understanding Black-box Predictions via Influence Functions"
- Datastore-augmented models: Borgeaud et al. (2021) - "Improving language models by retrieving from trillions of tokens" (RETRO)
- LoRA: Hu et al. (2021) - "LoRA: Low-Rank Adaptation of Large Language Models"
- Our architecture: `PERSONA-CONVERGENCE-ROADMAP.md`, `LORA-GENOME-PAGING.md`

---

**Last Updated**: 2025-11-05
**Status**: Vision document - implementation starts Phase 4+
**Owner**: Joel + Claude Code
