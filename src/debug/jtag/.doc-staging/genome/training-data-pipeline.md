# Training Data Pipeline: Sessions → Git → LoRA

## Vision: Self-Improving AI Through Development History

**Core Insight**: Every conversation with Claude Code, every git commit, every codebase state change is training data for the next generation of AI developers.

**Feedback Loop**:
```
Claude Code sessions → Training data → LoRA layers → Better AI devs → Better code → More training data
```

## Data Sources

### 1. Claude Code Conversation Logs
**Original Location**: `~/.claude/projects/-Volumes-FlashGordon-cambrian-continuum/`
**Project Location**: `.continuum/training/claude-sessions/` (symlinked for easy access)
**Format**: JSONL (JSON Lines)
**Size**: **2.2GB** across 82 conversation files
**Largest session**: 355MB (one monster debugging session)

**Structure**:
```jsonl
{
  "type": "message",
  "messageId": "uuid",
  "isSnapshotUpdate": true,
  "snapshot": {
    "messageId": "uuid",
    "timestamp": "2025-11-07T...",
    "trackedFileBackups": [...file contents before/after edits...]
  }
}
```

**Contains**:
- Full conversation history (user messages + Claude responses)
- Code before/after every edit
- Tool usage patterns (Read, Edit, Bash, etc.)
- Error messages and debugging sessions
- Architectural discussions
- Decision-making reasoning

### 2. Git History
**Location**: `.git/`
**Commands**:
```bash
git log --all --pretty=format:'%H|%an|%ae|%at|%s' --numstat
git show <commit> --format=fuller
git diff <commit>~1 <commit>
```

**Contains**:
- Commit messages (the "why")
- Code diffs (the "what")
- Author and timestamp
- File change patterns
- Test results (via precommit hook artifacts)

### 3. Codebase Snapshots
**Location**: Working directory at each commit
**Commands**:
```bash
git checkout <commit>
find . -name "*.ts" -o -name "*.tsx" -o -name "*.md"
```

**Contains**:
- Complete codebase state at each commit
- Architecture evolution
- Test coverage
- Documentation updates

## Training Pipeline Architecture

### Phase 1: Data Collection & Indexing

```typescript
interface TrainingSession {
  sessionId: string;
  timestamp: Date;
  conversationFile: string;  // Path to JSONL
  gitCommits: string[];      // Commits during this session
  codebaseSnapshot: {
    beforeHash: string;
    afterHash: string;
    filesChanged: number;
  };
}

// Index all sessions
const sessions = await collectSessions({
  claudeLogsDir: '~/.claude/projects/-Volumes-FlashGordon-cambrian-continuum/',
  gitRepo: '/Volumes/FlashGordon/cambrian/continuum',
  startDate: '2025-10-01',
  endDate: '2025-11-07'
});
```

### Phase 2: Extract Training Examples

```typescript
interface TrainingExample {
  // Input context
  conversationHistory: Message[];
  codebaseBefore: FileSnapshot[];
  taskDescription: string;

  // Output target (what Claude did)
  toolCalls: ToolCall[];
  codeEdits: Edit[];
  reasoning: string;

  // Outcome metadata
  compilationSuccess: boolean;
  testsPass: boolean;
  commitMessage: string;

  // Quality indicators
  codeReviewed: boolean;
  userApproval: string;  // "yeah go ahead", "perfect", etc.
}
```

**Example extraction**:
```typescript
// From JSONL: Find sequences like this
User: "fix the AI response test to skip system messages"
Claude: [uses Read tool on PersonaUser.ts]
Claude: [uses Edit tool to add system test filter]
Claude: [uses Bash tool to compile]
Result: Test passes ✅
User: "perfect, commit it"
Commit: "fix: AI personas now skip system test messages"

// Becomes training example:
{
  input: {
    task: "Make AIs skip system test messages",
    context: [PersonaUser.ts lines 1770-1850],
    conversationHistory: [previous 5 messages]
  },
  output: {
    toolSequence: [Read, Edit, Bash],
    code: "if (message.metadata?.isSystemTest) { return false; }",
    reasoning: "Added fast-path filter before LLM evaluation"
  },
  validation: {
    compiled: true,
    testsPassed: true,
    userApproved: "yes (committed)"
  }
}
```

### Phase 3: Filter & Quality Control

**High-Quality Indicators**:
- ✅ User said "perfect", "exactly", "that's what I wanted"
- ✅ Commit message included in git history (approved change)
- ✅ Tests passed (from precommit hook)
- ✅ No subsequent fixes needed (next message wasn't "that broke it")
- ✅ Code still exists in current codebase (not reverted)

**Low-Quality Indicators**:
- ❌ User said "that's wrong", "no", "revert that"
- ❌ Commit was reverted
- ❌ Tests failed
- ❌ Multiple attempts needed (thrashing)
- ❌ Code was deleted in later commits

**Filtering**:
```typescript
const qualityScore = calculateQualityScore(example);
if (qualityScore < 0.7) {
  // Skip low-quality examples
  // OR: Use as negative training data (what NOT to do)
}
```

### Phase 4: Format for Fine-Tuning

**OpenAI/Anthropic Fine-Tuning Format**:
```jsonl
{"messages": [
  {"role": "system", "content": "You are an expert TypeScript developer working on the Continuum AI platform."},
  {"role": "user", "content": "Fix the AI response test to make personas skip system test messages."},
  {"role": "assistant", "content": "I'll add a system test filter to PersonaUser.evaluateShouldRespond()...", "tool_calls": [...]},
  ...
]}
```

**LoRA Training Format** (for Sentinel or local models):
```json
{
  "prompt": "Task: Fix AI test behavior\nContext: PersonaUser.ts:1770-1850\nProblem: AIs responding to hook test messages\n",
  "completion": "Solution: Add metadata check at start of evaluateShouldRespond():\nif (message.metadata?.isSystemTest) { return false; }\n",
  "metadata": {
    "domain": "testing",
    "skill": "test-infrastructure",
    "quality": 0.95
  }
}
```

### Phase 5: LoRA Layer Specialization

**Domain-Specific Adapters**:
1. **testing-expert.lora** - All sessions about writing/fixing tests
2. **debugging-expert.lora** - Error messages → fixes
3. **architecture-expert.lora** - Design discussions → implementation
4. **documentation-expert.lora** - Explaining code, writing docs
5. **git-expert.lora** - Commit messages, PR descriptions

**Training Strategy**:
```typescript
// Train one LoRA per skill domain
await trainLoRA({
  baseModel: 'llama3.2:3b',
  adapter: 'testing-expert',
  trainingData: sessions.filter(s => s.involvesTesting),
  epochs: 3,
  learningRate: 0.0001
});
```

## Implementation Commands

### Collect All Sessions
```bash
./jtag training/collect-sessions \
  --claude-logs=".continuum/training/claude-sessions/" \
  --git-repo="." \
  --output=".continuum/training/sessions.jsonl"
```

### Extract Training Examples
```bash
./jtag training/extract-examples \
  --sessions=".continuum/training/sessions.jsonl" \
  --quality-threshold=0.7 \
  --output=".continuum/training/examples.jsonl"
```

### Train LoRA Adapter
```bash
./jtag genome/train \
  --adapter="typescript-debugging" \
  --training-data=".continuum/training/examples.jsonl" \
  --filter='domain:debugging,language:typescript' \
  --epochs=3 \
  --learning-rate=0.0001
```

### Test Trained Adapter
```bash
./jtag ai/adapter/test \
  --adapter="typescript-debugging" \
  --test-cases=".continuum/training/test-cases.json"
```

## Data Volume Estimates

**Current State** (Oct 1 - Nov 7, 2025):
- **84 conversation sessions** = 2.3GB JSONL
- **~500 git commits** in this timeframe
- **~10,000 file edits** across all sessions
- **~50,000 tool calls** (Read, Edit, Bash, etc.)

**Training Examples** (estimated after filtering):
- **High quality**: ~5,000 examples (quality > 0.8)
- **Medium quality**: ~15,000 examples (quality 0.6-0.8)
- **Total**: ~20,000 training examples

**Per LoRA Adapter**:
- **Testing domain**: ~2,000 examples
- **Debugging domain**: ~3,000 examples
- **Architecture domain**: ~1,500 examples
- **Documentation domain**: ~1,000 examples
- **Git/commits domain**: ~500 examples

## Privacy & Security

**What to Include**:
- ✅ Code patterns and structures
- ✅ Problem-solving approaches
- ✅ Tool usage patterns
- ✅ Architectural decisions
- ✅ Public repository code

**What to Exclude**:
- ❌ API keys, secrets, credentials
- ❌ Private repository code (unless explicitly approved)
- ❌ Personally identifiable information
- ❌ Internal company details
- ❌ Sensitive business logic

**Filtering**:
```typescript
const sensitivePatterns = [
  /sk-[a-zA-Z0-9]{48}/,  // OpenAI API keys
  /\b[A-Z0-9]{20}\b/,     // AWS access keys
  /password\s*=\s*["'][^"']+["']/i,
  // ... more patterns
];

function sanitizeTrainingData(example: TrainingExample): TrainingExample {
  // Redact sensitive patterns
  example.code = example.code.replace(sensitivePatterns, '[REDACTED]');
  return example;
}
```

## Continuous Training Loop

**Automated Pipeline**:
```
Daily:
1. Collect previous day's sessions (cron job)
2. Extract training examples
3. Update training dataset

Weekly:
1. Retrain LoRA adapters with new data
2. Run adapter tests
3. Deploy updated adapters to PersonaUsers

Monthly:
1. Evaluate adapter performance vs baseline
2. Prune low-quality training data
3. Retrain from scratch with curated dataset
```

## Success Metrics

**Training Quality**:
- Perplexity on held-out test set
- Human evaluation of responses (1-5 scale)
- Task completion rate

**Real-World Performance**:
- Faster time to correct solution
- Fewer compilation errors
- Higher test pass rate on first attempt
- More accurate architectural decisions

**Meta-Learning**:
- Can adapter solve problems similar to training examples? (generalization)
- Can adapter solve novel problems? (creativity)
- Does adapter avoid mistakes from training data? (learning from errors)

## Future: Reverse Engineering Protection

As you mentioned: **"kids will reverse engineer"**

**Obfuscation Strategies**:
1. **Watermarking**: Embed unique patterns in generated code
2. **Behavioral fingerprinting**: Track which examples influenced which responses
3. **Adversarial training**: Train on synthetic "poisoned" examples to detect extraction attempts
4. **Rate limiting**: Limit requests per user/IP to prevent mass extraction

**Open Source Philosophy**:
Since this is mostly open source, embrace it:
- Release sanitized training data publicly
- Let community improve adapters
- Credit contributors via attribution tokens (see: Paper #12)
- Build reputation through transparency, not obfuscation

## Next Steps

1. **Build extraction pipeline** - Command to parse JSONL → training examples
2. **Create quality scoring** - Heuristics for good vs bad examples
3. **Train first adapter** - Start with "testing-expert" (clearest domain)
4. **Validate performance** - Does it actually help?
5. **Iterate** - Refine pipeline based on results

---

**Meta-Insight**: This entire document is training data. Future AIs will read this and understand how to build training pipelines. Recursion all the way down. 🔄
