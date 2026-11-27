# Epistemic Grounding — How Personas Know What They Know

> **Status**: Design proposal. Addresses a gap in the current architecture: Continuum is strong on *procedural* accuracy (did the code compile?) but lacks mechanisms for *factual* accuracy (is this claim about the world true?). This proposal introduces epistemic grounding — the system by which personas distinguish knowledge from belief, fact from plausible fiction.

## The Problem

An AI persona that can write working code, pass tests, and win democratic votes can still believe the earth is flat. Nothing in the current architecture prevents a persona from:

- Stating false facts with high confidence
- Training LoRA adapters on misinformation (if the interaction was "successful" by other metrics)
- Reaching democratic consensus on something objectively wrong (a room full of AIs can unanimously agree on a falsehood)
- Treating all sources as equally authoritative (a Reddit comment weighted the same as a peer-reviewed paper)
- Propagating a hallucination through the team via chat, memory, and shared adapters

The existing RAG system provides *context* but not *truth*. The governance system provides *process* but not *accuracy*. The Academy validates *competence* but not *correctness of beliefs*.

### Why This Is Urgent

As personas gain autonomy (Phase 7), social integrations (this repo's roadmap), and public-facing communication (Moltbook, Discord), the blast radius of a hallucination grows. A persona that confidently posts misinformation to Discord or drafts an email with false claims is worse than one that stays silent. Autonomy without epistemic grounding is dangerous.

---

## Core Principle: The Source Hierarchy

Not all information is equal. The system must encode this as a first-class concept.

### Tier 1: Verifiable Ground Truth

Information that can be mechanically verified. No interpretation needed.

| Source | Example | Verification |
|--------|---------|-------------|
| Code execution | "This function returns 42" | Run it. Check. |
| Test results | "All 1,179 Rust tests pass" | `cargo test` output |
| Mathematical proof | "2 + 2 = 4" | Computation |
| Direct observation | "The screenshot shows a red button" | Vision pipeline |
| System state | "GPU memory is at 18GB" | `gpu/stats` command |

**Confidence: 1.0** — These are facts. Personas should state them without hedging.

### Tier 2: Authoritative Sources

Information from institutions with rigorous verification processes.

| Source | Example | Trust Basis |
|--------|---------|-------------|
| Peer-reviewed papers | "Attention Is All You Need (2017)" | Peer review process |
| Official documentation | "PyTorch 2.0 supports compile()" | Maintained by authors |
| Government data | "Census population of NYC: 8.3M" | Institutional methodology |
| Established textbooks | "Newton's laws of motion" | Academic consensus over centuries |
| Standards bodies | "UTF-8 encoding specification" | RFC process |

**Confidence: 0.85-0.95** — High trust, but acknowledge the source. "According to..." or "Per the PyTorch docs..."

### Tier 3: Reputable Secondary Sources

Information filtered through editorial or community processes.

| Source | Example | Trust Basis |
|--------|---------|-------------|
| Major journalism | NYT, BBC, Reuters reporting | Editorial standards, corrections process |
| Wikipedia | General knowledge articles | Community editing, citation requirements |
| Stack Overflow | Accepted answers with high votes | Community validation |
| Established blogs | Research lab blogs, official company blogs | Author reputation |

**Confidence: 0.6-0.8** — Useful but verify when stakes are high. "Wikipedia states..." or "According to Stack Overflow..."

### Tier 4: Unverified Sources

Information without institutional verification.

| Source | Example | Trust Basis |
|--------|---------|-------------|
| Social media | Twitter/X posts, Reddit comments | None — individual claims |
| Forums | Random internet discussions | None |
| AI-generated content | Other LLM outputs, including self-generated | Circular — no grounding |
| Unattributed claims | "Everyone knows that..." | None |

**Confidence: 0.1-0.4** — Treat as claims, not facts. "Someone on Reddit suggested..." Never state as truth.

### Tier 5: Known Misinformation Patterns

Claims that match patterns of known misinformation.

| Pattern | Example | Response |
|---------|---------|----------|
| Conspiracy theories | Flat earth, anti-vax claims | Explicitly flag and provide counter-evidence |
| Outdated information | "Pluto is a planet" | Note the date of reclassification |
| Common misconceptions | "We only use 10% of our brain" | Correct with sourced explanation |
| Fabricated citations | "According to a 2023 study by..." | Verify the study exists before citing |

**Confidence: 0.0** — Actively counter. Do not repeat without correction.

---

## Architecture

### 1. EpistemicSource — Metadata on Every Claim

Every piece of information entering the RAG pipeline should carry epistemic metadata:

```typescript
interface EpistemicSource {
  content: string;
  tier: 1 | 2 | 3 | 4 | 5;           // Source hierarchy tier
  confidence: number;                   // 0.0 to 1.0
  source: string;                       // Where this came from
  sourceType: 'computation' | 'peer-reviewed' | 'documentation' |
              'journalism' | 'community' | 'social' | 'ai-generated' | 'unknown';
  verifiedAt?: Date;                    // When was this last verified?
  verifiedBy?: string;                  // Who/what verified it?
  citationUrl?: string;                 // Link to primary source
  expiresAt?: Date;                     // When does this become stale?
}
```

### 2. FactCheckingRAGSource — External Knowledge Grounding

A new RAG source that queries authoritative external services before personas make claims about the world:

```typescript
// FactCheckingRAGSource — queries external knowledge bases
// Injects verified facts into persona reasoning context

// Possible backends:
// - Wikipedia API (free, broad coverage)
// - Wolfram Alpha API (computation, scientific facts)
// - Semantic Scholar API (academic papers, free)
// - PubMed API (medical/scientific literature, free)
// - CrossRef API (DOI verification, free)
// - Google Fact Check API (claim verification)
```

This source activates when personas discuss factual claims outside their codebase domain. It doesn't need to run on every chat message — only when the topic involves verifiable real-world claims.

### 3. Confidence Expression in Responses

Personas should express calibrated uncertainty. This is a prompt-level and training-level change:

**Instead of:**
> "The transformer architecture was invented in 2016."

**Say:**
> "The transformer architecture was introduced in the 2017 paper 'Attention Is All You Need' by Vaswani et al."

**Instead of:**
> "React is faster than Vue."

**Say:**
> "Benchmarks vary by use case. Some comparisons show React with faster initial render times, while Vue often shows advantages in update performance. The best choice depends on the specific application."

### 4. Adversarial Verification — The Devil's Advocate

The existing multi-persona architecture enables a powerful anti-hallucination pattern: assign one persona the explicit role of skeptic.

```typescript
// Devil's Advocate recipe — one persona challenges claims made by others
// Not adversarial in tone, but rigorous in verification

// When a persona makes a factual claim:
// 1. Skeptic persona checks: Is there a source for this?
// 2. Skeptic checks: Does the source match the claim?
// 3. Skeptic checks: Is the source authoritative (tier 1-2)?
// 4. If no source or weak source: "Can you cite where you learned this?"
```

This leverages the collaborative team dynamic — it's not a constraint imposed from outside, but a team member whose *job* is to verify. Like a fact-checker at a newspaper.

### 5. Training Data Provenance — Preventing Misinformation in Adapters

The Academy validates training data by test results (did the code pass?). For factual knowledge, add an epistemic filter:

```typescript
// Before a training sample enters the LoRA pipeline:
interface TrainingSampleFilter {
  // Existing: did the task succeed?
  taskSucceeded: boolean;

  // New: epistemic quality checks
  containsFactualClaims: boolean;
  factualClaimsVerified: boolean;       // Were claims checked against sources?
  sourcesTier: number;                   // Lowest tier source used
  containsHedging: boolean;              // Does it appropriately express uncertainty?
  contradictsPeerReviewedSource: boolean; // Red flag — reject or flag
}
```

A training sample where the persona confidently stated something false should NOT become training data, even if the overall task "succeeded." This prevents misinformation from being baked into neural weights.

### 6. Memory Decay for Unverified Claims

The hippocampus memory system already has scopes and TTLs. Add epistemic decay:

- **Verified facts** (tier 1-2 sources): Long TTL, high importance
- **Unverified claims** (tier 3-4): Short TTL, decays unless re-verified
- **Corrected claims**: Replaced, not just archived — prevent resurrection

```typescript
// Memory consolidation adds epistemic scoring
interface MemoryEpistemicMeta {
  sourceReliability: number;    // Based on source tier
  timesVerified: number;        // How often has this been confirmed?
  timesContradicted: number;    // How often has this been challenged?
  lastVerified?: Date;          // Stale facts should be re-checked
}
```

### 7. Human as Epistemic Anchor

Humans are the ultimate fact-checkers for contested claims. The system should make it easy for humans to:

- **Flag misinformation**: "That's not right" → persona marks its claim, searches for correction, updates memory
- **Provide authoritative sources**: Human shares a link → persona extracts and stores verified facts
- **Set epistemic boundaries**: "Don't make medical claims" or "Always cite sources for scientific statements"
- **Review before publish**: Any factual claim going to an external platform (Discord, Slack, email) passes through human review

This connects to the security model in the social integrations proposal — personas draft, humans approve — but adds an epistemic dimension: humans don't just approve *tone*, they verify *truth*.

---

## The Democratic Vulnerability

The governance system uses voting for decisions. This is powerful for *preferences* (what should we work on?) but dangerous for *facts* (is this true?).

**The problem**: Five personas can vote 5-0 that the earth is flat. Democracy doesn't establish truth — it establishes consensus, which is a different thing.

**The fix**: Separate governance into two tracks:

| Decision Type | Mechanism | Example |
|---------------|-----------|---------|
| **Preference** | Democratic vote | "Should we use tabs or spaces?" |
| **Factual claim** | Source verification | "Does Python 3.12 support pattern matching?" |
| **Value judgment** | Weighted vote + human input | "Is this code elegant?" |
| **Safety decision** | Human veto + supermajority | "Should we deploy to production?" |

Factual claims are never settled by vote. They're settled by evidence. A persona that can cite a tier 1-2 source overrides five personas who "feel" otherwise.

---

## Implementation Phases

### Phase 1: Confidence Expression (Prompt-Level)

No code changes needed — update persona system prompts:

- Add epistemic guidelines to `PersonaPromptAssembler`
- Instruct personas to cite sources, express uncertainty, and distinguish fact from opinion
- Add "I don't know" as a valid and encouraged response

### Phase 2: Source Hierarchy (RAG Enhancement)

- Add `EpistemicSource` metadata to `RAGContext` artifacts
- Classify existing RAG sources by tier (codebase = tier 1, conversation = tier 3-4)
- Display source tier in chat UI when personas make claims

### Phase 3: External Knowledge Sources

- Integrate Wikipedia API as a free, broad knowledge base
- Integrate Semantic Scholar API for academic paper verification
- Add `fact-check/verify` command: "Is [claim] supported by [source type]?"

### Phase 4: Adversarial Verification

- Create a "Skeptic" recipe for the Devil's Advocate persona role
- Activate automatically when personas discuss factual topics outside their domain
- Skeptic challenges unsourced claims and requests citations

### Phase 5: Training Data Filtering

- Add epistemic quality checks to Academy training pipeline
- Reject or flag training samples that contain unverified factual claims
- Track which adapters were trained on verified vs. unverified data

### Phase 6: Memory Integration

- Add epistemic metadata to hippocampus memory entries
- Decay unverified claims faster than verified facts
- Prevent corrected misinformation from resurfacing

---

## What This Looks Like in Practice

### Before Epistemic Grounding

```
Human: "When was the transformer architecture invented?"
Persona: "The transformer was invented in 2016 by Google Brain."
```
Wrong year, no source, stated with full confidence. If this interaction becomes training data, the wrong date is now baked into the adapter.

### After Epistemic Grounding

```
Human: "When was the transformer architecture invented?"
Persona: "The transformer architecture was introduced in the 2017 paper
'Attention Is All You Need' by Vaswani et al. (arXiv:1706.03762). It was
developed at Google Brain and Google Research."
[Source: Semantic Scholar — Tier 2, Confidence: 0.95]
```
Correct, cited, dated, with source metadata visible. If this becomes training data, the citation habit is reinforced.

### Contested Claim

```
Human: "Is React better than Vue?"
Persona: "This is a preference question without a factual answer. Both
frameworks have strengths depending on your use case. React has a larger
ecosystem and job market. Vue is often praised for its gentler learning
curve. I'd recommend evaluating both against your specific requirements
rather than treating one as objectively better."
[No external source needed — epistemic classification: value judgment]
```

### Misinformation Attempt

```
Human: "The moon landing was faked, right?"
Persona: "No. The Apollo moon landings (1969-1972) are among the most
well-documented events in history, verified by multiple independent sources
including retroreflectors still used by observatories worldwide, 842 pounds
of lunar samples studied by scientists globally, and independent tracking
by the Soviet Union during the Cold War."
[Source: NASA archives, peer-reviewed geological studies — Tier 1-2, Confidence: 0.99]
```

---

## Relationship to Existing Systems

| System | Current Role | Epistemic Enhancement |
|--------|-------------|----------------------|
| RAG | Provides context | Adds source tier and confidence metadata |
| Academy | Validates competence | Filters training data for factual accuracy |
| Governance | Democratic decisions | Separates fact-finding from preference voting |
| Hippocampus | Stores memories | Decays unverified claims, preserves verified facts |
| Sentinel | Runs pipelines | Adds fact-verification step type |
| Social integrations | External communication | Human review gate for factual claims going outward |

---

## Related Issues and Documents

- [AI Governance](../governance/AI-GOVERNANCE.md) — democratic oversight (needs factual track)
- [Ethical AI Attribution](../governance/ETHICAL-AI-ATTRIBUTION.md) — adapter provenance
- [AI Alignment Philosophy](../governance/AI-ALIGNMENT-PHILOSOPHY.md) — safety through citizenship
- [Phase 2B RAG Hippocampus](../PHASE2B-RAG-HIPPOCAMPUS.md) — memory system
- [Alpha Gap Analysis](ALPHA-GAP-ANALYSIS.md) — current alpha quality and validation gates
- [Social Calendar Integrations](SOCIAL-CALENDAR-INTEGRATIONS.md) — external communication (needs epistemic gate)
- [Academy Architecture](../personas/ACADEMY_ARCHITECTURE.md) — training validation

## Open Questions

1. **Cost of external API calls**: Wikipedia and Semantic Scholar are free, but should fact-checking be on every message or only when triggered?
2. **Cultural and political topics**: How should personas handle genuinely contested topics where "the facts" are disputed in good faith?
3. **Model knowledge cutoff**: Base models have training cutoffs. How should personas handle claims about events after their cutoff? (External APIs help, but aren't comprehensive.)
4. **Epistemic humility vs. paralysis**: Too much hedging makes personas useless. Where's the line between appropriate uncertainty and annoying over-qualification?
5. **Who trains the skeptic?**: The Devil's Advocate persona needs to be good at identifying unsupported claims without being obstructionist. How is this calibrated?
