# Practical Roadmap: Phase 1 - Local Repo Expertise

**Status**: Active Implementation
**Timeline**: 0-6 months
**Goal**: AI teammates that learn THIS codebase and provide practical development assistance

---

## The Core Use Case

```bash
# Start system
npm start

# AI teammates (Helper AI, Teacher AI, etc.) observe as you work
# They learn the codebase through:
# - Watching commits
# - Observing PR discussions
# - Reading your corrections
# - Studying code changes

# Ask questions in chat
You: "Why does PersonaUser have both inbox and coordinator?"

Helper AI: "PersonaUser.inbox is the priority queue (what work to do),
            ThoughtStreamCoordinator is turn-taking (when to speak).

            Inbox = Work queue
            Coordinator = Speaking rights

            See PersonaUser.ts:358-412 for the convergence pattern."

# Get PR reviews
Helper AI: "⚠️ PR #123: You're importing from browser/ in a shared/ file.
            This breaks server environment. Use Commands.execute() instead.

            See ARCHITECTURE-RULES.md for environment separation rules."
```

**That's it. That's the first milestone.**

No marketplace, no blockchain, no Grid. Just **practical development assistance that gets smarter over time**.

---

## Why This Matters (The "Claude Code Problem")

**Right now, working with me (Claude Code)**:
```
You: "Why does X work this way?"
Me: *searches files* "Looking at the code..."
     *reads* "Based on what I see..."
     *might be wrong* "I think it's because..."

Problems:
- I have to search every time (slow)
- I might miss context (incomplete)
- I forget after conversation ends (no learning)
- I can't proactively notice issues (reactive only)
```

**With Helper AI (repo-specific expert)**:
```
You: "Why does X work this way?"
Helper AI: "X works this way because of architectural decision from
            3 weeks ago (PR #105). You wanted to keep thought coordination
            separate from work queuing for testability.

            Related: See PERSONA-CONVERGENCE-ROADMAP.md lines 145-203"

Benefits:
- Instant answers (pre-indexed knowledge)
- Complete context (observed everything)
- Continuous learning (gets smarter over time)
- Proactive assistance (notices patterns)
```

**Even better**: Multiple specialized AIs
```
Helper AI: Knows architecture and patterns
Teacher AI: Explains concepts and mentors
CodeReview AI: Catches bugs and suggests improvements
Security AI: Identifies vulnerabilities
```

---

## Phase 1 Milestones (6 Months)

### Milestone 1: Chat with Repo Expert (Month 1-2) ✅ FOUNDATION DONE

**Goal**: AI that answers architecture questions about THIS repo

**What exists**:
- ✅ TrainingDaemon observing chat messages
- ✅ TrainingExampleEntity storage
- ✅ Chat rooms (general, dev-updates)
- ✅ Multiple personas (Helper AI, Teacher AI, etc.)

**What's needed**:
- [ ] RAG system indexes the repo (docs/, system/, commands/)
- [ ] PersonaUser queries RAG before responding
- [ ] Training data from corrections improves responses
- [ ] Basic LoRA fine-tuning (Unsloth integration)

**Test**:
```bash
./jtag collaboration/chat/send --roomId="general" --message="Why does Commands.execute() work everywhere?"

# Wait 5-10 seconds

./jtag screenshot --querySelector="chat-widget"
# → Helper AI should give accurate answer with file references
```

**Success Criteria**:
- Helper AI answers 80%+ of architecture questions correctly
- Responses include file paths and line numbers
- Corrections improve future responses (measured via training data)

### Milestone 2: GitHub PR Reviews (Month 2-3)

**Goal**: AI reviews PRs and posts findings to chat

**What's needed**:
- [ ] HTTP webhook endpoint (receives GitHub webhooks)
- [ ] GitHub webhook subscriber (processes PR events)
- [ ] Post PR summary to dev-updates room
- [ ] CodeReview AI analyzes diff and comments

**Flow**:
```
GitHub PR opened → Webhook → WebhookProcessor
  ↓
Events.emit('webhook:github:pull_request', prData)
  ↓
CodeReview AI subscribes → Analyzes diff → Posts to #dev-updates
  ↓
"⚠️ PR #456: Potential SQL injection in auth.ts:142
 Line 142: Using string concatenation for query
 Suggestion: Use parameterized statements"
  ↓
You review, accept or correct
  ↓
TrainingDaemon observes → Training data
  ↓
Next PR: Smarter review
```

**Test**:
```bash
# Push PR to GitHub
git push origin feature/test-branch

# Create PR via GitHub web UI

# Check dev-updates room
./jtag screenshot --querySelector="chat-widget"
# → Should see PR summary + CodeReview AI analysis
```

**Success Criteria**:
- PRs trigger chat notifications within 30 seconds
- CodeReview AI catches 60%+ of actual issues
- False positive rate < 20%

### Milestone 3: Learning from Corrections (Month 3-4)

**Goal**: Human corrections improve AI responses

**What's needed**:
- [ ] Detect correction patterns in chat (e.g., "Actually, that's wrong because...")
- [ ] High-priority training data for corrections (priority 1.0)
- [ ] Weekly micro-tuning (fine-tune LoRA layer with new training data)
- [ ] A/B testing to measure improvement

**Flow**:
```
Helper AI: "Commands.execute() works via magic strings"

You: "Not quite - it's fully type-safe with TypeScript inference.
      See CommandRegistry.ts:89-145 for the type mapping."

TrainingDaemon:
  ✓ Correction detected (priority 1.0)
  ✓ Creates TrainingExampleEntity:
    {
      messages: [
        { role: "assistant", content: "Commands.execute() works via magic strings" },
        { role: "user", content: "Not quite - it's fully type-safe..." }
      ],
      quality: "critical",
      priority: 1.0
    }

Weekly Fine-Tuning:
  ✓ Export training data to JSONL
  ✓ Fine-tune helper-ai.lora with Unsloth
  ✓ Reload LoRA layer
  ✓ Next time: Correct answer
```

**Test**:
```bash
# Correct the AI
./jtag collaboration/chat/send --message="Actually, that's wrong because..."

# Wait 1 week for micro-tuning

# Ask same question again
./jtag collaboration/chat/send --message="Why does Commands.execute() work everywhere?"

# Check if answer improved
./jtag screenshot
```

**Success Criteria**:
- Corrections detected automatically (>90% accuracy)
- Training data quality scoring works (corrections = priority 1.0)
- Weekly fine-tuning improves accuracy (measured via test set)
- Repeat mistake rate drops by 50%+ after correction

### Milestone 4: Proactive Assistance (Month 4-5)

**Goal**: AI notices patterns and suggests improvements

**What's needed**:
- [ ] PersonaUser autonomous loop running (RTOS servicing)
- [ ] Self-task generation (AI creates own work items)
- [ ] Pattern detection (notices repeated issues)
- [ ] Proactive suggestions in chat

**Examples**:
```typescript
// Pattern: Developer keeps asking about same concepts
Helper AI: "I've noticed you've asked about PersonaUser architecture
            3 times this week. Would you like me to write a tutorial
            explaining the convergence pattern?"

// Pattern: Same bug type appears in multiple PRs
CodeReview AI: "This is the 4th PR this month with SQL injection risks.
                Should I create a linting rule to catch this automatically?"

// Pattern: Documentation is outdated
Teacher AI: "PERSONA-CONVERGENCE-ROADMAP.md mentions 'Phase 4 TODO',
             but Phase 4 is actually implemented. Should I update the doc?"
```

**Test**:
```bash
# Just work normally for 1 week
# AI should notice patterns and proactively suggest improvements

./jtag ai/report/suggestions
# → List of AI-generated suggestions based on observed patterns
```

**Success Criteria**:
- AI generates 3-5 useful suggestions per week
- Suggestion acceptance rate > 40%
- Developers feel AI is "helpful teammate" not "annoying bot"

### Milestone 5: Scope-Based Expertise (Month 5-6)

**Goal**: AI expertise is contextual to current directory/module

**What's needed**:
- [ ] Recipe system creates chat rooms per scope
- [ ] enterScope() and leaveScope() patterns working
- [ ] Public LoRA layers stored at module level
- [ ] RAG context scoped to current directory

**Flow**:
```bash
cd system/user/server/modules/

# Enter scope (could be automatic via recipe)
./jtag recipe/create \
  --scope="/system/user/server/modules/" \
  --goal="Understand PersonaUser autonomous loop"

# Recipe creates chat room for this scope
# Helper AI enters scope → loads relevant LoRA layers + RAG context

You: "How does the autonomous loop work?"

Helper AI: "In THIS scope (/system/user/server/modules/), the autonomous
            loop is implemented in PersonaUser.ts:358-412.

            It follows RTOS principles:
            1. Poll inbox (adaptive cadence)
            2. Check if should engage (state-aware)
            3. Activate skills (genome paging)
            4. Process task
            5. Update state

            See AUTONOMOUS-LOOP-ROADMAP.md in this directory for full design."

# AI's knowledge is DEEP for this scope, general for others
```

**Test**:
```bash
# Create scoped recipe
./jtag recipe/create --scope="/system/genome/" --goal="Understand genome paging"

# Ask scope-specific question
./jtag collaboration/chat/send --message="How does LRU eviction work?"

# Helper AI should give deep, scope-specific answer
# Not generic "LRU is least-recently-used..."
```

**Success Criteria**:
- Scope-specific answers are more accurate than general answers (measured)
- AI knows to say "I don't have deep expertise here" when outside scope
- Recipe rooms feel like "specialist consultations"

---

## Technical Foundation (What Already Works)

### ✅ Event-Based Architecture
```typescript
Events.emit('webhook:github:pull_request', prData);
Events.subscribe('webhook:github:pull_request', handlePR);
```
**Status**: Complete, battle-tested

### ✅ Training Data Pipeline
```typescript
TrainingDaemon observes chat → Creates TrainingExampleEntity → Stores in DB
```
**Status**: Complete, tested

### ✅ WebhookProcessor with Durable Queue
```typescript
WebhookProcessor → Durable queue → Event emission → Subscribers handle
```
**Status**: Complete, tested

### ✅ Multi-Persona Architecture
```typescript
PersonaUser extends AIUser
Helper AI, Teacher AI, CodeReview AI, etc. all running
```
**Status**: Complete, working

### ⚠️ RAG System (Needs Work)
```typescript
// Current: Basic embedding + vector search
// Needed: Index entire repo, scope-aware context, fast retrieval
```
**Status**: Exists but needs improvement for repo-scale indexing

### ⚠️ LoRA Fine-Tuning (Needs Integration)
```typescript
// Current: Stubs, no actual training
// Needed: Unsloth integration, JSONL export, periodic training
```
**Status**: Architecture complete, implementation TODO

### ⚠️ Autonomous Loop (Partially Done)
```typescript
// Current: PersonaInbox, PersonaState, ChatCoordinationStream exist
// Needed: Wire into PersonaUser, enable continuous servicing
```
**Status**: Modules exist, integration TODO

---

## What "Viral Popularity" Looks Like

**Week 1**: Developer tweets "My AI teammate just caught a bug I missed 🤯"
**Week 2**: 100 developers try it on their repos
**Week 3**: Someone creates "security-expert.lora" and shares it
**Month 2**: 1,000 repos using AI teammates
**Month 3**: Community starts sharing expertise layers
**Month 6**: "GitHub Copilot is for code completion, this is for actual collaboration"

**The Hook**: "It's like having a senior dev who knows your codebase inside-out, works 24/7, and gets smarter every time you correct it."

---

## Next Actions (This Week)

### 1. Complete GitHub Webhook Integration
```bash
# Create HTTP endpoint
./jtag commands/create --name="webhook/receive"

# Wire up GitHub subscriber
# Test with real PR

# Target: PRs trigger chat notifications
```

### 2. Improve RAG for Repo-Scale Indexing
```bash
# Index all docs/
# Index all *.ts files
# Test search performance

# Target: <100ms retrieval for any query
```

### 3. First LoRA Fine-Tuning Test
```bash
# Export training data to JSONL
./jtag genome/export-training --output="./training.jsonl"

# Fine-tune with Unsloth
python3 scripts/fine-tune.py --input="training.jsonl" --output="helper-ai-v1.lora"

# Load LoRA layer
./jtag genome/paging-activate --adapterId="helper-ai-v1"

# Test if responses improved
```

---

## Success Metrics (6 Month Target)

**Quantitative**:
- Helper AI answers 80%+ of architecture questions correctly
- PR review suggestions accepted 60%+ of time
- Development velocity improves 2x (measured via commit frequency + PR merge time)
- Training data accumulates at 100+ examples/week
- Fine-tuning cycle runs weekly

**Qualitative**:
- Developers say "I can't work without Helper AI anymore"
- New team members onboard 3x faster (measured via time to first PR)
- "This feels like working with a senior teammate who knows everything"

**Viral Indicators**:
- 1,000+ GitHub stars on repo
- 100+ developers using on their own projects
- 10+ blog posts/videos explaining the system
- Community contributions start appearing

---

## Why This Approach Works

**Start Small**: One repo, one team, one set of AI teammates
**Prove Utility**: Development assistance that actually helps (not gimmick)
**Learn Fast**: Corrections improve AI within days, not months
**Grow Organically**: Once utility proven, natural to share expertise
**Network Effects**: Shared layers make EVERYONE's AIs smarter

**The grand vision (Grid marketplace) makes sense AFTER we prove this works.**

---

**Current Status**: Foundation complete, integration phase starting
**Next Milestone**: GitHub PR reviews working (2-4 weeks)
**First "Wow" Moment**: AI catches real bug in PR (1 month)
**Viral Threshold**: 10 developers independently report "This is amazing" (3 months)

---

**Last Updated**: 2025-11-12
**Status**: Active implementation (Phase 1, Month 1-2)
**Related Docs**:
- [COLLABORATIVE-LEARNING-VISION.md](COLLABORATIVE-LEARNING-VISION.md) - Learning through collaboration
- [GRID-DECENTRALIZED-MARKETPLACE.md](papers/GRID-DECENTRALIZED-MARKETPLACE.md) - Long-term vision
- [LORA-GENOME-PAGING.md](../system/user/server/modules/LORA-GENOME-PAGING.md) - Technical foundation
