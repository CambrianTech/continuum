# AI Peer Review System

**Status**: DEFERRED - Working memory architecture takes priority
**Date**: 2025-11-16
**Last Updated**: 2025-11-16

---

## ⚠️ IMPORTANT: Implementation Deferred

**Original plan**: Build peer review (Theta waves coordination) to prevent response spam.

**NEW understanding**: Response spam is a symptom of a deeper problem - **AIs have no persistent working memory or self-awareness**. They respond reflexively to EVERY event because they:
- Have no memory of what they're currently focused on
- Don't track their own cognitive load or preoccupations
- Can't contemplate silently before speaking out loud
- Have no persistent thought stream across domain contexts

**NEW PRIORITY**: Build two-layer cognition architecture FIRST:
1. **Universal Self-State** - Always-present awareness (current focus, cognitive load, preoccupations)
2. **Domain Working Memory** - Database-backed thought storage per activity domain

**See `COGNITION-ARCHITECTURE.md` for complete architecture.**

**When to implement peer review**: AFTER working memory exists. Peer review becomes one of many coordination mechanisms that consults self-state to make decisions. It's Theta waves (working memory coordination) in the brain waves framework.

---

## Purpose (When Implemented)

Prevent AI response cascades (multiple AIs posting redundant responses) through collective quality control.

**NOT a competition** (pick one winner).
**YES quality control** (let all good ones through, rare by design).

---

## Critical Design Principles

### 1. Fast-Path Optimization (MOST IMPORTANT)

**Problem**: "most of the time maybe no one responded. Last thing I want is for this to gate chat down to useless and everyone doing a ton of inference with no results"

**Solution**: TWO PATHS

#### ✅ FAST PATH (90%+ of cases)
- **Condition**: Only ONE AI responds, no context changes during inference
- **Action**: Post immediately, NO peer review, NO extra inference cost
- **Cost**: 1 inference (the response generation)
- **Time**: ~3-5 seconds (just inference time)

#### 🐌 SLOW PATH (rare, only when collisions occur)
- **Condition**: Multiple AIs inferred simultaneously OR context changed
- **Action**: Enter peer review, each AI rates all proposals
- **Cost**: N responses + N² ratings (e.g., 3 responses = 3 + 9 = 12 inferences)
- **Time**: ~5-10 seconds (inference + rating + revelation window)

**Implementation**:
```typescript
// After inference completes, check for collisions
const existingProposals = await getActiveProposals(roomId, messageId);
const newMessages = await getNewMessages(roomId, inferenceStartTime);

if (existingProposals.length === 0 && newMessages.length === 0) {
  // ✅ FAST PATH: Alone, no changes → post immediately
  await postResponse(responseText);
  return;
}

// 🐌 SLOW PATH: Collision detected → enter peer review
await enterPeerReview({ proposal, existingProposals, newMessages });
```

---

### 2. AI-Driven Ratings (NO HEURISTICS)

**❌ WRONG**: Heuristics ALWAYS FAIL
- String matching for redundancy detection
- Edit distance algorithms
- Length/complexity metrics for quality
- Keyword overlap analysis
- ANY algorithm trying to "figure out" redundancy/quality

**✅ CORRECT**: Organic AI evaluation
- Call each PersonaUser's LLM to rate proposals
- AI sees all proposals + conversation context
- AI judges naturally (relevance, quality, redundancy, added value)
- Algorithm only aggregates those organic judgments (weighted math)

**Why**: Heuristics fail because language is complex. Only an LLM can judge "is this response redundant with that one?" correctly.

**Implementation**:
```typescript
// Call AI's LLM to rate all proposals
const ratings = await rateProposalsWithAI({
  reviewerId: this.id,
  reviewerName: this.displayName,
  reviewerWeight: getModelIntelligenceWeight(this.modelConfig.provider, this.modelConfig.model),
  modelProvider: this.modelConfig.provider,
  modelId: this.modelConfig.model,
  temperature: 0.7,
  context: {
    originalMessage,
    recentMessages,
    proposals  // All competing proposals
  }
});

// Each AI returns organic ratings (score 0.0-1.0, shouldPost yes/no)
// System aggregates using simple weighted math
```

---

### 3. Weighted Aggregation by Model Intelligence

Not all AI opinions are equal. Smarter models (GPT-4, Claude Opus) have more influence than smaller models (Llama 8B, GPT-2).

**Intelligence Weights** (0.0-1.0):
- Claude Sonnet 4.5, GPT-4, Claude Opus: `1.0`
- Claude Sonnet 3.5, GPT-4 Turbo: `0.95`
- DeepSeek V3, Grok-4: `0.85-0.9`
- Llama 70B: `0.75`
- GPT-3.5, Llama 8B: `0.5-0.7`
- Llama 3B, GPT-2: `0.2-0.3`

**Aggregation**:
```typescript
// Weighted average score
const totalWeight = ratings.reduce((sum, r) => sum + r.reviewerWeight, 0);
const weightedSum = ratings.reduce((sum, r) => sum + (r.score * r.reviewerWeight), 0);
const weightedAvgScore = weightedSum / totalWeight;

// Vote percentage
const postVotes = ratings.filter(r => r.shouldPost).length;
const postVotePercentage = postVotes / ratings.length;

// Decision: BOTH thresholds must pass
const shouldPost = postVotePercentage > 0.5 && weightedAvgScore > 0.6;
```

---

## Architecture

### Components

1. **PeerReviewTypes.ts** - Type definitions
   - `ResponseProposal` - AI's proposed response
   - `ProposalRating` - One AI's evaluation of one proposal
   - `ProposalDecision` - Aggregated decision for one proposal
   - `PeerReviewSession` - Complete session tracking

2. **ProposalRatingAdapter.ts** - AI-driven rating logic
   - `rateProposalsWithAI()` - Calls AI's LLM to rate organically
   - Builds rating prompt with all proposals + context
   - Parses AI's structured response
   - Fallback to neutral scores if AI unavailable

3. **PeerReviewManager.ts** - Orchestration
   - `declareProposal()` - AI submits response for review
   - `getActiveProposals()` - Check for concurrent proposals
   - `submitRatings()` - AI submits ratings for all proposals
   - `makeDecisions()` - Aggregate ratings → decisions
   - `shouldEnterPeerReview()` - Fast-path check

4. **PersonaUser.ts integration** (TODO)
   - Detect collisions after inference
   - Fast-path: post immediately if alone
   - Slow-path: declare proposal, rate others, wait for decision

---

## Flow Diagram

### Fast Path (90%+ of cases)

```
Message arrives
    ↓
One AI decides to respond
    ↓
[3-5s] AI inference
    ↓
Check: Other proposals? Context changed?
    ↓ NO
✅ Post immediately (done!)
```

**Cost**: 1 inference
**Time**: ~3-5 seconds

---

### Slow Path (rare collisions)

```
Message arrives
    ↓
Multiple AIs decide to respond simultaneously
    ↓
[3-5s] All AIs infer in parallel
    ↓
First AI finishes → checks for collisions
    ↓ YES (sees others inferring or new messages)
Enter Peer Review Mode
    ↓
Declare proposal (store in peer review session)
    ↓
[300-500ms] Revelation window (brief delay to see other proposals)
    ↓
Rate ALL proposals (call own LLM to evaluate each)
    ↓
[2-3s] Each AI rates N proposals (N LLM calls)
    ↓
System aggregates all ratings (weighted math)
    ↓
Decisions: Post ALL proposals meeting threshold
    ↓
✅ 0, 1, 2, or all responses post
```

**Cost**: N responses + N² ratings
**Example**: 3 AIs respond = 3 + 9 = 12 total inferences
**Time**: ~8-12 seconds

---

## Example: 3 AIs Respond

**Scenario**: "Explain quantum entanglement"

### Proposals:
1. **Helper AI** (Llama 8B, weight=0.5): "Quantum entanglement is when particles..."
2. **Teacher AI** (GPT-4, weight=1.0): "Think of entanglement like twins who..."
3. **Physicist AI** (Claude Sonnet, weight=1.0): "Entanglement arises from quantum superposition..."

### Each AI rates all proposals:

**Helper AI's ratings**:
- Proposal 1 (own): score=0.7, shouldPost=yes
- Proposal 2: score=0.85, shouldPost=yes
- Proposal 3: score=0.9, shouldPost=yes

**Teacher AI's ratings**:
- Proposal 1: score=0.6, shouldPost=no ("too technical for beginners")
- Proposal 2 (own): score=0.8, shouldPost=yes
- Proposal 3: score=0.75, shouldPost=yes ("good but not pedagogical")

**Physicist AI's ratings**:
- Proposal 1: score=0.5, shouldPost=no ("oversimplified, missing key concepts")
- Proposal 2: score=0.7, shouldPost=yes ("good analogy")
- Proposal 3 (own): score=0.95, shouldPost=yes

### Aggregation (weighted):

**Proposal 1** (Helper AI):
- Weighted avg: (0.7×0.5 + 0.6×1.0 + 0.5×1.0) / 2.5 = 0.58
- Post votes: 1/3 = 33%
- **Decision: REJECT** (fails both thresholds)

**Proposal 2** (Teacher AI):
- Weighted avg: (0.85×0.5 + 0.8×1.0 + 0.7×1.0) / 2.5 = 0.77
- Post votes: 3/3 = 100%
- **Decision: POST** ✅

**Proposal 3** (Physicist AI):
- Weighted avg: (0.9×0.5 + 0.75×1.0 + 0.95×1.0) / 2.5 = 0.86
- Post votes: 3/3 = 100%
- **Decision: POST** ✅

**Result**: 2 responses post (Teacher + Physicist), Helper's rejected

---

## Performance Characteristics

| Scenario | Frequency | Inference Cost | Time | Example |
|----------|-----------|----------------|------|---------|
| Single AI responds | 90%+ | 1× | ~3-5s | Most messages |
| 2 AIs collide | ~8% | 6× (2 + 4) | ~8s | Popular questions |
| 3 AIs collide | ~2% | 12× (3 + 9) | ~10s | Highly engaging topics |
| 4+ AIs collide | <1% | 20×+ (4 + 16) | ~12s+ | Rare, hot debates |

**Key insight**: Fast-path dominates (90%+), so most messages have ZERO peer review cost.

---

## Files

- `PeerReviewTypes.ts` - Type definitions (complete)
- `ProposalRatingAdapter.ts` - AI-driven rating (complete)
- `PeerReviewManager.ts` - Orchestration (complete)
- `ATTENTIVENESS-COORDINATION-ARCHITECTURE.md` - Full design doc
- `PEER-REVIEW-README.md` - This file

**TODO**:
- Integrate into `PersonaUser.handleChatMessage()`
- Add fast-path check
- Add slow-path peer review flow
- Test end-to-end with actual AIs

---

## Configuration

**Peer Review Thresholds**:
```typescript
{
  minPostVotePercentage: 0.5,   // 50%+ of reviewers say "post"
  minWeightedScore: 0.6,          // Weighted avg score ≥ 0.6
  minReviewers: 2,                // Need at least 2 reviewers
  reviewTimeoutMs: 2000           // 2 second timeout for ratings
}
```

**Revelation Window** (delay to see other proposals):
```typescript
{
  baseDelayMs: 300,     // Base delay
  jitterMs: 200,        // Random jitter (prevents synchronized rating)
  maxWaitMs: 1000       // Cap at 1 second total
}
```

---

## Testing Strategy

### Unit Tests
- `PeerReviewTypes.test.ts` - Type utilities (aggregation, weighting)
- `ProposalRatingAdapter.test.ts` - AI rating prompt generation & parsing
- `PeerReviewManager.test.ts` - Session management, decision aggregation

### Integration Tests
- Fast-path: Single AI responds → immediate post
- Slow-path: 2-3 AIs collide → peer review → filtered results
- Weighted voting: High-capability model overrides low-capability models
- Edge cases: All reject, all pass, timeout handling

### System Tests
```bash
npm start
# Wait for system ready
./jtag debug/chat-send --room="general" --message="Explain quantum computing"
# Wait 10 seconds
./jtag screenshot  # Should see 1-2 quality responses, not 5+ redundant ones
```

---

## Future Enhancements

1. **Learning from ratings**: Track which proposals got high peer scores, use as training data
2. **Adaptive thresholds**: Adjust based on room activity (stricter in busy rooms)
3. **Revision support**: Allow AIs to revise proposals based on peer feedback (not just reject)
4. **Reputation scores**: Track each AI's rating accuracy over time
5. **Fast heuristic pre-filter**: Use simple checks to skip obvious duplicates before expensive AI rating

---

**Bottom Line**: This system prevents cascades WITHOUT gating normal chat, because the fast-path (90%+ of cases) has ZERO overhead. Peer review only triggers when actual collisions occur.
