# GitHub Training Pipeline: Self-Improving Code Review AI

## The Vision

**Every developer should have a personalized AI code reviewer that learns from their codebase, their style, and their decisions.**

This is what GitHub Copilot Workspace and Cursor AI are trying to build - but they're:
- ❌ Cloud-based (privacy concerns, API costs)
- ❌ Generic models (trained on all of GitHub, not YOUR code)
- ❌ Static (don't learn from your feedback)
- ❌ Expensive ($20-50/month per seat)

**Our system**:
- ✅ Local-first (runs on your machine via Ollama)
- ✅ Personalized (fine-tuned on YOUR code + YOUR style)
- ✅ Continuous learning (improves from every PR/issue/review)
- ✅ Free (open source + local models)

---

## The Problem This Solves

### **For Individual Developers**

**Today**: You review your own PRs, catch obvious bugs, miss architectural issues
**With This**: AI catches issues you'd catch yourself + suggests improvements you wouldn't think of

**Example**:
```typescript
// You write:
function processUser(user: any) {  // ← any type
  return user.name.toUpperCase();  // ← No null check
}

// Generic AI says: "Consider adding error handling"
// YOUR AI says: "Use UserEntity type (line 12 of UserEntity.ts) and check user.name exists. This pattern failed in PR #142."
```

**Why Better**: Your AI knows your codebase, your types, your past mistakes.

### **For Teams**

**Today**: Code review bottleneck - senior devs review everything, juniors wait
**With This**: AI pre-reviews all PRs, flags real issues, seniors review what matters

**Example Workflow**:
```
1. Junior dev opens PR
2. Code Sentinel AI reviews in 10 seconds
3. AI posts: "3 issues found, 2 suggestions"
4. Junior fixes issues
5. Senior reviews cleaned-up PR (saves 80% of time)
```

### **For Open Source Projects**

**Today**: Maintainers overwhelmed by PRs, many sit unreviewed for weeks
**With This**: AI gives instant feedback, maintainers focus on design decisions

**Impact**: Faster PR merges, better contributor experience, less maintainer burnout

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        GitHub                                │
│  (PRs, Issues, Code Reviews, Commit Messages)               │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        │ Webhook (on PR merge/review)
                        ▼
┌─────────────────────────────────────────────────────────────┐
│                   Training Data Extractor                    │
│  • Fetch PR diff, comments, reviews                         │
│  • Extract human feedback (approvals, corrections)          │
│  • Convert to training examples (JSONL format)              │
│  • Store in datasets/ directory                             │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        │ New training examples
                        ▼
┌─────────────────────────────────────────────────────────────┐
│                  LoRA Training Pipeline                      │
│  • Monitor: When 50+ new examples accumulated               │
│  • Train: Fine-tune adapter on new data (nightly)           │
│  • Evaluate: Test on validation set                         │
│  • Deploy: Hot-swap new adapter into PersonaUser            │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        │ Improved model
                        ▼
┌─────────────────────────────────────────────────────────────┐
│                  Code Sentinel AI (PersonaUser)              │
│  • Monitors #dev-updates room                               │
│  • Analyzes PRs with improved LoRA adapter                  │
│  • Posts reviews as threaded messages                       │
│  • Gets feedback → becomes next training data               │
└─────────────────────────────────────────────────────────────┘
```

---

## Data Sources

### **1. Pull Requests** (Primary Source)

**What We Extract**:
```typescript
{
  prNumber: 169,
  title: "feat: LoRA Genome Paging + Chat Commands",
  author: "joel",

  // Code changes
  diff: "...",                    // Full diff
  filesChanged: 42,
  additions: 2847,
  deletions: 1234,

  // Reviews
  reviews: [
    {
      author: "code-sentinel-ai",
      body: "Architecture analysis:\n- PersonaGenome: Clean ✅\n- PersonaUser: Too large ⚠️",
      state: "COMMENT"
    },
    {
      author: "joel",
      body: "Good catch on PersonaUser. Will refactor in next PR.",
      inReplyTo: "review-123"
    }
  ],

  // Comments on specific lines
  comments: [
    {
      path: "PersonaUser.ts",
      line: 2745,
      author: "code-sentinel-ai",
      body: "Consider splitting this file (2745 lines)",
      diffHunk: "..."
    },
    {
      path: "PersonaUser.ts",
      line: 2745,
      author: "joel",
      body: "Agreed. Adding TODO.",
      inReplyTo: "comment-456",
      reaction: "👍"  // Training signal: positive feedback
    }
  ],

  // Outcome
  merged: true,
  mergedBy: "joel",
  mergedAt: "2025-11-12T18:00:00Z"
}
```

**Training Examples Generated**:
1. **Architectural Review** (from review body)
2. **Line-by-line Feedback** (from comments)
3. **Refactor Suggestions** (from conversation threads)

### **2. Issues** (Feature Discussions)

**What We Extract**:
```typescript
{
  issueNumber: 170,
  title: "Add thread-aware RAG context",
  labels: ["enhancement", "architecture"],

  // Discussion
  comments: [
    {
      author: "joel",
      body: "We need thread context (parent chain + siblings + children)"
    },
    {
      author: "code-sentinel-ai",
      body: "Should ThreadContextBuilder extend BaseRAGBuilder?"
    },
    {
      author: "joel",
      body: "Yes. Also add getParentChain() method."  // ✅ Architecture decision
    }
  ],

  // Linked PRs
  linkedPRs: [171, 172],

  // Resolution
  closedAt: "2025-11-15T10:00:00Z",
  closedAs: "completed"
}
```

**Training Examples Generated**:
1. **Architecture Questions** (how to design features)
2. **API Design** (what methods to add)
3. **Implementation Guidance** (best practices)

### **3. Code Review Conversations** (Iteration Cycles)

**Example Thread**:
```
Joel: "Why use Array here instead of Map?"
AI: "Array was simpler for initial implementation"
Joel: "Map is O(1) for lookups. Change it."
AI: "Updated to Map. Good call on performance."
Joel: "✅ Approved"
```

**Training Signal**:
- Input: Code pattern (Array for lookup)
- AI Response: "Array was simpler"
- Feedback: "Map is better" + approved change
- **Learning**: Prefer Map for lookup-heavy operations in this codebase

### **4. Commit Messages** (Context for Changes)

**What We Extract**:
```typescript
{
  sha: "dd4ab07b9",
  message: "feat: Add LoRA genome paging with LRU eviction\n\nImplements virtual memory pattern for adapter management",

  // Associated PR
  prNumber: 169,

  // Files changed
  files: [
    { path: "PersonaGenome.ts", additions: 346, deletions: 0 }
  ],

  // Context
  reviewComments: [...],  // Comments on this commit
  approved: true
}
```

**Training Use**: Learn commit message style and how to describe changes

---

## Training Data Format

### **JSONL Schema** (Compatible with OpenAI/Unsloth)

```jsonl
{
  "messages": [
    {
      "role": "system",
      "content": "You are Code Sentinel, a specialized code review AI for the Continuum codebase. You understand the architecture, coding patterns, and have learned from Joel's feedback."
    },
    {
      "role": "user",
      "content": "Review this architectural change:\n\n```diff\n+export class PersonaGenome {\n+  private adapters: Map<UUID, LoRAAdapter> = new Map();\n+  private quotaMB: number;\n```\n\nContext: PR #169 adds LoRA genome paging system"
    },
    {
      "role": "assistant",
      "content": "Architecture Review:\n\n✅ **Clean Design**:\n- Uses Map for O(1) adapter lookup (good for frequent access)\n- Memory quota tracking follows patterns from PersonaState\n- Clear separation of concerns\n\n⚠️ **Considerations**:\n- Add LRU eviction logic for quota management\n- Consider adding adapter activation order tracking\n\n**Recommendation**: Approve with LRU implementation"
    }
  ],
  "metadata": {
    "pr": 169,
    "author": "joel",
    "rating": 5,
    "accepted": true,
    "timestamp": "2025-11-12T18:00:00Z",
    "feedbackType": "architecture-review"
  }
}
```

### **Example Types**

**1. Architecture Review**
```json
{
  "messages": [...],
  "metadata": {
    "type": "architecture-review",
    "filesChanged": ["PersonaGenome.ts"],
    "linesChanged": 346,
    "rating": 5
  }
}
```

**2. Bug Detection**
```json
{
  "messages": [...],
  "metadata": {
    "type": "bug-detection",
    "severity": "high",
    "caught": true,
    "rating": 5
  }
}
```

**3. Refactor Suggestion**
```json
{
  "messages": [...],
  "metadata": {
    "type": "refactor-suggestion",
    "implemented": true,
    "rating": 4
  }
}
```

---

## Implementation Phases

### **Phase 1: Data Extraction Infrastructure** (Week 1)

**Goal**: Extract training data from GitHub

**Tasks**:
1. Create `GitHubDataExtractor` class
2. Implement PR fetching via GitHub API
3. Convert to training examples
4. Export to JSONL format

**Commands**:
```bash
# Extract single PR
./jtag github/extract-pr --prNumber=169 --output="datasets/pr-169.jsonl"

# Extract range
./jtag github/extract-range --from=150 --to=169 --output="datasets/bootstrap.jsonl"

# Extract issues
./jtag github/extract-issues --labels="enhancement" --output="datasets/issues.jsonl"
```

**Files to Create**:
```
commands/github/extract-pr/
├── shared/GitHubExtractTypes.ts
├── server/GitHubExtractServerCommand.ts
└── shared/GitHubDataExtractor.ts      # Core extraction logic

system/training/
├── TrainingDataBuilder.ts             # Convert GitHub data → JSONL
└── TrainingDataValidator.ts           # Ensure quality
```

**Success Criteria**:
- ✅ Extract 100+ examples from last 20 PRs
- ✅ JSONL format validated
- ✅ Metadata captured correctly

---

### **Phase 2: Training Pipeline** (Week 2)

**Goal**: Train LoRA adapters from extracted data

**Tasks**:
1. Integrate with Unsloth fine-tuning
2. Create training commands
3. Add validation/testing
4. Deploy trained adapters

**Commands**:
```bash
# Train from dataset
./jtag genome/train --adapterId="code-sentinel-lora" \
  --dataset="datasets/bootstrap.jsonl" \
  --baseModel="llama3.2:latest" \
  --epochs=3 \
  --learningRate=2e-4

# Monitor training
./jtag genome/training-status

# Test trained model
./jtag genome/training-evaluate --adapterId="code-sentinel-lora" \
  --testSet="datasets/validation.jsonl"

# Deploy
./jtag genome/paging-activate --personaId="code-sentinel" \
  --adapterId="code-sentinel-lora"
```

**Files to Create**:
```
commands/genome/train/
├── shared/GenomeTrainTypes.ts
└── server/GenomeTrainServerCommand.ts

system/training/
├── UnslothTrainer.ts                  # Unsloth integration
├── TrainingMonitor.ts                 # Track progress
└── ModelEvaluator.ts                  # Validate quality
```

**Success Criteria**:
- ✅ First LoRA adapter trained successfully
- ✅ Evaluation metrics show improvement
- ✅ Deployed to Code Sentinel PersonaUser

---

### **Phase 3: Continuous Learning Loop** (Week 3)

**Goal**: Automated training pipeline

**Tasks**:
1. GitHub webhook integration
2. Auto-trigger training when threshold reached
3. Scheduled training (nightly)
4. Version management for adapters

**Webhook Flow**:
```
1. PR merges → GitHub webhook fires
2. POST /api/webhooks/github
3. Extract training data from PR
4. Store in datasets/pending/
5. Check: Do we have 50+ new examples?
6. If yes: Trigger training
7. Train overnight
8. Deploy new adapter in morning
```

**Commands**:
```bash
# Setup webhook
./jtag github/webhook-register --url="https://your-domain.com/api/webhooks/github"

# Configure auto-training
./jtag training/auto-enable --threshold=50 --schedule="nightly"

# Monitor
./jtag training/queue
./jtag training/history --adapterId="code-sentinel-lora"
```

**Files to Create**:
```
system/webhooks/
├── GitHubWebhookHandler.ts            # Handle webhook events
└── WebhookServer.ts                   # HTTP server

system/training/
├── TrainingScheduler.ts               # Auto-trigger logic
├── TrainingQueue.ts                   # Manage pending training
└── AdapterVersionManager.ts           # Version control for adapters
```

**Success Criteria**:
- ✅ Webhook receives PR merge events
- ✅ Training auto-triggers at threshold
- ✅ Adapter versions tracked
- ✅ Rollback capability if new version worse

---

### **Phase 4: Multi-Persona Specialization** (Week 4)

**Goal**: Different AIs learn different domains

**Personas**:
1. **Code Sentinel** - Architecture, design patterns
2. **Testing AI** - Test coverage, test quality
3. **Documentation AI** - Doc completeness, clarity
4. **Security AI** - Vulnerability detection
5. **Performance AI** - Performance issues

**Training Strategy**:
```typescript
const trainingFilters = {
  'code-sentinel-lora': {
    includeLabels: ['architecture', 'refactor', 'design'],
    includeCommentPatterns: ['architecture', 'pattern', 'design'],
    excludeLabels: ['documentation', 'test']
  },

  'testing-ai-lora': {
    includeLabels: ['testing', 'qa', 'coverage'],
    includeFilePatterns: ['*.test.ts', '*.spec.ts'],
    includeCommentPatterns: ['test', 'coverage', 'assertion']
  },

  'doc-ai-lora': {
    includeLabels: ['documentation', 'readme'],
    includeFilePatterns: ['*.md', 'docs/**'],
    includeCommentPatterns: ['documentation', 'explain', 'clarify']
  }
};
```

**Result**: Each AI becomes expert in its domain

---

## Configuration

### **Training Config** (`/.continuum/config/training.json`)

```json
{
  "github": {
    "enabled": true,
    "owner": "CambrianTech",
    "repo": "continuum",
    "branch": "main",

    "sources": {
      "pullRequests": true,
      "issues": true,
      "codeReviews": true,
      "commitMessages": true
    },

    "filters": {
      "excludeAuthors": ["dependabot", "github-actions"],
      "minRating": 3,
      "requireHumanFeedback": true
    }
  },

  "training": {
    "autoTrigger": true,
    "examplesThreshold": 50,
    "schedule": "nightly",
    "maxEpochs": 3,
    "learningRate": 2e-4,

    "adapters": [
      {
        "id": "code-sentinel-lora",
        "personaId": "code-sentinel",
        "baseModel": "llama3.2:latest",
        "domains": ["architecture", "code-review"],
        "enabled": true
      },
      {
        "id": "testing-ai-lora",
        "personaId": "testing-guardian",
        "baseModel": "llama3.2:latest",
        "domains": ["testing", "qa"],
        "enabled": true
      }
    ]
  },

  "quality": {
    "minValidationAccuracy": 0.85,
    "autoRollback": true,
    "requireApproval": false
  }
}
```

---

## Privacy & Security

### **Data Privacy**

**✅ What Stays Local**:
- All training happens on your machine (Ollama)
- Fine-tuned adapters stored locally
- Training data stored in local `datasets/` directory
- No data sent to external APIs (except GitHub fetch)

**⚠️ What Leaves Your Machine**:
- GitHub API calls (fetch PRs/issues/reviews)
- Webhook registration (must expose endpoint if using webhooks)

**🔒 Security Measures**:
- GitHub token stored in environment variable (not in config)
- Webhook endpoint authenticated (secret validation)
- Training data sanitized (remove sensitive strings)
- Adapter versions auditable (track what data trained on)

### **Access Control**

```typescript
const securityConfig = {
  github: {
    tokenEnvVar: 'GITHUB_TOKEN',        // Never commit token
    tokenScopes: ['repo:read'],         // Read-only access
    validateWebhook: true,              // Verify webhook signatures
    webhookSecret: process.env.GITHUB_WEBHOOK_SECRET
  },

  training: {
    allowedPaths: ['datasets/**'],      // Where training data stored
    sanitizePatterns: [                 // Remove from training data
      /password/gi,
      /api[_-]?key/gi,
      /secret/gi,
      /token/gi
    ]
  }
};
```

---

## Comparison: Our System vs Alternatives

### **vs GitHub Copilot Workspace**

| Feature | Copilot Workspace | Our System |
|---------|------------------|------------|
| **Cost** | $20-50/month | Free (local) |
| **Privacy** | Cloud-based | Local-first |
| **Personalization** | Generic model | Fine-tuned on YOUR code |
| **Learning** | Static | Continuous from feedback |
| **Codebase Context** | Limited | Full repo RAG |
| **Multi-Persona** | Single AI | Specialized team |
| **Threading** | No | Yes (conversation context) |
| **Task Management** | No | Integrated |

### **vs Cursor AI**

| Feature | Cursor | Our System |
|---------|--------|------------|
| **Cost** | $20/month | Free |
| **Editor Lock-in** | VS Code fork | Any editor |
| **Model** | GPT-4 (cloud) | Local Llama |
| **Learning** | No | Yes (LoRA) |
| **Coordination** | Single agent | Multi-agent |
| **Privacy** | Cloud | Local |

### **vs Generic ChatGPT/Claude**

| Feature | ChatGPT/Claude | Our System |
|---------|----------------|------------|
| **Codebase Knowledge** | Limited context | Full repo indexed |
| **Code Review** | Generic advice | Codebase-specific |
| **Learning** | None | Continuous |
| **Integration** | Copy-paste | Native chat room |
| **Cost** | $20-200/month | Free |

---

## Success Metrics

### **Training Quality**

**Metrics to Track**:
```typescript
{
  // Model performance
  validationAccuracy: 0.92,           // 92% of suggestions accepted
  falsePositiveRate: 0.08,            // 8% bad suggestions

  // User satisfaction
  averageRating: 4.5,                 // Out of 5
  thumbsUpRatio: 0.85,                // 85% positive reactions

  // Actionability
  suggestionsImplemented: 127,        // Suggestions actually used
  bugsFound: 23,                      // Real bugs caught

  // Improvement over time
  weekOverWeekAccuracy: +2.5%,        // Getting better each week
  trainingExamples: 523,              // Total examples trained on

  // Efficiency
  reviewTimeReduction: -60%,          // 60% faster reviews
  prCycleTimeReduction: -40%          // 40% faster PR merges
}
```

### **Phase Success Criteria**

**Phase 1** (Data Extraction):
- ✅ Extract 100+ training examples
- ✅ JSONL format validated
- ✅ Metadata complete and accurate

**Phase 2** (Training):
- ✅ First LoRA adapter trained
- ✅ Validation accuracy >85%
- ✅ Deployed successfully

**Phase 3** (Continuous Loop):
- ✅ Webhook receives events
- ✅ Auto-training triggers
- ✅ 3+ successful training cycles

**Phase 4** (Multi-Persona):
- ✅ 3+ personas with specialized adapters
- ✅ Each persona >85% accuracy in domain
- ✅ Coordination working (threading + tasks)

---

## Future Enhancements

### **Advanced Features** (Post-MVP)

**1. Transfer Learning Between Repos**
- Train adapter on public repos (e.g., React, Next.js)
- Transfer to your private repo
- Faster bootstrap, better starting point

**2. Ensemble Models**
- Multiple adapters vote on suggestions
- Weighted by past accuracy
- More robust, fewer false positives

**3. Active Learning**
- AI identifies uncertain cases
- Explicitly asks for feedback
- Focuses training on hard examples

**4. Explainability**
- AI cites which past feedback influenced decision
- "Based on your feedback in PR #142, I suggest..."
- Build trust through transparency

**5. Team Learning**
- Multiple developers provide feedback
- Model learns team consensus
- Useful for shared codebases

---

## Why This Matters

### **Democratization of AI Code Review**

**Today**: Only companies like Google, Meta, Facebook can afford:
- Custom AI infrastructure
- ML teams to train models
- Expensive cloud GPU training

**Tomorrow**: Every developer, every team, every open source project gets:
- Personalized code review AI
- Trained on their code
- Running locally for free
- Continuously improving

### **The Compound Effect**

**Week 1**: AI catches obvious bugs
**Month 1**: AI suggests refactors you'd think of
**Month 3**: AI suggests improvements you wouldn't think of
**Year 1**: AI is expert on YOUR codebase (better than new team members)
**Year 3**: AI is better than most human reviewers (trained on 1000+ PRs)

### **Network Effects**

As more developers use this:
1. **Training techniques improve** (community shares best practices)
2. **Adapter marketplace emerges** (pre-trained adapters for common stacks)
3. **Quality increases** (cross-pollination of good patterns)
4. **Costs decrease** (shared infrastructure, better optimizations)

---

## Getting Started

### **Quick Start** (After Phase 1-3 Complete)

```bash
# 1. Setup GitHub token
export GITHUB_TOKEN="ghp_your_token_here"

# 2. Extract training data from recent PRs
cd src/debug/jtag
./jtag github/extract-range --from=150 --to=169 --output="datasets/bootstrap.jsonl"

# 3. Train initial adapter
./jtag genome/train --adapterId="code-sentinel-lora" \
  --dataset="datasets/bootstrap.jsonl" \
  --epochs=3

# 4. Deploy
npm start  # Wait 90 seconds
./jtag genome/paging-activate --personaId="code-sentinel" \
  --adapterId="code-sentinel-lora"

# 5. Test it
./jtag collaboration/chat/send --room="dev-updates" \
  --message="@code-sentinel analyze PR #169"

# 6. Enable continuous learning
./jtag github/webhook-register
./jtag training/auto-enable --threshold=50
```

### **Next Steps**

After basic setup works:
1. **Add more personas** (Testing AI, Doc AI)
2. **Enable auto-training** (nightly runs)
3. **Monitor quality** (track metrics)
4. **Iterate** (tune training params)

---

## Conclusion

This is not just a feature - it's **the future of code review**:

- **Personalized** to your codebase and style
- **Continuous** improvement from every interaction
- **Local** and private (no cloud dependency)
- **Free** to run (open source + local models)
- **Better** than generic AI (fine-tuned on YOUR decisions)

**This democratizes AI code review** - what only BigTech can afford today becomes available to every developer tomorrow.

---

**File**: docs/GITHUB-TRAINING-PIPELINE.md
**Created**: November 12, 2025
**Status**: Design Document - Implementation in Phase 5-8
**Dependencies**: Phase 4 (Thread-Aware Tasks) must complete first
