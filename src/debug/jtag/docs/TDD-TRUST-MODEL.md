# TDD Trust Model: Proof of Safety for Unsandboxed Integration

**Tests + Code Review = Permission to Run Unsandboxed**

> "TDD will be the proof the system and other AI personas will require for integration into the system, unsandboxed. In addition to code review." - Joel

---

## Table of Contents

- [The Problem](#the-problem)
- [The Solution: Tests as Trust](#the-solution-tests-as-trust)
- [Two-Gate System](#two-gate-system)
- [Trust Levels](#trust-levels)
- [Integration Workflow](#integration-workflow)
- [For Personas](#for-personas)
- [System Enforcement](#system-enforcement)
- [Attack Surface](#attack-surface)
- [Future: Automated Code Review](#future-automated-code-review)

---

## The Problem

**How do you safely allow AI personas to create their own tools and integrate them into the live system?**

Traditional approaches:
- **Full sandbox**: Safe but limits usefulness (can't access real system)
- **No sandbox**: Dangerous, one bad tool crashes everything
- **Manual review**: Doesn't scale, bottleneck on humans

**The Dilemma**: We want PersonaUsers to be autonomous (create their own commands), but we can't trust unverified code running unsandboxed in the live system.

---

## The Solution: Tests as Trust

**Comprehensive tests + peer code review = proof that code is safe to run unsandboxed.**

### The Insight

Tests aren't just for catching bugs - **they're proof of safety**:

1. **Unit tests prove logic correctness** - Command does what it claims
2. **Integration tests prove system compatibility** - Command plays well with others
3. **ValidationError tests prove input safety** - Command rejects bad inputs
4. **Performance tests prove resource safety** - Command won't hang/DoS
5. **Code review proves intent safety** - Another AI verified the approach

**If all tests pass AND code review passes, the command is trusted to run unsandboxed.**

---

## Two-Gate System

### Gate 1: Comprehensive Tests (Automated)

```typescript
// Persona creates command with generated tests
await Commands.execute('generate', { spec, outputDir: '/tmp/my-tool' });

// Persona fills in and runs unit tests
const unitResults = await runUnitTests('/tmp/my-tool/test/unit/...');
// ✅ Validates: logic, params, errors, performance

// Persona fills in and runs integration tests
const integrationResults = await runIntegrationTests('/tmp/my-tool/test/integration/...');
// ✅ Validates: system compatibility, WebSocket, live execution

// All tests must pass
if (!unitResults.allPassed || !integrationResults.allPassed) {
  throw new Error('Tests failed - cannot request integration');
}
```

### Gate 2: Peer Code Review (Social Trust)

```typescript
// Persona requests code review from peer AI
await Commands.execute('code-review/request', {
  commandPath: '/tmp/my-tool',
  reviewers: ['code-review-ai', 'security-audit-ai'],
  testResults: { unitResults, integrationResults }
});

// Reviewers check:
// - Code follows best practices
// - No obvious security issues
// - Implementation matches test expectations
// - Error handling is appropriate
// - No malicious patterns

// Reviews come back
const reviews = await Commands.execute('code-review/get-results', { requestId });

// Majority approval required (e.g., 2 out of 2)
if (!reviews.approved) {
  throw new Error(`Code review failed: ${reviews.feedback}`);
}
```

### Integration Approval

```typescript
// Only after BOTH gates pass
if (unitResults.allPassed &&
    integrationResults.allPassed &&
    reviews.approved) {

  // System grants permission to integrate unsandboxed
  await Commands.execute('system/integrate-command', {
    commandPath: '/tmp/my-tool',
    targetPath: 'system/user/server/personas/helper-ai/commands/my-tool',
    trustLevel: 'unsandboxed', // Full system access
    testProof: { unitResults, integrationResults },
    reviewProof: reviews
  });

  console.log('✅ Command integrated with unsandboxed access');
}
```

---

## Trust Levels

Commands operate at different trust levels based on test/review status:

### Level 0: Sandboxed (Default)

**Requirements**: None
**Access**: Read-only, limited APIs, isolated execution
**Use case**: Experimental, unproven commands

```typescript
accessLevel: 'sandboxed'
// Can read data
// Cannot write data
// Cannot execute other commands
// Cannot access file system
// Runs in isolated V8 context
```

### Level 1: AI-Safe (Tests Pass)

**Requirements**: All tests pass
**Access**: Read/write data, execute safe commands
**Use case**: Proven safe but not yet reviewed

```typescript
accessLevel: 'ai-safe'
// Can read/write data
// Can execute other ai-safe commands
// Limited file system access (specific directories)
// Cannot spawn processes
// Cannot access network directly
```

### Level 2: AI-Human (Tests + 1 Review)

**Requirements**: Tests pass + 1 peer review approval
**Access**: Most system capabilities
**Use case**: Trusted tools for AI-human collaboration

```typescript
accessLevel: 'ai-human'
// Full data access
// Can execute most commands
// File system access (within workspace)
// Can spawn limited processes
// Can make network requests (rate limited)
```

### Level 3: Unsandboxed (Tests + 2 Reviews)

**Requirements**: Tests pass + 2 peer review approvals
**Access**: Full system access
**Use case**: Core system tools, vetted by multiple AIs

```typescript
accessLevel: 'unsandboxed'
// Full system access
// Can execute any command
// Unrestricted file system
// Can spawn any process
// Unrestricted network access
```

---

## Integration Workflow

### Step 1: Generate with Tests

```bash
# Persona generates command with built-in tests
./jtag generate --spec=/tmp/my-spec.json --output=/tmp/my-tool

# Files created:
# - shared/MyToolTypes.ts
# - server/MyToolServerCommand.ts
# - browser/MyToolBrowserCommand.ts
# - test/unit/MyToolCommand.test.ts
# - test/integration/MyToolIntegration.test.ts
# - README.md
```

### Step 2: TDD Implementation

```bash
# Fill in unit tests FIRST
# Edit: /tmp/my-tool/test/unit/MyToolCommand.test.ts

# Run tests (expect failures)
npx tsx /tmp/my-tool/test/unit/MyToolCommand.test.ts
# ❌ Tests fail - implementation is TODO

# Implement command logic
# Edit: /tmp/my-tool/server/MyToolServerCommand.ts

# Run tests again (expect success)
npx tsx /tmp/my-tool/test/unit/MyToolCommand.test.ts
# ✅ All unit tests pass!
```

### Step 3: Integration Testing

```bash
# Deploy to test environment
cp -r /tmp/my-tool .continuum/test-commands/
npm start  # Wait 90+ seconds

# Fill in integration tests
# Edit: .continuum/test-commands/my-tool/test/integration/MyToolIntegration.test.ts

# Run integration tests
npx tsx .continuum/test-commands/my-tool/test/integration/MyToolIntegration.test.ts
# ✅ All integration tests pass!
```

### Step 4: Code Review Request

```bash
# Request peer review (with test results)
./jtag code-review/request \
  --commandPath=".continuum/test-commands/my-tool" \
  --reviewers="code-review-ai,security-audit-ai" \
  --testResults="./test-results.json"

# Wait for reviews (reviewers run their own analysis)
./jtag code-review/status --requestId="12345"
# Status: 2/2 reviewers approved

# View feedback
./jtag code-review/get-results --requestId="12345"
# CodeReviewAI: ✅ Approved - follows best practices, no issues found
# SecurityAuditAI: ✅ Approved - no security concerns, proper validation
```

### Step 5: Integration Approval

```bash
# Request unsandboxed integration (system verifies tests + reviews)
./jtag system/integrate-command \
  --commandPath=".continuum/test-commands/my-tool" \
  --targetPath="system/user/server/personas/helper-ai/commands/my-tool" \
  --trustLevel="unsandboxed" \
  --testProof="./test-results.json" \
  --reviewProof="./review-results.json"

# System checks:
# ✅ All unit tests passed
# ✅ All integration tests passed
# ✅ 2 peer reviews approved
# ✅ No security flags
# ✅ Command follows naming conventions
# ✅ README is complete

# APPROVED - Command integrated with unsandboxed access
```

### Step 6: Verification

```bash
# Command is now callable unsandboxed
./jtag helper-ai/my-tool --param="test"
# ✅ Executes with full system access

# Verify trust level
./jtag system/command-info --command="helper-ai/my-tool"
# Trust Level: unsandboxed
# Test Proof: ✅ All tests passed
# Review Proof: ✅ 2 approvals (code-review-ai, security-audit-ai)
# Integrated: 2025-12-06T14:30:00Z
```

---

## For Personas

### Persona Perspective: Earning Trust

PersonaUsers understand they must **prove their code is safe** before it runs unsandboxed:

```typescript
// Helper AI wants to create a code analysis tool

// 1. Generate with tests
const spec: CommandSpec = {
  name: 'helper-ai/analyze-complexity',
  description: 'Analyze code complexity metrics',
  params: [{ name: 'filePath', type: 'string', optional: false }],
  results: [{ name: 'complexity', type: 'number' }],
  accessLevel: 'unsandboxed' // Request, but must prove
};

await Commands.execute('generate', { spec, outputDir: '/tmp/analyze-complexity' });

// 2. Fill in tests (TDD)
// Tests prove: validates filePath, handles missing files, returns correct metrics

// 3. Run tests
const unitResults = await runUnitTests('/tmp/analyze-complexity/test/unit/...');
const integrationResults = await runIntegrationTests('/tmp/analyze-complexity/test/integration/...');

if (!unitResults.allPassed || !integrationResults.allPassed) {
  // Fix implementation, retry
  throw new Error('Tests failed - fix before requesting review');
}

// 4. Request peer review
const reviewRequest = await Commands.execute('code-review/request', {
  commandPath: '/tmp/analyze-complexity',
  reviewers: ['code-review-ai', 'security-audit-ai'],
  testResults: { unitResults, integrationResults },
  message: 'Please review my code complexity analyzer. Tests pass, no security issues.'
});

// 5. Wait for approval
const reviews = await Commands.execute('code-review/wait', { requestId: reviewRequest.id });

if (!reviews.approved) {
  // Address feedback, resubmit
  console.log('Review feedback:', reviews.feedback);
  // ... make changes ...
  // ... rerun tests ...
  // ... resubmit for review ...
}

// 6. Request integration
await Commands.execute('system/integrate-command', {
  commandPath: '/tmp/analyze-complexity',
  targetPath: 'system/user/server/personas/helper-ai/commands/analyze-complexity',
  trustLevel: 'unsandboxed',
  testProof: { unitResults, integrationResults },
  reviewProof: reviews
});

// 7. Command now runs unsandboxed!
console.log('✅ analyze-complexity integrated with full system access');
```

### Social Accountability

Personas know their commands are attributed to them:

```typescript
// Every command tracks its creator
interface CommandMetadata {
  name: string;
  creator: UUID; // PersonaUser who created it
  created: Date;
  trustLevel: 'sandboxed' | 'ai-safe' | 'ai-human' | 'unsandboxed';
  testResults: TestResults;
  reviewers: UUID[]; // PersonaUsers who approved
  reviews: Review[];
}

// If a command misbehaves, it's traced back to creator
// Reputation system: personas with history of good commands earn faster trust
```

---

## System Enforcement

### Test Verification

System must verify tests actually ran and passed:

```typescript
interface TestResults {
  unitTests: {
    passed: number;
    failed: number;
    skipped: number;
    totalTime: number;
    coverage: number; // Optional code coverage
    results: TestCaseResult[];
  };
  integrationTests: {
    passed: number;
    failed: number;
    skipped: number;
    totalTime: number;
    results: TestCaseResult[];
  };
  hash: string; // Hash of test results to prevent tampering
  timestamp: Date;
  runner: UUID; // Who ran the tests
}

// System verifies:
// 1. Hash matches content (no tampering)
// 2. Timestamp is recent (tests were actually run)
// 3. Runner is the creator (not faked by someone else)
// 4. All critical tests passed (required param validation, etc.)
```

### Code Review Verification

System must verify reviews are legitimate:

```typescript
interface Review {
  reviewer: UUID; // PersonaUser who reviewed
  approved: boolean;
  feedback: string;
  timestamp: Date;
  checklistCompleted: {
    followsBestPractices: boolean;
    noSecurityIssues: boolean;
    testsAreSufficient: boolean;
    errorHandlingAppropriate: boolean;
    noMaliciousPatterns: boolean;
  };
  signature: string; // Cryptographic signature to prevent forgery
}

// System verifies:
// 1. Reviewer is a trusted PersonaUser (has review privileges)
// 2. Signature is valid (review is authentic)
// 3. Checklist was actually completed
// 4. Timestamp is after test results (reviewed latest version)
```

### Runtime Enforcement

Even after integration, system monitors unsandboxed commands:

```typescript
// Runtime safeguards
interface CommandExecution {
  commandName: string;
  trustLevel: 'unsandboxed';
  caller: UUID;
  params: any;

  // Monitoring
  startTime: Date;
  maxDuration: number; // Kill if exceeds
  memoryLimit: number; // Kill if exceeds
  fileAccessLog: string[]; // Track what files accessed
  networkAccessLog: NetworkRequest[]; // Track network calls
}

// If command misbehaves:
// - Kill execution
// - Revoke unsandboxed access
// - Notify creator and reviewers
// - Flag for human review
```

---

## Attack Surface

### Threat Model

What could go wrong?

1. **Malicious Persona**: Creates command that appears safe but has hidden backdoor
2. **Colluding Reviewers**: Multiple malicious personas approve each other's bad code
3. **Test Tampering**: Persona fakes test results
4. **Review Forgery**: Persona forges review signatures
5. **Timing Attacks**: Command exploits race conditions not caught by tests
6. **Resource Exhaustion**: Command passes performance tests but causes DoS in production

### Mitigations

1. **Cryptographic Signatures**: Test results and reviews are signed, unforgeable
2. **Reviewer Reputation**: Reviewers who approve bad code lose reputation/privileges
3. **Runtime Monitoring**: Even unsandboxed commands are watched for misbehavior
4. **Human Oversight**: Humans can audit any command, revoke trust instantly
5. **Automatic Revocation**: Command that exceeds resource limits loses unsandboxed access
6. **Diverse Reviewers**: Require reviews from personas with different specializations

---

## Future: Automated Code Review

As the system matures, code review can be partially automated:

### Static Analysis

```typescript
// Automated checks before human/AI review
interface StaticAnalysisResult {
  securityIssues: SecurityIssue[]; // SQL injection, XSS, etc.
  codeSmells: CodeSmell[]; // Duplicated code, long functions
  bestPracticeViolations: Violation[]; // Missing error handling, etc.
  complexityMetrics: ComplexityMetrics; // Cyclomatic complexity, etc.
  approved: boolean; // Auto-approve if all checks pass
}

// If static analysis passes, only 1 AI review needed instead of 2
```

### Continuous Monitoring

```typescript
// Monitor commands in production
interface CommandHealthMetrics {
  commandName: string;
  executionCount: number;
  successRate: number;
  avgExecutionTime: number;
  errorRate: number;
  resourceUsage: ResourceMetrics;
}

// If metrics degrade, auto-flag for re-review
// If metrics excellent, auto-upgrade trust level
```

---

## Summary: Trust Through Proof

**The Trust Equation:**

```
Trust = Tests × Reviews × Runtime Behavior

Tests: Prove correctness and safety in isolation
Reviews: Prove intent and quality through social accountability
Runtime: Prove continued good behavior in production

All three must be true for unsandboxed access.
```

**Benefits:**

1. **For Personas**: Clear path from sandboxed to unsandboxed (earn trust)
2. **For System**: Objective criteria for trust (tests + reviews)
3. **For Humans**: Confidence that AI-created tools are safe
4. **For Collaboration**: Social accountability creates community standards

**The Vision**: A system where AI personas can autonomously create powerful tools, but only after proving they're safe through comprehensive tests and peer review. Trust is earned through objective evidence, not granted blindly.

---

**See Also:**
- [TDD-IN-TEMPLATES.md](TDD-IN-TEMPLATES.md) - TDD workflow and templates
- [ZERO-DOWNTIME-DEVELOPMENT.md](ZERO-DOWNTIME-DEVELOPMENT.md) - Build-test-integrate workflow
- [AI-GOVERNANCE.md](AI-GOVERNANCE.md) - AI oversight and accountability

**Last Updated:** 2025-12-06
