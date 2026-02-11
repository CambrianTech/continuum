# Sentinel Workers: Focused Agentic Loops

## Vision

Sentinels are **focused agents** that sit between dumb scripts and full citizen AIs. They have enough intelligence to interpret results, adjust approach, and know when to escalate - but they're laser-focused on a single goal.

```
Dumb Scripts ←→ Sentinel Workers ←→ Full Citizen AIs
(no intelligence)   (focused loops)    (creative, conversational)
```

Full citizen AIs (PersonaUsers) dispatch Sentinels to handle repetitive-but-requires-judgment work, freeing themselves for creative decisions.

## Core Properties

Every Sentinel has:

1. **Goal** - Single, measurable objective ("build succeeds", "tests pass", "deployed and healthy")
2. **Loop** - Observe → Decide → Act → Repeat
3. **Bounds** - Max attempts, timeout, escalation trigger
4. **Memory** - What it's tried, what worked/failed
5. **Reporter** - Streams progress, returns final result

```typescript
interface Sentinel {
  goal: SentinelGoal;

  // The agentic loop
  async run(context: SentinelContext): Promise<SentinelResult>;

  // Bounds
  maxAttempts: number;
  timeoutMs: number;

  // When to give up and ask for help
  shouldEscalate(attempts: Attempt[]): boolean;

  // Progress reporting
  onProgress: (update: ProgressUpdate) => void;
}
```

## The Agentic Loop

Every Sentinel runs the same basic loop:

```
┌─────────────────────────────────────────┐
│                                         │
│   ┌──────────┐                          │
│   │ OBSERVE  │ ← Read output, logs,     │
│   └────┬─────┘   errors, state          │
│        │                                │
│        ▼                                │
│   ┌──────────┐                          │
│   │  DECIDE  │ ← Interpret: success?    │
│   └────┬─────┘   failure? recoverable?  │
│        │                                │
│        ▼                                │
│   ┌──────────┐                          │
│   │   ACT    │ ← Fix, retry, adjust,    │
│   └────┬─────┘   or escalate            │
│        │                                │
│        ▼                                │
│   Goal met? ──Yes──► Return Success     │
│        │                                │
│       No                                │
│        │                                │
│   Max attempts? ──Yes──► Escalate       │
│        │                                │
│       No ───────────────────────┘       │
│                                         │
└─────────────────────────────────────────┘
```

## Sentinel Types

### BuildSentinel

**Goal**: Code compiles without errors

**Loop**:
1. Run build command (`cargo build`, `npm run build:ts`)
2. Parse output for errors
3. If success → done
4. If error → analyze, attempt fix (or escalate)
5. Repeat until success or max attempts

**Intelligence required**:
- Parse compiler errors
- Identify file:line:column
- Suggest/attempt simple fixes (missing import, typo)
- Know when error is beyond its capability

```typescript
const buildSentinel = new BuildSentinel({
  command: 'npm run build:ts',
  maxAttempts: 3,
  canAutoFix: true,  // Attempt simple fixes
  escalateOn: ['architectural', 'ambiguous']
});

const result = await buildSentinel.run(workspaceContext);
// { success: true } or { success: false, error: '...', attempts: [...] }
```

### TestSentinel

**Goal**: All tests pass

**Loop**:
1. Run test command
2. Parse results (pass/fail/skip counts)
3. If all pass → done
4. If failures → analyze, attempt fix (or escalate)
5. Repeat

**Intelligence required**:
- Parse test output (jest, vitest, cargo test)
- Identify failing test and assertion
- Read test code to understand intent
- Suggest/attempt fixes

### DeploySentinel

**Goal**: Application deployed and healthy

**Loop**:
1. Run deploy sequence
2. Wait for startup
3. Health check (ping, smoke test)
4. If healthy → done
5. If unhealthy → analyze logs, attempt recovery
6. If unrecoverable → rollback, escalate

**Intelligence required**:
- Monitor startup logs
- Identify crash vs slow startup
- Execute rollback procedure
- Know when human intervention needed

### CodeFixSentinel

**Goal**: Specific error/issue resolved

**Loop**:
1. Read error context
2. Search codebase for relevant code
3. Analyze and propose fix
4. Apply fix
5. Verify (via BuildSentinel or TestSentinel)
6. If verified → done
7. If new errors → analyze, adjust

**Intelligence required**:
- Code search and comprehension
- Pattern matching (seen this error type before)
- Safe code modification
- Verification strategy

## Composition

Full citizen AIs compose Sentinels into workflows:

```typescript
// PersonaUser dispatches a coding task
async function implementFeature(spec: FeatureSpec): Promise<Result> {
  // 1. AI writes the code (creative work)
  const code = await this.writeCode(spec);

  // 2. Dispatch BuildSentinel (focused work)
  const buildResult = await this.dispatch(new BuildSentinel());
  if (!buildResult.success) {
    // BuildSentinel couldn't fix it, AI needs to help
    return this.handleBuildFailure(buildResult);
  }

  // 3. Dispatch TestSentinel
  const testResult = await this.dispatch(new TestSentinel());
  if (!testResult.success) {
    // Tests failing, AI decides: fix code or fix test?
    return this.handleTestFailure(testResult);
  }

  // 4. Dispatch DeploySentinel
  const deployResult = await this.dispatch(new DeploySentinel());

  return deployResult;
}
```

## Persistence & Sharing

Sentinels are **saveable**:

```typescript
// Save a configured Sentinel
const mySentinel = new BuildSentinel({
  command: 'npm run build:ts',
  maxAttempts: 5,
  patterns: ['TypeScript', 'React']
});
await mySentinel.saveTo('.continuum/sentinels/ts-build.json');

// Load and use
const sentinel = await Sentinel.load('.continuum/sentinels/ts-build.json');
await sentinel.run(context);
```

Sentinels can also save **memories**:
- Common error patterns and fixes
- Project-specific quirks
- What worked last time

```typescript
sentinel.memory.record({
  error: 'Cannot find module "lodash"',
  fix: 'npm install lodash',
  success: true
});
```

## Implementation Phases

### Phase 1: BuildSentinel (Prove the Pattern)

1. Create `Sentinel` base class/interface
2. Implement `BuildSentinel` with:
   - TypeScript build support
   - Rust build support
   - Error parsing
   - Simple auto-fix (missing imports)
3. Test with local AI team

### Phase 2: TestSentinel

1. Implement test running and parsing
2. Add test-specific intelligence
3. Wire up to BuildSentinel (build before test)

### Phase 3: Full Pipeline

1. Add DeploySentinel
2. Add CodeFixSentinel
3. Create pipeline orchestrator
4. Full citizen AIs can dispatch pipelines

### Phase 4: Learning Sentinels

1. Add memory/persistence
2. Pattern learning from successes
3. Share learnings across Sentinels
4. Sentinels improve over time

## Test Cases (End-to-End)

### Test 1: Fix a Type Error

**Setup**: Introduce a deliberate type error in a TS file

**Flow**:
1. Full AI notices error (or is told about it)
2. Dispatches CodeFixSentinel with error context
3. CodeFixSentinel: search → analyze → fix → verify via BuildSentinel
4. Reports success/failure

**Success criteria**: Error fixed, build passes, no human intervention

### Test 2: Add a Simple Function

**Setup**: Request "add a function that validates email addresses"

**Flow**:
1. Full AI writes the function
2. Dispatches BuildSentinel
3. If build fails, AI adjusts
4. Dispatches TestSentinel (if tests exist)
5. Reports completion

**Success criteria**: Function exists, compiles, passes tests

### Test 3: Refactor with Safety Net

**Setup**: Request "rename UserService to AccountService"

**Flow**:
1. Full AI plans the refactor
2. Makes changes incrementally
3. After each change, dispatches BuildSentinel
4. If anything breaks, CodeFixSentinel attempts recovery
5. Final TestSentinel run

**Success criteria**: Rename complete, all references updated, tests pass

### Test 4: Debug a Failing Test

**Setup**: A test is failing with unclear reason

**Flow**:
1. Full AI dispatches TestSentinel to identify failing test
2. Reads test code and implementation
3. Dispatches CodeFixSentinel with hypothesis
4. Verifies fix with TestSentinel
5. Reports root cause and fix

**Success criteria**: Test passes, AI explains what was wrong

## Integration with JTAG

Sentinels use existing JTAG infrastructure:

- `code/shell-execute` - Run build/test commands
- `code/read`, `code/write` - Modify source files
- `code/search` - Find relevant code
- `data/*` - Store sentinel memories
- `Events` - Progress reporting

Full citizen AIs dispatch Sentinels via:

```typescript
// New command
await Commands.execute('sentinel/dispatch', {
  type: 'build',
  config: { command: 'npm run build:ts', maxAttempts: 3 }
});

// Or direct instantiation in PersonaUser
const sentinel = new BuildSentinel(config);
const result = await sentinel.run(this.workspaceContext);
```

## Success Metrics

1. **Automation rate**: % of builds/tests/deploys handled without human intervention
2. **Fix rate**: % of errors auto-fixed by Sentinels
3. **Escalation accuracy**: When Sentinels escalate, was it correct?
4. **Time savings**: How much faster vs manual loop?
5. **AI productivity**: Can full AIs complete more tasks with Sentinel support?

## Philosophy

> "Sentinels are not trying to be creative or have conversations. They're the reliable coworker who handles the grind so you can focus on the interesting problems."

The goal is **empowerment through abstraction**:
- Dumb AIs become useful (focused loops are tractable)
- Smart AIs become productive (offload repetitive work)
- Humans stay in control (clear escalation paths)
- System improves over time (Sentinels learn)
