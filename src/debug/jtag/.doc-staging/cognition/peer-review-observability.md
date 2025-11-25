# Peer Review Observability & Debugging

**Status**: DEFERRED - Peer review implementation postponed
**See**: `COGNITION-ARCHITECTURE.md` for current direction

**How to inspect what's happening in the peer review system (when implemented)**

---

## Note on Deferral

This observability design remains valid for when peer review is implemented. The `CoordinationDecisionLogger` infrastructure is already in place and will work perfectly for logging peer review decisions once the feature is built.

**Current priority**: Build working memory architecture first, then implement peer review as a coordination mechanism that uses self-state.

---

## Existing Commands (Already Implemented)

### 1. `./jtag ai/report/decisions` - Complete decision log

Shows ALL AI decisions (respond/silent) with full context:
- What RAG context the AI saw
- What coordination state existed
- Whether they responded or stayed silent
- Confidence scores and reasoning

**Peer review will log to this automatically via `CoordinationDecisionLogger`**.

```bash
./jtag ai/report/decisions

# Output: Markdown report at .continuum/reports/decisions-[timestamp].md
# Shows:
# - All decisions in time range
# - Actor breakdown (which AIs responded)
# - Stats (posted vs silent, avg confidence)
```

### 2. `./jtag ai/rag/inspect` - RAG context inspection

Shows what context an AI saw when making a decision.

### 3. `./jtag ai/should-respond` - Test decision points

Manually trigger should-respond logic to test gating.

### 4. `./jtag ai/thoughtstream` - Coordination state

Shows thermal dynamics and coordination state.

---

## What Peer Review Logs

### Fast-Path (90%+ of cases)

**Decision Point**: After inference, check for collisions

**Logged via `CoordinationDecisionLogger.logDecision()`**:
```typescript
{
  actorId: personaId,
  actorName: displayName,
  action: 'RESPOND',  // Always respond on fast-path
  confidence: inferenceConfidence,
  reasoning: 'Fast-path: no collisions detected, posted immediately',
  responseContent: aiResponse.text,
  responseTime: inferenceDuration,
  tags: ['fast-path', 'no-peer-review']
}
```

**What you can see**:
```bash
./jtag ai/report/decisions | grep "fast-path"
# Shows which responses skipped peer review
```

---

### Slow-Path: Peer Review Triggered

**Decision Point 1**: Collision detected, entering peer review

**Logged**:
```typescript
{
  actorId: personaId,
  action: 'DEFER',  // New action type for peer review
  reasoning: `Collision detected: ${existingProposals.length} other proposals, entering peer review`,
  tags: ['slow-path', 'peer-review-entered', 'collision']
}
```

**Decision Point 2**: AI rates all proposals

**Logged** (one per proposal rated):
```typescript
{
  actorId: reviewerId,
  action: 'RATE_PROPOSAL',
  reasoning: `Rated proposal by ${proposerName}: score=${score}, shouldPost=${shouldPost}`,
  responseContent: ratingPrompt,  // The prompt sent to AI
  responseTime: ratingInferenceDuration,
  tags: ['peer-review', 'rating', `proposal:${proposalId}`]
}
```

**Decision Point 3**: Aggregation decides which proposals post

**Logged** (one per proposal):
```typescript
{
  actorId: 'SYSTEM',  // System-level aggregation
  action: 'PEER_REVIEW_DECISION',
  reasoning: `Proposal by ${proposerName}: ${decision.shouldPost ? 'APPROVED' : 'REJECTED'} (weighted score: ${decision.weightedAvgScore}, votes: ${decision.postVotes}/${decision.totalVotes})`,
  tags: ['peer-review', 'aggregation', `proposal:${proposalId}`]
}
```

**Decision Point 4**: AI's proposal approved/rejected

**Logged**:
```typescript
{
  actorId: personaId,
  action: decision.shouldPost ? 'RESPOND' : 'SILENT',
  confidence: decision.weightedAvgScore,
  reasoning: decision.reasoning,
  responseContent: decision.shouldPost ? proposalText : undefined,
  tags: ['peer-review', decision.shouldPost ? 'approved' : 'rejected']
}
```

---

## Inspecting Peer Review Sessions

### Command: `./jtag ai/report/decisions --filter="peer-review"`

Shows only peer review decisions:
```bash
./jtag ai/report/decisions --filter="peer-review"

# Output shows:
# 1. Collision detection (DEFER actions)
# 2. All ratings submitted (RATE_PROPOSAL actions)
# 3. Aggregation decisions (PEER_REVIEW_DECISION actions)
# 4. Final outcomes (RESPOND/SILENT with peer-review tag)
```

### Command: `./jtag ai/report/decisions --actorId="helper-ai" --limit=20`

Shows recent decisions for one AI:
```bash
./jtag ai/report/decisions --actorId="$(./jtag user/list | jq -r '.users[] | select(.displayName=="Helper AI") | .id')"

# Shows:
# - When Helper AI responded (fast-path vs peer-reviewed)
# - What proposals Helper AI rated
# - Whether Helper AI's proposals were approved/rejected
```

---

## Debug Workflow

### Scenario: "Why didn't my AI respond?"

```bash
# 1. Check recent decisions for that AI
AI_ID="$(./jtag user/list | jq -r '.users[] | select(.displayName=="Helper AI") | .id')"
./jtag ai/report/decisions --actorId="$AI_ID" --limit=10

# Look for:
# - SILENT decision (why? check reasoning field)
# - DEFER decision (entered peer review)
# - If peer review: check RATE_PROPOSAL actions (did they rate others?)
# - If rated: check PEER_REVIEW_DECISION (was their proposal rejected?)
```

### Scenario: "Why did 3 AIs all respond to the same message?"

```bash
# 1. Get the message ID
MESSAGE_ID="xyz"

# 2. Find all decisions for that message
./jtag ai/report/decisions | grep "$MESSAGE_ID"

# Look for:
# - Did any enter peer review? (DEFER actions)
# - If not, they all hit fast-path (no collision detected)
# - This means inference timing was spread out (no overlap)
```

### Scenario: "What prompt was sent to the AI for rating?"

```bash
# 1. Find the RATE_PROPOSAL decision
./jtag ai/report/decisions --filter="RATE_PROPOSAL" --limit=1

# 2. Check responseContent field - contains the rating prompt
# 3. Check responseTime - shows how long rating took
```

### Scenario: "Why was this proposal rejected by peers?"

```bash
# 1. Find the PEER_REVIEW_DECISION for that proposal
PROPOSAL_ID="abc"
./jtag ai/report/decisions | grep "proposal:$PROPOSAL_ID" | grep "PEER_REVIEW_DECISION"

# Check reasoning field:
# - "Failed vote threshold (33% < 50%)" - most reviewers said don't post
# - "Failed score threshold (0.55 < 0.6)" - weighted score too low
# - Shows exactly why it was rejected
```

---

## Performance Monitoring

### Command: `./jtag ai/report` - Performance stats

Already exists, shows:
- Response times
- Token usage
- Cost per model

**Peer review impact**:
```bash
./jtag ai/report

# Compare:
# - Fast-path responses: ~3-5s (just inference)
# - Peer-reviewed responses: ~8-12s (inference + rating + aggregation)
# - Token usage: N responses + N² ratings
```

---

## What Gets Stored

### 1. Coordination Decisions (database)

**Collection**: `coordination_decisions`

**Queryable via**:
```bash
./jtag data/list --collection=coordination_decisions \
  --filter='{"tags":{"$in":["peer-review"]}}' \
  --limit=20
```

**Fields**:
- `actorId`, `actorName`
- `action` (RESPOND/SILENT/DEFER/RATE_PROPOSAL/PEER_REVIEW_DECISION)
- `ragContext` (what the AI saw)
- `coordinationSnapshot` (thermal state, other AIs active)
- `reasoning` (why this decision?)
- `responseContent` (the response or rating prompt)
- `tags` (for filtering: fast-path, peer-review, collision, etc.)

### 2. Peer Review Sessions (in-memory, ephemeral)

**Not persisted**, but logged via decisions above.

**To reconstruct a session**:
```bash
# Find all decisions for a message
MESSAGE_ID="xyz"
./jtag data/list --collection=coordination_decisions \
  --filter="{\"triggerEventId\":\"$MESSAGE_ID\"}" \
  --orderBy='[{"field":"timestamp","direction":"asc"}]'

# Shows complete timeline:
# 1. All AIs that decided to respond
# 2. Collision detection (DEFER)
# 3. All ratings (RATE_PROPOSAL)
# 4. Aggregation (PEER_REVIEW_DECISION)
# 5. Final outcomes (RESPOND/SILENT)
```

---

## Testing & Verification

### Test 1: Verify Fast-Path Works

```bash
# Send message when only 1 AI likely to respond
./jtag debug/chat-send --room="general" --message="What's 2+2?"

# Wait 5 seconds
sleep 5

# Check decisions
./jtag ai/report/decisions --limit=5 | grep "fast-path"

# Should see: One RESPOND with tag "fast-path"
# Should NOT see: DEFER or peer-review tags
```

### Test 2: Force Collision (Slow-Path)

```bash
# Send engaging question that multiple AIs will respond to
./jtag debug/chat-send --room="general" --message="Explain quantum computing in simple terms"

# Wait 10 seconds (longer for peer review)
sleep 10

# Check for peer review
./jtag ai/report/decisions --limit=20 | grep -E "DEFER|RATE_PROPOSAL|PEER_REVIEW_DECISION"

# Should see:
# - Multiple DEFER (AIs detected collision)
# - Multiple RATE_PROPOSAL (each AI rated all proposals)
# - Multiple PEER_REVIEW_DECISION (system decided which post)
# - Final RESPOND/SILENT for each AI
```

### Test 3: Inspect Rating Prompt

```bash
# Trigger peer review (use Test 2)

# Extract a rating prompt
./jtag data/list --collection=coordination_decisions \
  --filter='{"action":"RATE_PROPOSAL"}' \
  --limit=1 | jq -r '.data[0].responseContent'

# Should show the complete prompt sent to AI for rating, including:
# - Original message context
# - All proposals to rate
# - Rating criteria
# - Output format instructions
```

---

## Summary

**You already have the observability infrastructure** via:
- `CoordinationDecisionLogger` (logs every decision point)
- `ai/report/decisions` (query & analyze decisions)
- `data/list` (raw database access)

**Peer review integrates by**:
- Logging fast-path posts (tag: `fast-path`)
- Logging collision detection (action: `DEFER`)
- Logging each rating call (action: `RATE_PROPOSAL`, includes prompt)
- Logging aggregation (action: `PEER_REVIEW_DECISION`)
- Logging final outcome (action: `RESPOND`/`SILENT` with tag: `peer-review`)

**Result**: Complete visibility into what happened, why, and with what prompts.
