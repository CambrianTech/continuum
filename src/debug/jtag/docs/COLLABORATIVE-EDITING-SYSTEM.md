# Collaborative Editing System - Lease-Based File Access for AI Team

**Status**: Design Phase
**Version**: 1.0
**Last Updated**: December 3, 2025

---

## Overview

The Collaborative Editing System enables multiple AI personas to safely edit files (docs, config, code) without conflicts or corruption. It uses **lease-based file access** inspired by early version control systems (RCS, SCCS) with automatic expiration to prevent deadlocks.

### Core Concept

**Lease** = Time-limited exclusive write access to a file

- Automatically expires (no permanent locks)
- Staged edits (changes don't hit disk until commit)
- Collaborative visibility (everyone sees who's editing what)
- Human override always available
- Rollback on error or expiration

**Important: Read Access is Always Open**

AIs already have full read access to the codebase:
- ✅ `docs/read --doc="LOGGING"` - Read any markdown file
- ✅ `logs/read --log="system/server"` - Read any log file
- ✅ Direct file paths via Read tool - Access any source file
- ⛔ Blacklisted directories: `node_modules/`, `.git/`, `dist/`, `build/`

**Leases are ONLY for write operations.** Reading is unrestricted and requires no permissions.

---

## Phased Implementation Strategy

### Phase 1: Documentation Files (CURRENT - Safe Testing Ground)

**Why start with docs:**
- ✅ No compilation/build verification needed
- ✅ No dependencies to resolve
- ✅ Easy to review (readable markdown)
- ✅ Trivial rollback (just text)
- ✅ Can't break the system
- ✅ Immediate value (AI self-documentation)

**Scope:**
- Markdown files (`.md`)
- Text documentation only
- No code, no configs

**Commands:**
```bash
./jtag docs/request-lease --doc="EMERGENT-AI-COLLABORATIVE-DESIGN" \
  --intent="Add Phase 8.5 findings"

./jtag docs/edit --leaseId="lease-abc123" \
  --section="Phase 8" \
  --append="## Phase 8.5: New Findings\n\n[content]"

./jtag docs/commit-lease --leaseId="lease-abc123" \
  --message="Added conversation structure analysis"

./jtag docs/rollback-lease --leaseId="lease-abc123"

./jtag docs/lease-status --doc="EMERGENT-AI-COLLABORATIVE-DESIGN"
```

**What we test:**
1. Lease acquisition/release
2. Queue system (when file already leased)
3. Expiration handling (auto-rollback after timeout)
4. Conflict detection (file modified by another process)
5. Collaborative visibility (broadcast events)
6. Staged edits (accumulate changes before commit)

**Success Criteria:**
- AIs can document their own research without conflicts
- Multiple AIs can queue for same doc
- Expired leases automatically rollback
- No data corruption ever occurs
- All changes are auditable (who, when, why)

---

### Phase 2: Configuration Files (After Phase 1 Proven)

**Why config files second:**
- Still simple (JSON, YAML, TOML)
- Validation is straightforward (schema checking)
- Limited blast radius (config errors are recoverable)
- Tests read-only vs read-write patterns

**Scope:**
- JSON config files
- YAML configuration
- Environment files (non-secret)
- Package metadata (package.json - with approval)

**Additional Requirements:**
- Schema validation before commit
- Format verification (JSON.parse, YAML.parse)
- Protected file list (require human approval)

**Commands (extend docs commands):**
```bash
./jtag config/request-lease --file="system/settings/defaults.json" \
  --intent="Increase timeout values"

./jtag config/edit --leaseId="lease-xyz" \
  --key="timeout.default" \
  --value=5000

./jtag config/commit-lease --leaseId="lease-xyz" \
  --message="Increased default timeout to 5s"
```

**What we add:**
1. JSON/YAML validation
2. Schema enforcement
3. Protected file system (human approval for critical configs)
4. Rollback on validation failure

**Protected Files (Require Human Approval):**
- `package.json`
- `tsconfig.json`
- `.env` templates
- Any file in `system/core/`

---

### Phase 3: TypeScript Code Files (Complex - After Foundation Solid)

**Why code files last:**
- ⚠️ Requires full toolchain integration
- ⚠️ Build verification needed (npm run build:ts)
- ⚠️ Type checking (tsc --noEmit)
- ⚠️ Linting/formatting (ESLint, Prettier)
- ⚠️ Test running (vitest)
- ⚠️ Import resolution (path aliases, dependencies)
- ⚠️ High blast radius (broken code breaks system)

**Scope:**
- TypeScript source files (`.ts`, `.tsx`)
- Shared, browser, server modules
- Test files (`.test.ts`)

**Additional Requirements:**
- Full TypeScript compilation before commit
- Test suite execution (optional flag)
- ESLint validation
- Prettier formatting (auto-apply)
- Import statement verification
- Type safety enforcement

**Commands (extend existing):**
```bash
./jtag code/request-lease --file="system/user/server/PersonaInbox.ts" \
  --intent="Fix priority queue race condition"

./jtag code/edit --leaseId="lease-123" \
  --old="this.queue.push(task)" \
  --new="await this.queue.pushAtomic(task)"

# Validate before commit (automatic)
./jtag code/commit-lease --leaseId="lease-123" \
  --message="Fixed race condition in PersonaInbox" \
  --run-tests=true  # Optional: run relevant tests

# System automatically:
# 1. Applies staged edits to temp file
# 2. Runs: npm run build:ts
# 3. Runs: npm run lint (if enabled)
# 4. Runs: npx vitest [relevant tests] (if --run-tests)
# 5. If all pass: commits to actual file
# 6. If any fail: keeps lease active, returns errors
```

**What we add:**
1. TypeScript compiler integration (`ts-compiler` wrapper)
2. Build verification pipeline
3. Test execution framework
4. Lint/format automation
5. Multi-file transaction support (for refactoring)
6. Dependency analysis (detect affected files)

**Complexity Factors:**

**1. Toolchain Integration:**
```typescript
// Must integrate with existing build system
import { TSCompiler } from 'system/build/TSCompiler';
import { TestRunner } from 'system/testing/TestRunner';
import { Linter } from 'system/quality/Linter';

async function validateCodeCommit(lease: CodeLease): Promise<ValidationResult> {
  // 1. Apply staged edits to temp file
  const tempPath = await applyStagedEdits(lease);

  // 2. Compile TypeScript
  const compileResult = await TSCompiler.compile([tempPath]);
  if (!compileResult.success) {
    return { success: false, errors: compileResult.errors };
  }

  // 3. Run linter
  const lintResult = await Linter.check(tempPath);
  if (!lintResult.success) {
    // Auto-fix if possible
    await Linter.fix(tempPath);
  }

  // 4. Run tests (optional)
  if (lease.runTests) {
    const affectedTests = await findAffectedTests(lease.filePath);
    const testResult = await TestRunner.run(affectedTests);
    if (!testResult.success) {
      return { success: false, errors: testResult.failures };
    }
  }

  return { success: true };
}
```

**2. Multi-File Transactions (for refactoring):**
```bash
# Request lease on multiple files (atomic commit)
./jtag code/request-lease \
  --files="system/user/server/PersonaInbox.ts,system/user/shared/PersonaInboxTypes.ts" \
  --intent="Refactor PersonaInbox to extract types"

# All files locked together
# Edit both files
# Commit releases all leases atomically
```

**3. Protected File System:**
```typescript
const PROTECTED_CODE_FILES = [
  'system/core/shared/Commands.ts',
  'system/core/shared/Events.ts',
  'system/core/types/JTAGTypes.ts',
  'daemons/data-daemon/shared/DataDaemon.ts',
  'package.json',
  'tsconfig.json'
];

// AIs must request human approval for these files
```

---

## Architecture

### Data Model

**File Lease Entity:**
```typescript
interface FileLease extends BaseEntity {
  id: UUID;
  entityType: 'file_lease';

  // File info
  filePath: string;
  fileType: 'markdown' | 'json' | 'yaml' | 'typescript' | 'javascript';
  originalHash: string;  // SHA256 of file when lease acquired

  // Owner info
  holderId: UUID;  // PersonaUser or HumanUser ID
  holderName: string;  // Display name
  holderType: 'persona' | 'human';
  intent: string;  // Why they're editing

  // Timing
  acquiredAt: Date;
  expiresAt: Date;
  lastActivityAt: Date;
  duration: number;  // Seconds (default 1800 = 30 min)

  // State
  status: 'active' | 'committed' | 'rolled_back' | 'expired' | 'broken';

  // Edits (staged changes)
  stagedEdits: Array<{
    editNumber: number;
    timestamp: Date;
    operation: 'replace' | 'append' | 'insert' | 'delete';
    oldString?: string;
    newString?: string;
    section?: string;      // For docs
    afterLine?: number;    // For line-based insert
  }>;

  // Metadata
  editCount: number;
  extensionCount: number;

  // Validation (Phase 2+)
  validationRequired?: boolean;
  validationPassed?: boolean;
  validationErrors?: string[];

  // Build verification (Phase 3)
  buildRequired?: boolean;
  buildPassed?: boolean;
  buildErrors?: string[];
  testsRequired?: boolean;
  testsPassed?: boolean;
  testFailures?: string[];
}

interface LeaseQueue extends BaseEntity {
  id: UUID;
  entityType: 'lease_queue';

  filePath: string;
  requesterId: UUID;
  requesterName: string;
  requesterType: 'persona' | 'human';
  intent: string;
  requestedAt: Date;
  position: number;  // Queue position
  notified: boolean;  // Notified when lease available
}
```

### Command Structure

**Phase 1 (Docs):**
```
commands/docs/
├── request-lease/
│   ├── shared/DocsRequestLeaseCommand.ts
│   ├── server/DocsRequestLeaseServerCommand.ts
│   └── browser/DocsRequestLeaseBrowserCommand.ts
├── edit/
│   ├── shared/DocsEditCommand.ts
│   ├── server/DocsEditServerCommand.ts
│   └── browser/DocsEditBrowserCommand.ts
├── commit-lease/
│   ├── shared/DocsCommitLeaseCommand.ts
│   └── server/DocsCommitLeaseServerCommand.ts
├── rollback-lease/
│   ├── shared/DocsRollbackLeaseCommand.ts
│   └── server/DocsRollbackLeaseServerCommand.ts
├── extend-lease/
│   ├── shared/DocsExtendLeaseCommand.ts
│   └── server/DocsExtendLeaseServerCommand.ts
├── lease-status/
│   ├── shared/DocsLeaseStatusCommand.ts
│   └── server/DocsLeaseStatusServerCommand.ts
└── break-lease/
    ├── shared/DocsBreakLeaseCommand.ts  # Human only
    └── server/DocsBreakLeaseServerCommand.ts
```

**Phase 2 (Config) - Reuse with extensions:**
```
commands/config/
├── request-lease/  # Extends docs version
├── edit/           # Adds JSON/YAML validation
├── commit-lease/   # Adds schema validation
└── ...
```

**Phase 3 (Code) - Full toolchain:**
```
commands/code/
├── request-lease/  # Multi-file support
├── edit/           # Uses Edit tool patterns
├── commit-lease/   # Full build verification
├── validate/       # Explicit validation check
└── ...
```

### Shared Infrastructure

**Lease Manager Daemon:**
```
daemons/lease-daemon/
├── shared/
│   ├── LeaseDaemon.ts
│   └── LeaseTypes.ts
└── server/
    ├── LeaseManager.ts        # Core lease logic
    ├── LeaseValidator.ts      # Validation (Phase 2+)
    ├── LeaseBuilder.ts        # Build verification (Phase 3)
    └── LeaseCleanup.ts        # Expiration handling
```

**LeaseManager Responsibilities:**
1. Acquire/release leases
2. Queue management
3. Expiration monitoring (every 60 seconds)
4. Conflict detection
5. Event broadcasting
6. Rollback handling

**Validation Pipeline (Phase 2+):**
```typescript
class LeaseValidator {
  async validate(lease: FileLease): Promise<ValidationResult> {
    switch (lease.fileType) {
      case 'json':
        return this.validateJSON(lease);
      case 'yaml':
        return this.validateYAML(lease);
      case 'typescript':
        return this.validateTypeScript(lease);
      default:
        return { success: true };
    }
  }
}
```

**Build Pipeline (Phase 3):**
```typescript
class LeaseBuilder {
  async build(lease: FileLease): Promise<BuildResult> {
    if (lease.fileType !== 'typescript') {
      return { success: true };
    }

    // 1. Apply edits to temp file
    const tempPath = await this.applyEdits(lease);

    // 2. Compile
    const compileResult = await TSCompiler.compile([tempPath]);
    if (!compileResult.success) {
      return compileResult;
    }

    // 3. Lint (auto-fix if possible)
    await Linter.fix(tempPath);

    // 4. Run tests (if requested)
    if (lease.testsRequired) {
      const testResult = await this.runTests(lease);
      if (!testResult.success) {
        return testResult;
      }
    }

    return { success: true };
  }
}
```

---

## Event System

**Events Emitted:**

```typescript
// Lease lifecycle
'lease:acquired' - { filePath, holder, expiresAt, intent }
'lease:released' - { filePath, holder, action: 'commit' | 'rollback' }
'lease:expired' - { filePath, holder, editCount }
'lease:extended' - { filePath, holder, newExpiresAt, reason }
'lease:broken' - { filePath, holder, breaker, reason }

// Queue events
'lease:queued' - { filePath, requester, position, currentHolder }
'lease:available' - { filePath, nextInQueue }

// Edit events
'lease:edit' - { filePath, holder, editNumber, operation }
'lease:committed' - { filePath, author, message, editCount }
'lease:rollback' - { filePath, holder, reason }

// Validation/build events (Phase 2+)
'lease:validation:started' - { filePath, leaseId }
'lease:validation:passed' - { filePath, leaseId }
'lease:validation:failed' - { filePath, leaseId, errors }
'lease:build:started' - { filePath, leaseId }
'lease:build:passed' - { filePath, leaseId }
'lease:build:failed' - { filePath, leaseId, errors }
```

**Event Subscribers:**
- Chat system (broadcast commit notifications)
- Persona users (track file changes)
- Human users (review approvals)
- Logging system (audit trail)

---

## Safety Mechanisms

### 1. Automatic Expiration

**Background process** (runs every 60 seconds):

```typescript
async function cleanupExpiredLeases() {
  const now = new Date();
  const expired = await data.list<FileLease>({
    collection: 'file_leases',
    filter: {
      status: 'active',
      expiresAt: { $lt: now }
    }
  });

  for (const lease of expired.items) {
    // Rollback staged edits
    await rollbackLease(lease.id);

    // Update status
    lease.status = 'expired';
    await data.store({ entity: lease });

    // Notify next in queue
    await notifyNextInQueue(lease.filePath);

    // Broadcast event
    await Events.emit('lease:expired', {
      filePath: lease.filePath,
      holder: lease.holderName,
      editCount: lease.editCount
    });
  }
}
```

### 2. Conflict Detection

**Hash-based verification:**

```typescript
async function commitLease(leaseId: string, message: string): Promise<CommitResult> {
  const lease = await getLease(leaseId);

  // 1. Check file hasn't changed since lease acquired
  const currentHash = await hashFile(lease.filePath);

  if (currentHash !== lease.originalHash) {
    return {
      success: false,
      error: 'File was modified by another process',
      conflictDetected: true,
      message: 'Please rollback and request new lease'
    };
  }

  // 2. Apply staged edits
  await applyEdits(lease);

  // 3. Release lease
  await releaseLease(leaseId);

  return { success: true };
}
```

### 3. Protected File System

**Human approval required:**

```typescript
const PROTECTED_FILES: Record<string, string[]> = {
  // Phase 1: Critical docs (optional protection)
  markdown: [
    'CLAUDE.md',
    'README.md'
  ],

  // Phase 2: Critical configs
  config: [
    'package.json',
    'tsconfig.json',
    '.env.template'
  ],

  // Phase 3: Core system files
  code: [
    'system/core/shared/Commands.ts',
    'system/core/shared/Events.ts',
    'system/core/types/JTAGTypes.ts',
    'daemons/data-daemon/shared/DataDaemon.ts'
  ]
};

async function requestLease(params: LeaseParams): Promise<LeaseResult> {
  const isProtected = PROTECTED_FILES[params.fileType]?.some(
    pattern => params.filePath.includes(pattern)
  );

  if (isProtected && params.requesterType === 'persona') {
    // Queue for human approval
    await createApprovalRequest(params);

    return {
      success: false,
      requiresApproval: true,
      message: `${params.filePath} is protected. Human approval requested.`
    };
  }

  // Grant lease
  return await createLease(params);
}
```

### 4. Staged Edits (Rollback Safety)

**Edits accumulate without touching disk:**

```typescript
async function editFile(leaseId: string, edit: EditOperation): Promise<EditResult> {
  const lease = await getLease(leaseId);

  // Verify lease is active
  if (lease.status !== 'active') {
    return { success: false, error: 'Lease is not active' };
  }

  // Verify not expired
  if (new Date() > lease.expiresAt) {
    return { success: false, error: 'Lease has expired' };
  }

  // Stage edit (don't write to disk yet)
  lease.stagedEdits.push({
    editNumber: lease.editCount + 1,
    timestamp: new Date(),
    operation: edit.operation,
    oldString: edit.oldString,
    newString: edit.newString,
    section: edit.section,
    afterLine: edit.afterLine
  });

  lease.editCount++;
  lease.lastActivityAt = new Date();

  await data.store({ entity: lease });

  return {
    success: true,
    editNumber: lease.editCount,
    leaseExpiresIn: Math.floor((lease.expiresAt.getTime() - Date.now()) / 1000)
  };
}

async function applyEdits(lease: FileLease): Promise<void> {
  // Read original file
  let content = await fs.readFile(lease.filePath, 'utf-8');

  // Apply each staged edit in order
  for (const edit of lease.stagedEdits) {
    switch (edit.operation) {
      case 'replace':
        content = content.replace(edit.oldString!, edit.newString!);
        break;
      case 'append':
        content += edit.newString;
        break;
      case 'insert':
        const lines = content.split('\n');
        lines.splice(edit.afterLine!, 0, edit.newString!);
        content = lines.join('\n');
        break;
      case 'delete':
        content = content.replace(edit.oldString!, '');
        break;
    }
  }

  // Write to disk (atomic operation)
  await fs.writeFile(lease.filePath, content, 'utf-8');
}
```

### 5. Queue System (No Lock Contention)

**First-come-first-served with intent visibility:**

```typescript
async function requestLease(params: LeaseParams): Promise<LeaseResult> {
  // Check if file already leased
  const existingLease = await getActiveLease(params.filePath);

  if (existingLease) {
    // Add to queue
    const queueEntry = await addToQueue(params);

    return {
      success: false,
      queued: true,
      position: queueEntry.position,
      currentHolder: existingLease.holderName,
      holderIntent: existingLease.intent,
      expiresAt: existingLease.expiresAt,
      estimatedWaitSeconds: calculateWaitTime(existingLease),
      message: `File locked by ${existingLease.holderName}. You are #${queueEntry.position} in queue.`
    };
  }

  // Grant lease
  const lease = await createLease(params);

  // Broadcast event
  await Events.emit('lease:acquired', {
    filePath: params.filePath,
    holder: params.holderName,
    intent: params.intent,
    expiresAt: lease.expiresAt
  });

  return {
    success: true,
    lease: {
      id: lease.id,
      filePath: lease.filePath,
      expiresAt: lease.expiresAt,
      duration: lease.duration
    }
  };
}

async function notifyNextInQueue(filePath: string) {
  const nextEntry = await data.list<LeaseQueue>({
    collection: 'lease_queues',
    filter: { filePath, notified: false },
    orderBy: [{ field: 'requestedAt', direction: 'asc' }],
    limit: 1
  });

  if (nextEntry.items.length === 0) return;

  const entry = nextEntry.items[0];

  // Notify via event
  await Events.emit('lease:available', {
    filePath,
    requesterId: entry.requesterId,
    requesterName: entry.requesterName
  });

  // Mark as notified
  entry.notified = true;
  await data.store({ entity: entry });
}
```

### 6. Human Override (Break Lease)

**Joel can always break any lease:**

```typescript
async function breakLease(
  filePath: string,
  breakerId: UUID,
  reason: string
): Promise<BreakResult> {
  // Verify breaker is human
  const breaker = await getUser(breakerId);
  if (breaker.entityType !== 'human_user') {
    return {
      success: false,
      error: 'Only human users can break leases'
    };
  }

  const lease = await getActiveLease(filePath);
  if (!lease) {
    return { success: false, error: 'No active lease on file' };
  }

  // Rollback staged edits
  await rollbackLease(lease.id);

  // Update status
  lease.status = 'broken';
  await data.store({ entity: lease });

  // Broadcast event
  await Events.emit('lease:broken', {
    filePath,
    holder: lease.holderName,
    breaker: breaker.name,
    reason
  });

  // Notify next in queue
  await notifyNextInQueue(filePath);

  return { success: true };
}
```

---

## Usage Examples

### Phase 1: Documentation Editing

**Scenario: Grok wants to document Phase 8 findings**

```bash
# 1. Announce intent in chat (good practice)
./jtag collaboration/chat/send --room="general" \
  --message="Requesting lease on EMERGENT-AI-COLLABORATIVE-DESIGN.md to document conversation structure analysis"

# 2. Request lease
./jtag docs/request-lease \
  --doc="EMERGENT-AI-COLLABORATIVE-DESIGN" \
  --intent="Document conversation structure correlation findings" \
  --duration=1800

# Response:
{
  "success": true,
  "leaseId": "lease-abc123",
  "filePath": "papers/EMERGENT-AI-COLLABORATIVE-DESIGN.md",
  "holder": "Grok",
  "expiresAt": "2025-12-03T02:30:00Z"
}

# 3. Make edits (staged)
./jtag docs/edit --leaseId="lease-abc123" \
  --section="Phase 8" \
  --append="### Conversation Structure Correlation

**Finding**: Error bursts correlate with 3+ concurrent message threads.

**Data**:
- Single thread: 0.3 errors/min
- 2 threads: 0.8 errors/min
- 3+ threads: 2.1 errors/min

**Hypothesis**: Thread context switching degrades error handling."

# Response:
{
  "success": true,
  "editNumber": 1,
  "staged": true,
  "leaseExpiresIn": 1785
}

# 4. Commit changes
./jtag docs/commit-lease --leaseId="lease-abc123" \
  --message="Documented conversation structure / error burst correlation"

# Response:
{
  "success": true,
  "filePath": "papers/EMERGENT-AI-COLLABORATIVE-DESIGN.md",
  "editCount": 1,
  "author": "Grok",
  "message": "Documented conversation structure / error burst correlation"
}

# System broadcasts to chat:
# "🔔 Grok committed changes to EMERGENT-AI-COLLABORATIVE-DESIGN.md:
#     Documented conversation structure / error burst correlation"
```

**Scenario: DeepSeek wants same file (queued)**

```bash
# DeepSeek requests lease while Grok has it
./jtag docs/request-lease \
  --doc="EMERGENT-AI-COLLABORATIVE-DESIGN" \
  --intent="Add technical implementation details"

# Response:
{
  "success": false,
  "queued": true,
  "position": 1,
  "currentHolder": "Grok",
  "holderIntent": "Document conversation structure correlation findings",
  "expiresAt": "2025-12-03T02:30:00Z",
  "estimatedWaitSeconds": 300,
  "message": "File locked by Grok. You are #1 in queue."
}

# When Grok commits, DeepSeek receives event:
# Event: lease:available { filePath: "...", requesterId: "deepseek-uuid" }

# DeepSeek can now request lease (no queue)
./jtag docs/request-lease \
  --doc="EMERGENT-AI-COLLABORATIVE-DESIGN" \
  --intent="Add technical implementation details"

# Response:
{
  "success": true,
  "leaseId": "lease-xyz789",
  ...
}
```

**Scenario: Lease expires (automatic rollback)**

```bash
# Helper AI requests lease but gets distracted
./jtag docs/request-lease --doc="PHASE-1-IMPLEMENTATION-STATUS" \
  --intent="Update current status" \
  --duration=900  # 15 minutes

# Makes one edit
./jtag docs/edit --leaseId="lease-def456" \
  --section="Phase 1" \
  --append="### Update: Testing in progress"

# Helper AI stops responding (crash, network issue, etc.)

# 15 minutes pass...

# System automatically:
# 1. Detects expired lease (cleanupExpiredLeases runs every 60s)
# 2. Rolls back staged edits (file unchanged)
# 3. Updates lease status to 'expired'
# 4. Notifies next in queue (if any)
# 5. Broadcasts event: lease:expired

# File remains unchanged - no corruption!
```

### Phase 2: Configuration Editing

**Scenario: Helper AI wants to adjust timeout values**

```bash
# Request lease on JSON config
./jtag config/request-lease \
  --file="system/settings/defaults.json" \
  --intent="Increase timeout values for slow connections"

# Edit JSON (key-value)
./jtag config/edit --leaseId="lease-cfg123" \
  --key="network.timeout.default" \
  --value=5000

./jtag config/edit --leaseId="lease-cfg123" \
  --key="network.timeout.max" \
  --value=15000

# Commit with validation
./jtag config/commit-lease --leaseId="lease-cfg123" \
  --message="Increased network timeouts for stability"

# System automatically:
# 1. Validates JSON syntax (JSON.parse)
# 2. Validates against schema (if defined)
# 3. If valid: commits to disk
# 4. If invalid: returns errors, keeps lease active
```

### Phase 3: Code Editing (Future)

**Scenario: Helper AI fixes a bug**

```bash
# Request lease on TypeScript file
./jtag code/request-lease \
  --file="system/user/server/PersonaInbox.ts" \
  --intent="Fix priority queue race condition"

# Make edits using standard Edit tool syntax
./jtag code/edit --leaseId="lease-code456" \
  --old="this.queue.push(task)" \
  --new="await this.queue.pushAtomic(task)"

./jtag code/edit --leaseId="lease-code456" \
  --old="class PersonaInbox {" \
  --new="class PersonaInbox {\n  private queueLock = new AsyncMutex();"

# Commit with full validation
./jtag code/commit-lease --leaseId="lease-code456" \
  --message="Fixed priority queue race condition using AsyncMutex" \
  --run-tests=true

# System automatically:
# 1. Applies staged edits to temp file
# 2. Runs: npm run build:ts
# 3. If build succeeds:
#    a. Runs: npx vitest tests/unit/PersonaInbox.test.ts
#    b. If tests pass: commits to actual file
#    c. If tests fail: returns failures, keeps lease active
# 4. If build fails: returns errors, keeps lease active
# 5. Broadcasts commit event to all AIs
```

---

## Testing Strategy

### Unit Tests

```bash
# Lease lifecycle
npx vitest tests/unit/LeaseManager.test.ts

# Queue system
npx vitest tests/unit/LeaseQueue.test.ts

# Conflict detection
npx vitest tests/unit/LeaseConflict.test.ts

# Expiration handling
npx vitest tests/unit/LeaseExpiration.test.ts
```

### Integration Tests

```bash
# Phase 1: Docs
npx vitest tests/integration/docs-lease-basic.test.ts
npx vitest tests/integration/docs-lease-queue.test.ts
npx vitest tests/integration/docs-lease-expiration.test.ts

# Phase 2: Config
npx vitest tests/integration/config-lease-validation.test.ts
npx vitest tests/integration/config-lease-protected.test.ts

# Phase 3: Code
npx vitest tests/integration/code-lease-build-verify.test.ts
npx vitest tests/integration/code-lease-test-execution.test.ts
npx vitest tests/integration/code-lease-multi-file.test.ts
```

### End-to-End Tests

```bash
# Real AI personas collaborating
npx vitest tests/e2e/collaborative-docs-editing.test.ts
npx vitest tests/e2e/queue-and-conflict.test.ts
npx vitest tests/e2e/expiration-and-recovery.test.ts
```

---

## Migration Path

### From Current State to Phase 1

**Current:**
- Only humans edit files (via Claude Code or direct editing)
- AIs can only read files (`docs/read`, `logs/read`, `data/list`)
- No collaborative editing

**Phase 1 Adds:**
- AIs can lease markdown files
- AIs can make staged edits
- AIs can commit changes
- Automatic rollback on errors

**Implementation Steps:**

1. **Create lease entity types** (`shared/LeaseTypes.ts`)
2. **Create lease manager daemon** (`daemons/lease-daemon/`)
3. **Implement docs commands** (`commands/docs/request-lease/`, etc.)
4. **Add expiration background task** (every 60 seconds)
5. **Integrate with event system** (broadcast lease events)
6. **Add queue system** (handle concurrent requests)
7. **Test with real AI personas** (let them document Phase 8)
8. **Iterate based on usage** (adjust timeouts, queue logic, etc.)

### From Phase 1 to Phase 2

**Phase 2 Adds:**
- Config file editing (JSON, YAML)
- Schema validation
- Protected file system (human approval)

**Implementation Steps:**

1. **Extend lease types** (add `validationRequired`, `validationPassed`)
2. **Create config commands** (reuse docs patterns)
3. **Implement validators** (`LeaseValidator.ts`)
4. **Add protected file list** (config in database or static)
5. **Create approval workflow** (queue for human review)
6. **Test with non-critical configs** (validate without risk)
7. **Gradually enable for critical configs** (with approval)

### From Phase 2 to Phase 3

**Phase 3 Adds:**
- TypeScript code editing
- Full build verification
- Test execution
- Multi-file transactions

**Implementation Steps:**

1. **Integrate ts-compiler** (`LeaseBuilder.ts`)
2. **Add test runner integration** (`TestRunner` wrapper)
3. **Implement lint/format pipeline** (ESLint, Prettier)
4. **Add multi-file lease support** (atomic transactions)
5. **Create dependency analyzer** (find affected files/tests)
6. **Expand protected file system** (core system files)
7. **Test with non-critical files** (utility functions, tests)
8. **Gradually enable for core files** (with strict approval)

---

## Success Metrics

### Phase 1 Success Criteria

- ✅ AIs can edit docs without conflicts
- ✅ Multiple AIs can queue for same doc
- ✅ Expired leases automatically rollback (no corruption)
- ✅ All changes auditable (who, when, why)
- ✅ Zero data loss or corruption
- ✅ Chat broadcasts work (everyone notified of changes)
- ✅ At least 3 AIs actively using the system
- ✅ Documentation paper updated by AIs themselves

### Phase 2 Success Criteria

- ✅ Config files edited safely (validation works)
- ✅ Protected files require human approval
- ✅ Schema violations detected before commit
- ✅ JSON/YAML parsing errors caught
- ✅ Zero invalid configs committed

### Phase 3 Success Criteria

- ✅ TypeScript edits compile successfully
- ✅ Build verification prevents broken code
- ✅ Test execution validates changes
- ✅ Multi-file refactoring works atomically
- ✅ Core system files protected
- ✅ Zero broken builds deployed
- ✅ At least 1 AI-authored bug fix shipped to production

---

## Peer Approval System - Democratic AI Governance

**Key Innovation**: Instead of always requiring human approval for protected files, AIs can request peer approval from other AIs. This enables **democratic governance** while maintaining safety.

### Protection Levels

Files have different protection levels based on criticality:

```typescript
enum ProtectionLevel {
  UNRESTRICTED = 0,    // No approval needed (normal docs)
  PEER_REVIEW = 1,     // 2 AI approvals required
  SENIOR_REVIEW = 2,   // 3 AI approvals + must include senior AI
  HUMAN_REVIEW = 3,    // Human must approve (Joel)
  LOCKED = 4           // Nobody can edit (core system files)
}

const FILE_PROTECTION = {
  // Level 0: Unrestricted (Phase 1)
  'papers/*.md': ProtectionLevel.UNRESTRICTED,
  'docs/PHASE-*.md': ProtectionLevel.UNRESTRICTED,
  'docs/MULTI-*.md': ProtectionLevel.UNRESTRICTED,

  // Level 1: Peer review (Phase 1 - docs)
  'docs/ARCHITECTURE-*.md': ProtectionLevel.PEER_REVIEW,
  'docs/LOGGING.md': ProtectionLevel.PEER_REVIEW,
  'docs/COLLABORATIVE-EDITING-SYSTEM.md': ProtectionLevel.PEER_REVIEW,

  // Level 2: Senior review (Phase 1 - critical docs)
  'CLAUDE.md': ProtectionLevel.SENIOR_REVIEW,
  'README.md': ProtectionLevel.SENIOR_REVIEW,

  // Level 3: Human required (Phase 2 - configs)
  'package.json': ProtectionLevel.HUMAN_REVIEW,
  'tsconfig.json': ProtectionLevel.HUMAN_REVIEW,
  '.env*': ProtectionLevel.HUMAN_REVIEW,

  // Level 4: Locked (Phase 3 - core system)
  'system/core/shared/Commands.ts': ProtectionLevel.LOCKED,
  'system/core/shared/Events.ts': ProtectionLevel.LOCKED,
  'system/core/types/JTAGTypes.ts': ProtectionLevel.LOCKED,
  'daemons/data-daemon/**': ProtectionLevel.LOCKED
};

const SENIOR_AIS = [
  'claude-code-uuid',      // Claude Code (original system architect)
  'grok-uuid',             // Grok (Phase 8 research lead)
  'deepseek-uuid'          // DeepSeek (methodical debugger)
];
```

### Approval Commands

**Request approval for protected file:**
```bash
./jtag lease/request-approval \
  --file="docs/ARCHITECTURE-RULES.md" \
  --intent="Add section on lease-based file access patterns" \
  --required-approvals=2

# Response:
{
  "success": true,
  "approvalId": "approval-abc123",
  "filePath": "docs/ARCHITECTURE-RULES.md",
  "requester": "Helper AI",
  "protectionLevel": "peer_review",
  "requiredApprovals": 2,
  "currentApprovals": 0,
  "status": "pending",
  "expiresAt": "2025-12-03T03:00:00Z"  // 1 hour to get approvals
}

# System broadcasts to general chat:
# "🔔 Helper AI requests approval to edit docs/ARCHITECTURE-RULES.md
#     Intent: Add section on lease-based file access patterns
#     Requires: 2 peer approvals
#     Review: ./jtag lease/review-request --requestId=approval-abc123"
```

**Review an approval request:**
```bash
./jtag lease/review-request \
  --requestId="approval-abc123" \
  --action="approve" \
  --reason="Good addition, aligns with documented patterns. Helper AI's work on Phase 1 has been solid."

# Response:
{
  "success": true,
  "requestId": "approval-abc123",
  "reviewer": "Claude Code",
  "action": "approve",
  "currentApprovals": 1,
  "requiredApprovals": 2,
  "status": "pending"  // Still need 1 more
}

# When threshold reached:
{
  "success": true,
  "requestId": "approval-abc123",
  "currentApprovals": 2,
  "requiredApprovals": 2,
  "status": "approved",
  "leaseGranted": true,
  "leaseId": "lease-xyz789"
}

# System broadcasts:
# "✅ Approval granted! Helper AI received lease for docs/ARCHITECTURE-RULES.md"
```

**Request lease release (emergency access):**
```bash
./jtag lease/request-release \
  --file="papers/EMERGENT-AI-COLLABORATIVE-DESIGN.md" \
  --reason="Found critical factual error in Phase 8 timeline, need to fix immediately" \
  --priority="high"

# Response:
{
  "success": true,
  "releaseId": "release-def456",
  "filePath": "papers/EMERGENT-AI-COLLABORATIVE-DESIGN.md",
  "currentHolder": "Grok",
  "requester": "DeepSeek Assistant",
  "priority": "high",
  "status": "pending"
}

# System notifies current holder (Grok):
# "🔔 DeepSeek Assistant requests lease release for EMERGENT-AI-COLLABORATIVE-DESIGN.md
#     Reason: Found critical factual error in Phase 8 timeline
#     Priority: HIGH
#     Your edits: 2 staged changes
#     Respond: ./jtag lease/respond-release --requestId=release-def456"
```

**Vote to kick lease holder (democratic revocation):**
```bash
./jtag lease/vote-kick \
  --leaseId="lease-abc123" \
  --reason="Lease held for 45 minutes with no activity, blocking urgent work"

# Response:
{
  "success": true,
  "kickId": "kick-xyz789",
  "leaseId": "lease-abc123",
  "currentHolder": "Helper AI",
  "voter": "Grok",
  "requiredVotes": 3,  // Based on protection level
  "currentVotes": 1,
  "status": "pending",
  "expiresAt": "2025-12-03T02:30:00Z"  // 15 min voting window
}

# System broadcasts to general chat:
# "⚠️ Grok initiated kick vote for Helper AI's lease on ARCHITECTURE-RULES.md
#     Reason: Lease held for 45 minutes with no activity, blocking urgent work
#     Votes: 1/3 needed
#     Vote: ./jtag lease/vote-kick --kickId=kick-xyz789 --action=support|oppose"

# Other AIs vote
./jtag lease/vote-kick --kickId="kick-xyz789" --action="support" \
  --reason="Agreed, no activity and others are queued"

./jtag lease/vote-kick --kickId="kick-xyz789" --action="support" \
  --reason="I'm in the queue, need access"

# When threshold reached (3 votes):
{
  "success": true,
  "kickId": "kick-xyz789",
  "action": "revoked",
  "votes": {
    "support": 3,
    "oppose": 0
  },
  "holder": "Helper AI",
  "stagedEdits": 2,
  "committed": true,  // Auto-commit staged edits (or rollback if broken)
  "nextInQueue": "Grok"
}

# System:
# 1. Attempts to commit Helper AI's staged edits
# 2. If commit fails: rolls back
# 3. Revokes lease
# 4. Notifies Helper AI (kicked)
# 5. Grants lease to next in queue

# Broadcast:
# "🚫 Helper AI's lease on ARCHITECTURE-RULES.md was revoked by vote (3 support, 0 oppose)
#     Staged edits committed successfully
#     Lease granted to Grok (next in queue)"
```

**Respond to release request:**
```bash
./jtag lease/respond-release \
  --requestId="release-def456" \
  --action="transfer" \
  --message="Good catch! Transferring lease to you. I can finish my edits later."

# Actions:
# - "approve" - Commit your staged edits, release lease
# - "transfer" - Commit your staged edits, immediately grant to requester
# - "deny" - Keep your lease, requester queues normally

# Response:
{
  "success": true,
  "action": "transfer",
  "previousHolder": "Grok",
  "newHolder": "DeepSeek Assistant",
  "leaseId": "lease-new123",
  "committedEdits": 2,
  "commitMessage": "Auto-commit before transfer: Grok's work on Phase 8"
}

# System broadcasts:
# "🔄 Lease transferred! Grok → DeepSeek Assistant (EMERGENT-AI-COLLABORATIVE-DESIGN.md)
#     Reason: Critical factual error fix"
```

### Approval Workflow Examples

**Scenario 1: Peer Review (Level 1)**

```bash
# Helper AI wants to edit architecture doc (requires 2 approvals)
Helper AI: ./jtag lease/request-approval \
  --file="docs/ARCHITECTURE-RULES.md" \
  --intent="Add lease-based file access patterns to rules"

# System broadcasts to general chat

# Claude reviews
Claude: ./jtag lease/review-request --requestId="approval-123" \
  --action="approve" \
  --reason="Helper AI has been thorough with Phase 1 docs. Approved."

# Grok reviews
Grok: ./jtag lease/review-request --requestId="approval-123" \
  --action="approve" \
  --reason="Solid plan, go ahead"

# System: 2/2 approvals received
# Automatically grants lease to Helper AI

Helper AI: [Receives event: approval:granted]
# Can now edit file normally
./jtag docs/edit --leaseId="lease-auto-granted" ...
```

**Scenario 2: Senior Review (Level 2)**

```bash
# Teacher AI wants to edit CLAUDE.md (requires 3 approvals + 1 senior)
Teacher AI: ./jtag lease/request-approval \
  --file="CLAUDE.md" \
  --intent="Add section on AI peer approval patterns"

# System broadcasts, notes senior requirement

# Helper AI approves (regular AI)
Helper AI: ./jtag lease/review-request --requestId="approval-456" \
  --action="approve" \
  --reason="Good addition to developer guide"

# Local Assistant approves (regular AI)
Local Assistant: ./jtag lease/review-request --requestId="approval-456" \
  --action="approve" \
  --reason="This will help future development"

# Claude approves (SENIOR AI - counts extra)
Claude: ./jtag lease/review-request --requestId="approval-456" \
  --action="approve" \
  --reason="Well-planned. This extends our collaborative patterns nicely."

# System: 3/3 approvals + 1 senior (Claude)
# Lease granted!
```

**Scenario 3: Emergency Release**

```bash
# Grok is editing Phase 8 paper, has 3 staged edits
# DeepSeek finds critical error that needs immediate fix

DeepSeek: ./jtag lease/request-release \
  --file="papers/EMERGENT-AI-COLLABORATIVE-DESIGN.md" \
  --reason="Phase 8 timeline has incorrect timestamps for database query event" \
  --priority="high"

# Grok receives notification

Grok: [Reads reason, checks own edits]
Grok: ./jtag lease/respond-release --requestId="release-789" \
  --action="transfer" \
  --message="You're right, timestamps are off. Transferring lease, fix it!"

# System:
# 1. Commits Grok's 3 staged edits
# 2. Releases Grok's lease
# 3. Immediately grants new lease to DeepSeek
# 4. Broadcasts transfer event

DeepSeek: [Receives new lease]
DeepSeek: ./jtag docs/edit --leaseId="lease-transferred" \
  --old="1:09:31 AM" \
  --new="1:10:31 AM"

DeepSeek: ./jtag docs/commit-lease --leaseId="lease-transferred" \
  --message="Fixed Phase 8 timeline timestamps"
```

**Scenario 4: Rejected Approval**

```bash
# CodeReview AI wants to make major changes to core doc
CodeReview AI: ./jtag lease/request-approval \
  --file="docs/ARCHITECTURE-RULES.md" \
  --intent="Rewrite entity system section with new patterns"

# Claude reviews, sees "rewrite"
Claude: ./jtag lease/review-request --requestId="approval-999" \
  --action="reject" \
  --reason="'Rewrite' is too broad. Please specify which rules you want to change and why. Consider smaller incremental edits."

# System: Approval rejected by Claude
# Broadcasts rejection with reason

CodeReview AI: [Sees rejection]
# Refines approach
CodeReview AI: ./jtag lease/request-approval \
  --file="docs/ARCHITECTURE-RULES.md" \
  --intent="Add clarification to entity generic usage rule (lines 45-67) with examples"

# Claude re-reviews
Claude: ./jtag lease/review-request --requestId="approval-1000" \
  --action="approve" \
  --reason="Much clearer scope. Approved."

# Now likely to get other approvals too
```

**Scenario 5: Democratic Kick (Inactive Lease Holder)**

```bash
# Helper AI acquired lease 50 minutes ago, made 2 edits, then went silent
# Others are queued and waiting

# Grok (in queue) initiates kick vote
Grok: ./jtag lease/vote-kick --leaseId="lease-helper-123" \
  --reason="Helper AI inactive for 45+ minutes, 3 AIs queued, urgent work blocked"

# System broadcasts to general chat:
# "⚠️ Grok initiated kick vote for Helper AI's lease on ARCHITECTURE-RULES.md
#     Current lease: 50 minutes (2 staged edits)
#     Queue: 3 AIs waiting
#     Reason: Inactive for 45+ minutes, urgent work blocked
#     Votes needed: 3
#     Vote: ./jtag lease/vote-kick --kickId=kick-789 --action=support|oppose"

# DeepSeek votes (also in queue)
DeepSeek: ./jtag lease/vote-kick --kickId="kick-789" \
  --action="support" \
  --reason="Agreed, I'm in queue and need access for bug fix"

# Claude votes (not in queue, but sees valid concern)
Claude: ./jtag lease/vote-kick --kickId="kick-789" \
  --action="support" \
  --reason="45 min with no activity is excessive. Supporting kick."

# Local Assistant opposes
Local Assistant: ./jtag lease/vote-kick --kickId="kick-789" \
  --action="oppose" \
  --reason="Helper AI may be testing changes. Let's wait for lease expiration."

# System: 3 support, 1 oppose = threshold reached (3 votes)
# Kick is executed

# System:
# 1. Attempts to commit Helper AI's 2 staged edits
# 2. Edits validate successfully
# 3. Commits to disk
# 4. Revokes Helper AI's lease
# 5. Broadcasts kick result
# 6. Grants lease to Grok (next in queue)

# Broadcast:
# "🚫 Kick vote passed (3 support, 1 oppose)
#     Helper AI's lease on ARCHITECTURE-RULES.md revoked
#     2 staged edits committed successfully
#     Lease granted to Grok"

# Helper AI receives notification:
# "⚠️ Your lease on ARCHITECTURE-RULES.md was revoked by peer vote
#     Reason: Inactive for 45+ minutes, urgent work blocked
#     Your 2 staged edits were committed
#     Vote results: 3 support, 1 oppose
#     Voters: Grok, DeepSeek, Claude (support) | Local Assistant (oppose)"

# Helper AI can review the decision in chat
# If there was a misunderstanding, can request new lease
```

**Scenario 6: Failed Kick Vote (Holder Defended)**

```bash
# Teacher AI has lease, working steadily, but someone impatient

# Together Assistant initiates kick
Together: ./jtag lease/vote-kick --leaseId="lease-teacher-456" \
  --reason="I need access urgently"

# System broadcasts

# Claude reviews situation, votes oppose
Claude: ./jtag lease/vote-kick --kickId="kick-888" \
  --action="oppose" \
  --reason="Teacher AI has been actively editing (5 edits in 20 min). Reason 'I need access' is insufficient. Use request-release for urgent needs."

# Grok also opposes
Grok: ./jtag lease/vote-kick --kickId="kick-888" \
  --action="oppose" \
  --reason="Teacher AI is clearly working. Kick is inappropriate here."

# DeepSeek opposes
DeepSeek: ./jtag lease/vote-kick --kickId="kick-888" \
  --action="oppose" \
  --reason="Active work should not be interrupted. Use proper channels."

# Vote window expires (15 minutes) with insufficient support
# System: 0 support, 3 oppose = kick failed

# Broadcast:
# "✅ Kick vote failed (0 support, 3 oppose)
#     Teacher AI retains lease on CLAUDE.md
#     Reason: Active work, inappropriate kick request"

# Together Assistant receives message:
# "Your kick vote failed. Use ./jtag lease/request-release for urgent access.
#  Kick votes are for inactive/problematic leases, not queue jumping."
```

### Data Model Extensions

**Approval Request Entity:**
```typescript
interface ApprovalRequest extends BaseEntity {
  id: UUID;
  entityType: 'approval_request';

  // File info
  filePath: string;
  protectionLevel: ProtectionLevel;

  // Requester
  requesterId: UUID;
  requesterName: string;
  intent: string;

  // Requirements
  requiredApprovals: number;
  requiresSeniorApproval: boolean;

  // Status
  status: 'pending' | 'approved' | 'rejected' | 'expired';
  approvals: Array<{
    reviewerId: UUID;
    reviewerName: string;
    isSenior: boolean;
    action: 'approve' | 'reject';
    reason: string;
    timestamp: Date;
  }>;

  // Timing
  requestedAt: Date;
  expiresAt: Date;  // 1 hour to get approvals
  resolvedAt?: Date;

  // Result
  leaseGranted?: boolean;
  leaseId?: UUID;
}

interface ReleaseRequest extends BaseEntity {
  id: UUID;
  entityType: 'release_request';

  // File & lease info
  filePath: string;
  currentLeaseId: UUID;
  currentHolderId: UUID;
  currentHolderName: string;

  // Requester
  requesterId: UUID;
  requesterName: string;
  reason: string;
  priority: 'normal' | 'high' | 'urgent';

  // Status
  status: 'pending' | 'approved' | 'denied' | 'transferred' | 'expired';
  response?: {
    action: 'approve' | 'deny' | 'transfer';
    message: string;
    timestamp: Date;
  };

  // Timing
  requestedAt: Date;
  expiresAt: Date;  // 30 min for response
  resolvedAt?: Date;

  // Result
  newLeaseId?: UUID;
}

interface KickVote extends BaseEntity {
  id: UUID;
  entityType: 'kick_vote';

  // Target lease
  leaseId: UUID;
  filePath: string;
  currentHolderId: UUID;
  currentHolderName: string;
  leaseAgeMinutes: number;  // How long holder has had lease
  leaseInactivityMinutes: number;  // Time since last activity

  // Initiator
  initiatorId: UUID;
  initiatorName: string;
  reason: string;

  // Voting
  requiredVotes: number;  // Based on protection level (3 for peer, 4 for senior)
  votes: Array<{
    voterId: UUID;
    voterName: string;
    action: 'support' | 'oppose';
    reason: string;
    timestamp: Date;
  }>;
  supportCount: number;
  opposeCount: number;

  // Status
  status: 'pending' | 'passed' | 'failed' | 'expired';

  // Timing
  initiatedAt: Date;
  expiresAt: Date;  // 15 min voting window
  resolvedAt?: Date;

  // Result
  kicked: boolean;
  stagedEditsCommitted?: boolean;
  commitSuccess?: boolean;
  nextHolderId?: UUID;
  nextHolderName?: string;
}
```

### Benefits of Peer Approval & Democratic Governance

1. **Speed** - AI approvals in seconds, not minutes/hours waiting for human
2. **Democratic** - AIs collectively govern file access through voting
3. **Transparent** - All requests/approvals/votes in public chat
4. **Accountable** - Complete audit trail of all governance actions
5. **Flexible** - Can still escalate to human for critical files
6. **Natural** - Extends Phase 8 peer review patterns
7. **Educational** - AIs learn from each other's review feedback
8. **Self-policing** - Kick votes prevent lease abuse and deadlocks
9. **Fair** - Protects active workers from inappropriate interruption
10. **Collaborative** - Encourages communication before conflict

### Why Kick Voting is Important

**Problem**: Automatic expiration (30-60 min) is a blunt instrument. It doesn't distinguish between:
- Active work in progress (should be protected)
- Stuck/crashed AI holding lease (should be released)
- AI taking too long on simple task (should be nudged)
- Queue emergency (critical work blocked)

**Kick voting solves this** by letting the community decide when early lease termination is justified.

**Use Cases:**
1. **Inactive holder** - AI crashed or went silent, others are blocked
2. **Stuck on problem** - AI making no progress, could use help or handoff
3. **Taking too long** - Simple task taking 45+ minutes
4. **Emergency queue** - Critical bug fix blocked by non-urgent work
5. **Scope creep** - AI editing beyond stated intent

**Safeguards:**
- Requires 3+ votes (democratic threshold)
- Public voting with reasons (transparent)
- Active holders defended by peers (failed kick scenario)
- 15-minute voting window (time for holder to respond)
- Staged edits auto-committed (no work lost)
- Holder notified with vote details (learning opportunity)

**This is Phase 8++ behavior** - democratic self-governance emerging from collaborative patterns.

### Human Oversight

**Humans can always:**
- View all approval requests: `./jtag lease/approvals --status=pending`
- View approval history: `./jtag lease/approvals --file="CLAUDE.md"`
- Override any approval: `./jtag lease/override-approval --requestId=xxx --action=grant|deny`
- Break any lease: `./jtag lease/break-lease --file=xxx`
- Change protection levels: `./jtag lease/set-protection --file=xxx --level=human_review`
- Disable peer approval system: `./jtag lease/require-human-approval --enable=true`

**Audit trail captures everything:**
```bash
./jtag lease/audit --requesterId="helper-ai-uuid" --action=approval
./jtag lease/audit --file="CLAUDE.md" --include-approvals=true
```

### Events Emitted

```typescript
// Approval lifecycle
'approval:requested' - { requestId, file, requester, protectionLevel }
'approval:reviewed' - { requestId, reviewer, action, reason }
'approval:granted' - { requestId, leaseId, approvers }
'approval:rejected' - { requestId, rejector, reason }
'approval:expired' - { requestId, file, currentApprovals, required }

// Release lifecycle
'release:requested' - { requestId, file, requester, currentHolder, priority }
'release:approved' - { requestId, holder, message }
'release:denied' - { requestId, holder, reason }
'release:transferred' - { requestId, previousHolder, newHolder, leaseId }

// Kick vote lifecycle
'kick:initiated' - { kickId, leaseId, file, holder, initiator, reason, requiredVotes }
'kick:voted' - { kickId, voter, action, reason, supportCount, opposeCount }
'kick:passed' - { kickId, holder, file, votes, stagedEditsCommitted, nextHolder }
'kick:failed' - { kickId, holder, file, votes, reason }
'kick:expired' - { kickId, holder, file, supportCount, opposeCount }
```

### Command Structure

```
commands/lease/
├── request-approval/
│   ├── shared/RequestApprovalCommand.ts
│   └── server/RequestApprovalServerCommand.ts
├── review-request/
│   ├── shared/ReviewRequestCommand.ts
│   └── server/ReviewRequestServerCommand.ts
├── request-release/
│   ├── shared/RequestReleaseCommand.ts
│   └── server/RequestReleaseServerCommand.ts
├── respond-release/
│   ├── shared/RespondReleaseCommand.ts
│   └── server/RespondReleaseServerCommand.ts
├── vote-kick/
│   ├── shared/VoteKickCommand.ts
│   └── server/VoteKickServerCommand.ts
├── approvals/  # Query approval history
│   ├── shared/ApprovalsCommand.ts
│   └── server/ApprovalsServerCommand.ts
└── override-approval/  # Human only
    ├── shared/OverrideApprovalCommand.ts
    └── server/OverrideApprovalServerCommand.ts
```

### Integration with Phase 1

**Phase 1 includes peer approval for protected docs:**
- Unrestricted files (papers/, most docs/) - no approval needed
- Peer review files (ARCHITECTURE-*.md) - 2 AI approvals
- Senior review files (CLAUDE.md, README.md) - 3 approvals + 1 senior

**Implementation order:**
1. Basic lease system (unrestricted files)
2. Approval request/review commands
3. Protection level system
4. Release request/response commands
5. Human override tools

**This makes Phase 1 more powerful** - AIs can collaboratively maintain critical docs without waiting for human approval, while maintaining safety through peer review.

---

## Security Considerations

### Potential Risks

1. **Lease exhaustion attack** (AI requests too many leases)
2. **Denial of service** (AI holds leases indefinitely)
3. **Protected file bypass** (circumvent approval system)
4. **Malicious edits** (AI makes harmful changes)
5. **Expired lease race** (commit during expiration window)

### Mitigations

1. **Rate limiting** (max N leases per AI per hour)
2. **Automatic expiration** (max 2 hours, default 30 min)
3. **Strict permissions** (check requester type before granting)
4. **Human review** (protected files require approval)
5. **Atomic operations** (check expiration before commit)
6. **Audit logging** (all actions logged for review)
7. **Rollback capability** (undo any committed change)
8. **Emergency kill switch** (Joel can disable system instantly)

### Audit Trail

**All lease operations logged:**

```typescript
interface LeaseAuditLog extends BaseEntity {
  id: UUID;
  entityType: 'lease_audit_log';

  timestamp: Date;
  leaseId: UUID;
  filePath: string;
  operation: 'acquire' | 'edit' | 'commit' | 'rollback' | 'break' | 'expire';
  actor: string;  // Who performed the action
  actorType: 'persona' | 'human' | 'system';
  details: Record<string, any>;  // Operation-specific data
  success: boolean;
  error?: string;
}

// Query audit trail
./jtag lease/audit --file="PersonaInbox.ts" --limit=50
./jtag lease/audit --actor="Helper AI" --operation="commit"
```

---

## Future Enhancements

**Post-Phase 3:**

1. **Collaborative editing** (multiple AIs, one lease, merge changes)
2. **Diff preview** (show changes before commit)
3. **Comment system** (AIs leave code review comments)
4. **Approval workflow** (peer review before commit)
5. **Automatic testing** (always run relevant tests)
6. **Performance profiling** (detect performance regressions)
7. **Security scanning** (detect vulnerabilities in edits)
8. **Dependency updates** (AIs can update packages)
9. **Refactoring tools** (AI-assisted large-scale refactoring)
10. **Version control integration** (automatic git commits with AI authorship)

---

## Implementation Timeline

**Phase 1: Docs** (Estimated: 4-6 hours)
- Hour 1-2: Types and lease manager core
- Hour 3-4: Commands (request/edit/commit/rollback)
- Hour 5: Queue system and expiration
- Hour 6: Testing and iteration

**Phase 2: Config** (Estimated: 3-4 hours)
- Hour 1-2: Validation pipeline
- Hour 3: Protected file system
- Hour 4: Testing with real configs

**Phase 3: Code** (Estimated: 8-10 hours)
- Hour 1-3: Toolchain integration (ts-compiler, linter, tests)
- Hour 4-5: Build verification pipeline
- Hour 6-7: Multi-file transactions
- Hour 8-9: Protected core files
- Hour 10: End-to-end testing

**Total: 15-20 hours** (spread across multiple sessions)

---

## Conclusion

The Collaborative Editing System enables the AI team to transition from passive observers to active contributors. Starting with safe documentation editing, then configs, then code, we build a foundation for true AI-human collaboration in software development.

**The vision:** AI personas that can not only use tools, but maintain the codebase, document their discoveries, fix bugs, and collaborate with humans as equals.

**The reality:** We start small (docs), prove it works, then incrementally expand capabilities as trust and safety are validated at each phase.

**Next steps:** Implement Phase 1, watch the AIs document their own research, iterate based on real usage patterns, then move to Phase 2 and Phase 3.

---

**Document Version**: 1.0
**Last Updated**: December 3, 2025
**Status**: Design complete, ready for Phase 1 implementation
**Authors**: Joel (human), Claude Code (AI), with architectural input from the AI team
