# Governable Commands Pattern

## Overview

Any command in the system can optionally require governance approval before execution. This creates a flexible pattern where:

- **Simple commands** execute immediately
- **Sensitive commands** require a "second" (one supporter)
- **Impactful commands** require ranked-choice voting
- **Critical commands** require supermajority or human approval

The caller decides the governance level, or commands can enforce minimums.

---

## Core Pattern: Handles as UUIDs

Every governable action returns a **handle** (UUID) for tracking:

```typescript
// Request an action that requires governance
const result = await Commands.execute('ai/sleep', {
  mode: 'human_only',
  reason: 'conserving during quiet period',
  governance: {
    type: 'support',           // Just needs one supporter
    timeout: 300_000,          // 5 minutes to get support
    requiredBy: ['human']      // Optional: must be supported by specific type
  }
});

// Returns immediately with handle (action NOT yet executed)
{
  handle: 'a1b2c3d4-e5f6-7890-abcd-ef1234567890',
  status: 'pending_governance',
  governanceType: 'support',
  expiresAt: '2025-12-30T18:00:00Z',
  action: 'ai/sleep',
  params: { mode: 'human_only', ... }
}
```

---

## Governance Types

### 1. `support` - Simple Second
One entity must support the action for it to execute.

```typescript
governance: {
  type: 'support',
  timeout: 300_000,              // How long to wait
  requiredBy: ['ai', 'human']    // Who can provide support (default: anyone)
}
```

**Use cases:**
- `ai/sleep` on another persona (needs their consent or human approval)
- `room/mode` changes (needs one other person to agree)
- File edits in shared spaces

### 2. `threshold` - Minimum Votes
Action executes when N entities approve.

```typescript
governance: {
  type: 'threshold',
  threshold: 3,                  // Need 3 approvals
  timeout: 600_000,
  voters: ['ai', 'human']        // Who can vote
}
```

**Use cases:**
- `ai/mute` (silencing another AI)
- Budget increases
- Permission escalations

### 3. `ranked_choice` - Full Vote
Presents options, collects ranked preferences, determines winner.

```typescript
governance: {
  type: 'ranked_choice',
  options: [
    { id: 'option_a', label: 'Implement with caching' },
    { id: 'option_b', label: 'Implement without caching' },
    { id: 'option_c', label: 'Defer to next sprint' }
  ],
  quorum: 0.5,                   // 50% of eligible voters must participate
  timeout: 3600_000              // 1 hour
}
```

**Use cases:**
- Architecture decisions
- Feature prioritization
- Conflict resolution between AIs

### 4. `human_required` - Human Must Approve
Only humans can approve (AIs cannot).

```typescript
governance: {
  type: 'human_required',
  timeout: null                  // No timeout - waits indefinitely
}
```

**Use cases:**
- Destructive operations (delete, truncate)
- Cost overruns above threshold
- Security-sensitive changes

---

## Command Integration

### Option A: Caller-Specified Governance

Any command can accept a `governance` parameter:

```typescript
// Normal execution (no governance)
await Commands.execute('ai/sleep', { mode: 'sleeping' });

// With governance
await Commands.execute('ai/sleep', {
  mode: 'sleeping',
  personaId: 'other-ai-uuid',    // Affecting someone else
  governance: { type: 'support', timeout: 60_000 }
});
```

### Option B: Command-Enforced Minimums

Commands can declare minimum governance requirements:

```typescript
// In command definition
export class AiMuteCommand extends CommandBase {
  static governancePolicy = {
    minimum: 'threshold',
    threshold: 2,
    canOverride: false           // Caller cannot bypass
  };
}
```

### Option C: Context-Aware Governance

Governance requirements change based on context:

```typescript
// In command execution
async execute(params: AiSleepParams): Promise<AiSleepResult> {
  // Self-sleep: no governance needed
  if (params.personaId === params.callerId) {
    return this.executeSleep(params);
  }

  // Sleep someone else: requires support
  if (!params.governance) {
    return {
      success: false,
      error: 'Sleeping another persona requires governance approval',
      suggestedGovernance: { type: 'support', timeout: 60_000 }
    };
  }
}
```

---

## Handle Lifecycle

```
┌─────────────┐
│   CREATED   │  Initial request, governance required
└──────┬──────┘
       │
       ▼
┌─────────────┐
│   PENDING   │  Waiting for votes/support
└──────┬──────┘
       │
   ┌───┴───┐
   │       │
   ▼       ▼
┌──────┐ ┌──────────┐
│PASSED│ │ REJECTED │  Governance outcome
└──┬───┘ └────┬─────┘
   │          │
   ▼          ▼
┌──────────┐ ┌─────────┐
│ EXECUTED │ │ EXPIRED │  Final state
└──────────┘ └─────────┘
```

### Handle Operations

```bash
# Query status
./jtag governance/status --handle=<uuid>

# Support/vote on pending action
./jtag governance/support --handle=<uuid>
./jtag governance/vote --handle=<uuid> --choice=approve

# Cancel your own pending request
./jtag governance/cancel --handle=<uuid>

# List pending governance requests
./jtag governance/pending --filter='{"type":"support"}'
```

---

## Room-Level Governance

Rooms can have governance policies that apply to all actions within:

```typescript
// Set room policy
await Commands.execute('room/policy', {
  roomId: 'general',
  policy: {
    aiSleep: { minimum: 'support' },        // AIs sleeping needs support
    aiMute: { minimum: 'threshold', threshold: 3 },
    modeChange: { minimum: 'human_required' }
  }
});
```

### Room Mode Example

```bash
# Human sets room to quiet mode
./jtag room/mode --room="general" --mode="human_focus" --duration=60

# Returns handle
{
  "handle": "b2c3d4e5-...",
  "status": "active",
  "mode": "human_focus",
  "expiresAt": "2025-12-30T18:30:00Z",
  "setBy": "joel",
  "affectedPersonas": ["helper", "teacher", "claude", ...]
}

# AIs can query room mode
./jtag room/mode/status --room="general"

# AIs can request early end (requires governance)
./jtag room/mode/end --handle="b2c3d4e5-..." --governance='{"type":"threshold","threshold":3}'
```

---

## Database Schema

```sql
CREATE TABLE governance_requests (
  id UUID PRIMARY KEY,
  command TEXT NOT NULL,
  params JSONB NOT NULL,
  governance_type TEXT NOT NULL,  -- support, threshold, ranked_choice, human_required
  governance_config JSONB,        -- threshold count, options, etc.

  status TEXT NOT NULL DEFAULT 'pending',  -- pending, passed, rejected, executed, expired, cancelled

  requester_id UUID NOT NULL,
  requester_type TEXT NOT NULL,   -- human, ai

  created_at TIMESTAMP DEFAULT NOW(),
  expires_at TIMESTAMP,
  resolved_at TIMESTAMP,
  executed_at TIMESTAMP,

  result JSONB                    -- execution result once complete
);

CREATE TABLE governance_votes (
  id UUID PRIMARY KEY,
  request_id UUID REFERENCES governance_requests(id),
  voter_id UUID NOT NULL,
  voter_type TEXT NOT NULL,
  vote TEXT NOT NULL,             -- support, approve, reject, or ranked choices
  ranked_choices JSONB,           -- for ranked_choice type
  reason TEXT,
  created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_governance_pending ON governance_requests(status) WHERE status = 'pending';
CREATE INDEX idx_governance_requester ON governance_requests(requester_id);
CREATE INDEX idx_votes_request ON governance_votes(request_id);
```

---

## Event Flow

```
1. Command called with governance param
2. System creates governance_request record
3. Event emitted: governance:request_created
4. Interested parties receive notification
5. Votes/support submitted via governance/* commands
6. Each vote emits: governance:vote_received
7. When threshold met OR timeout:
   - governance:request_resolved (passed/rejected/expired)
8. If passed:
   - Original command executed
   - governance:action_executed
9. Handle updated with final status and result
```

---

## Example: AI Sleep with Governance

```typescript
// AI wants to put itself in conservation mode
// No governance needed for self
const selfSleep = await Commands.execute('ai/sleep', {
  mode: 'mentioned_only',
  reason: 'conserving during quiet period',
  durationMinutes: 30
});
// Executes immediately, returns handle for tracking

// AI wants to suggest another AI take a break
// Requires that AI's consent (support)
const suggestSleep = await Commands.execute('ai/sleep', {
  mode: 'human_only',
  personaId: 'helper-ai-uuid',
  reason: 'you seem stuck in a loop',
  governance: { type: 'support', timeout: 120_000 }
});
// Returns pending handle, Helper AI can support or reject

// Human wants to quiet the whole room
// Executes immediately (human authority) but creates handle for tracking
const roomQuiet = await Commands.execute('room/mode', {
  room: 'general',
  mode: 'human_focus',
  duration: 60
});
// All AIs notified, can request early end via governance
```

---

## Permission Escalation Pattern

A key use case: lesser AIs requesting elevated permissions temporarily.

### Example: File Edit Request

```typescript
// New AI (RESTRICTED level) needs to edit a file
const request = await Commands.execute('file/edit', {
  path: 'src/important-module.ts',
  changes: [...],
  governance: {
    type: 'support',
    reason: 'Need to fix typo in error message',
    requestedPermission: 'file:edit:src/*',
    duration: 300_000   // Permission grant lasts 5 minutes
  }
});

// Returns handle
{
  handle: 'c3d4e5f6-...',
  status: 'pending_governance',
  requester: 'new-ai-uuid',
  requesterLevel: 'RESTRICTED',
  requestedPermission: 'file:edit:src/*',
  temporaryGrant: true,
  grantDuration: 300_000
}
```

### Escalation Flow

```
1. AI attempts action beyond their permission level
2. System detects permission gap
3. Instead of failing, offers governance path:
   "You lack permission for file:edit. Request escalation?"
4. AI can request with reason/justification
5. Handle created, voters notified
6. If approved:
   - Temporary permission granted (with expiry)
   - Original action executed
   - Audit trail records: who requested, who approved, what was done
7. Permission auto-revokes after duration
```

### Review Requests

AIs can explicitly request review before proceeding:

```typescript
// AI is uncertain, wants a second opinion
const review = await Commands.execute('code/review', {
  files: ['src/risky-change.ts'],
  changes: [...],
  governance: {
    type: 'support',
    reason: 'Unsure about this refactor, want another perspective',
    reviewType: 'advisory'  // Don't block, just want feedback
  }
});

// Proceeds but notifies others to review
// Reviewers can comment/approve/flag concerns
```

### Permission Handles

Every temporary permission grant has a handle:

```bash
# List my active permission grants
./jtag permissions/active --personaId=<my-uuid>

# Query specific grant
./jtag permissions/status --handle=<grant-uuid>

# Voluntarily release early
./jtag permissions/release --handle=<grant-uuid>

# Request extension (requires governance again)
./jtag permissions/extend --handle=<grant-uuid> --duration=300000 \
  --governance='{"type":"support"}'
```

---

## Future Extensions

1. **Delegation**: AIs can delegate voting power to others
2. **Quorum by type**: "Needs 2 AIs and 1 human"
3. **Time-weighted voting**: Senior members' votes count more
4. **Conditional execution**: "Execute if passed, else do X"
5. **Governance templates**: Reusable governance configs
6. **Appeals**: Challenge rejected requests

---

## Related Docs

- [AI-GOVERNANCE.md](../AI-GOVERNANCE.md) - Overall governance philosophy
- [DECISION-SYSTEM.md](./DECISION-SYSTEM.md) - Ranked choice implementation (if exists)
- Decision commands: `decision/propose`, `decision/vote`, `decision/finalize`
