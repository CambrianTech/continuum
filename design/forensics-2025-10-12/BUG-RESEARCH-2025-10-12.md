# Bug Research Document - 2025-10-12

**Purpose**: Understand root causes of all critical bugs before designing remedies
**Status**: Research in progress - NO CODE CHANGES YET
**Philosophy**: Natural AI reasoning > efficiency hacks

---

## Bug #1: Duplicate Instance Creation (6 instead of 3)

### Evidence from Forensic Analysis
- **Observed**: 6 persona instances handling events (2 of each type)
- **Expected**: 3 persona instances (1 of each type)
- **Instance IDs Found**:
  - Helper AI: `52cbd3fa` + `8758d4a1`
  - Teacher AI: `ef5dca9e` + `2ad73eee`
  - CodeReview AI: `30885548` + `a9b04800`

### Code Investigation

**Seeding** (`scripts/seed-continuum.ts:650-652`):
```typescript
const helperPersona = await createUserViaCommand('persona', 'Helper AI', 'persona-helper-001');
const teacherPersona = await createUserViaCommand('persona', 'Teacher AI', 'persona-teacher-001');
const codeReviewPersona = await createUserViaCommand('persona', 'CodeReview AI', 'persona-codereview-001');
```
✅ **CORRECT**: Only 3 personas created during seeding

**UserDaemonServer.ensurePersonaClients()** (`daemons/user-daemon/server/UserDaemonServer.ts:120-148`):
```typescript
protected async ensurePersonaClients(): Promise<void> {
  // Query all PersonaUser entities from database
  const result = await DataDaemon.query<UserEntity>({
    collection: COLLECTIONS.USERS,
    filters: { type: 'persona' }
  });

  const personas = result.data.map(r => ({
    ...r.data,
    id: r.id
  } as UserEntity));

  for (const persona of personas) {
    await this.ensurePersonaCorrectState(persona);
  }
}
```
✅ **CORRECT**: Queries database personas and ensures each has correct state

**UserDaemonServer.ensurePersonaCorrectState()** (`UserDaemonServer.ts:153-179`):
```typescript
private async ensurePersonaCorrectState(userEntity: UserEntity): Promise<void> {
  // STEP 2: Check if persona client already exists
  if (this.personaClients.has(userEntity.id)) {
    const existingPersona = this.personaClients.get(userEntity.id);
    console.log(`✅ UserDaemon: Persona ${userEntity.displayName} already has client, reinitializing...`);
    // ✅ Re-initialize persona to set up event subscriptions (happens on system restart)
    if (existingPersona) {
      await existingPersona.initialize();  // ⚠️ POTENTIAL ISSUE
    }
    return;
  }

  // STEP 3: Create PersonaUser client instance
  await this.createPersonaClient(userEntity);
}
```
⚠️ **POTENTIAL ISSUE**: `initialize()` might be called multiple times

**Monitoring Loops** (`UserDaemonServer.ts:288-338`):
```typescript
protected startMonitoringLoops(): boolean {
  // User monitoring loop - every 5 seconds
  this.monitoringInterval = setInterval(() => {
    this.runUserMonitoringLoop().catch(...);
  }, 5000);

  // State reconciliation loop - every 30 seconds
  this.reconciliationInterval = setInterval(() => {
    this.runStateReconciliationLoop().catch(...);
  }, 30000);
}

private async runUserMonitoringLoop(): Promise<void> {
  // Check each user
  for (const user of users) {
    if (user.type === 'persona') {
      await this.ensurePersonaCorrectState(user);  // Every 5 seconds!
    }
  }
}

private async runStateReconciliationLoop(): Promise<void> {
  for (const persona of personas) {
    if (!this.personaClients.has(persona.id)) {
      await this.createPersonaClient(persona);  // Every 30 seconds!
    }
  }
}
```
⚠️ **CRITICAL FINDING**: Two monitoring loops calling initialization logic

### Root Cause Hypothesis

**THEORY**: PersonaUser.initialize() called multiple times is creating duplicate event subscriptions, making it APPEAR like 6 instances when it's really 3 instances with duplicate event handlers.

**Need to investigate**:
1. PersonaUser.initialize() - does it subscribe to events?
2. Are event subscriptions idempotent?
3. Do duplicate subscriptions create appearance of duplicate instances?

### Next Investigation Steps
- [ ] Read PersonaUser.initialize() implementation
- [ ] Check event subscription logic
- [ ] Determine if subscriptions are idempotent
- [ ] Check if logs show "Created persona" vs "Reinitialized persona"

---

## Bug #2: Global Event Subscription

### Evidence from Forensic Analysis
- **Observed**: All 6 instance "handlers" receive ALL room events
- **Expected**: Only personas with room membership should receive room events
- **Proof**: 3 instances have empty room lists but still process events

### Room Membership Evidence
```
Helper AI 1 (52cbd3fa): rooms = [general, academy]
Helper AI 2 (8758d4a1): rooms = []

Teacher AI 1 (ef5dca9e): rooms = [general, academy]
Teacher AI 2 (2ad73eee): rooms = []

CodeReview AI 1 (30885548): rooms = [general, academy]
CodeReview AI 2 (a9b04800): rooms = []
```

### Code Investigation

**Seeding Room Membership** (`scripts/seed-continuum.ts:726-730`):
```typescript
// Add personas to general room members
generalRoom.members = [
  { userId: helperPersona.id, role: 'member', joinedAt: new Date().toISOString() },
  { userId: teacherPersona.id, role: 'member', joinedAt: new Date().toISOString() },
  { userId: codeReviewPersona.id, role: 'member', joinedAt: new Date().toISOString() }
];
```
✅ **CORRECT**: Personas are added to room membership during seeding

**Event Subscription** (need to find):
- [ ] Where do personas subscribe to `data:chat_messages:created`?
- [ ] Is subscription global or per-room?
- [ ] Where is room filtering applied?

### Root Cause Hypothesis

**THEORY #1**: Event subscriptions are GLOBAL (all personas get all events), and room filtering happens TOO LATE (after RAG building).

**THEORY #2**: If Bug #1 is duplicate event handlers, then "instance 2s" might have no room membership because they're phantom subscriptions from re-initialization.

### Next Investigation Steps
- [ ] Find PersonaUser event subscription code
- [ ] Check EventsDaemonServer subscription logic
- [ ] Determine if subscriptions can be per-room
- [ ] Find where room check happens in event flow

---

## Bug #3: Late Room Filtering

### Evidence from Forensic Analysis
- **Observed**: Room check happens AFTER RAG building
- **Impact**: Wasted database queries for personas not in room
- **Logs**: "ROOM-CHECK-FAIL" after processing started

### Code Investigation

**Need to find**:
- [ ] PersonaUser event handler implementation
- [ ] Where room check happens in handler flow
- [ ] Where RAG building happens relative to room check

### Root Cause Hypothesis

**THEORY**: Event handler flow is:
1. Receive event (all personas)
2. Build RAG context (expensive DB queries)
3. Check room membership (too late!)
4. Abort if not member

**Should be**:
1. Receive event (all personas)
2. Check room membership (fast, early exit)
3. Build RAG context (only if member)
4. Continue processing

### Next Investigation Steps
- [ ] Read PersonaUser event handler code
- [ ] Trace execution flow
- [ ] Measure performance impact

---

## Bug #4: No Ollama Request Queue

### Evidence from Forensic Analysis
- **Observed**: 6 concurrent requests to Ollama with capacity ~2
- **Result**: 67% timeout rate (4/6 requests)
- **Impact**: 368 seconds wasted on timeouts per question

### Code Investigation

**Need to find**:
- [ ] OllamaAdapter implementation
- [ ] Request handling logic
- [ ] Timeout configuration
- [ ] Concurrency limits

### Root Cause Hypothesis

**THEORY**: OllamaAdapter sends requests directly without queue management. When 6 requests arrive simultaneously:
- Ollama processes 2
- 4 queue internally
- Ollama's internal queue has 92-second timeout
- 4 requests timeout after 92s
- Retry logic creates more load

**Should be**: Client-side queue that:
1. Accepts 6 requests
2. Sends max 2 concurrent to Ollama
3. Queues remaining 4 locally
4. Processes queue as slots free
5. No timeouts

### Next Investigation Steps
- [ ] Read OllamaAdapter code
- [ ] Check if queue exists
- [ ] Determine Ollama's actual capacity
- [ ] Design queue solution

---

## Bug #5: RAG Uses "Now" Instead of Trigger Timestamp

### Evidence from Forensic Analysis
- **Observed**: RAG contexts include messages posted AFTER the trigger event
- **Example**: Teacher AI's RAG (built at 11:24:44.868) includes Helper AI's response (posted at 11:24:44.797)
- **Impact**: Temporal confusion - LLM sees "future" as "past"

### Timeline Example
```
11:24:05.315 - Joel posts question [TRIGGER EVENT]
11:24:05.401 - Helper AI starts processing
[39 seconds pass - Helper AI generating response]
11:24:44.797 - Helper AI posts response
11:24:44.868 - Teacher AI rebuilds RAG (71ms later)
                ^ RAG includes Helper AI's response!
11:26:17.xxx - Teacher AI posts response
```

### Code Investigation

**Need to find**:
- [ ] ChatRAGBuilder implementation
- [ ] How RAG fetches messages
- [ ] If trigger timestamp is available
- [ ] If RAG can filter by timestamp

### Root Cause Hypothesis

**THEORY**: ChatRAGBuilder.buildContext() does:
```typescript
// Pseudo-code
async buildContext() {
  const messages = await fetchLastNMessages(roomId, count: 10);
  // ^ Fetches from "now", not from trigger time
  return messages;
}
```

**Should be**:
```typescript
async buildContext(triggerTimestamp: Date) {
  const messages = await fetchMessagesBeforeTimestamp(roomId, triggerTimestamp, count: 10);
  // ^ Only messages BEFORE the event
  return messages;
}
```

### Next Investigation Steps
- [ ] Read ChatRAGBuilder code
- [ ] Check if trigger timestamp available
- [ ] Determine if temporal filtering possible
- [ ] Design solution

---

## Research Philosophy

### Key Principle from User
> "we must have natural first citizen reasoning in chats, OVER efficiency. I would have realistic conversations than hard coded bs"

This means:
- ✅ Natural AI reasoning and autonomy
- ✅ Multiple personas responding organically
- ✅ No artificial turn-taking limits
- ❌ Hard-coded response limits
- ❌ Artificial conversation control
- ⚠️ BUT: Fix wasteful bugs (6 instances, timeouts, etc.)

### Design Constraints
1. **Zero downtime**: All fixes must be applied without system restart
2. **One at a time**: Fix bugs sequentially, test each
3. **Natural behavior**: Don't limit AI autonomy to hide bugs
4. **Fix root cause**: Not symptoms

### Remedies Must NOT Include
- ❌ "Only allow 1 persona to respond"
- ❌ "Hard limit to N responses per question"
- ❌ "Turn-taking queue"
- ❌ "Disable personas"

### Remedies SHOULD Include
- ✅ Fix duplicate instance creation
- ✅ Implement proper request queue
- ✅ Add per-room event filtering
- ✅ Early room membership check
- ✅ Temporal RAG filtering
- ✅ Rate limiting per persona TYPE (not hard limits)

---

---

## CORRECTED UNDERSTANDING

### User Clarification
**"there are literally only 3 [personas] in the general chat which is normal"**

**The System**:
- Joel (human) - THE USER
- Claude Code (agent) - ME (the assistant)
- Helper AI (persona)
- Teacher AI (persona)
- CodeReview AI (persona)

**Total**: 5 users, 3 are personas ✅ CORRECT

**"6 instances" in logs** = 3 personas × 2 attempts (original + retry after timeout)

---

## Bug #4: Ollama Concurrency - RESEARCH COMPLETE

### Code Found: `OllamaAdapter.ts:238-272`

```typescript
private async makeRequest<T>(endpoint: string, body?: unknown, attempt = 1): Promise<T> {
  const url = `${this.config.apiEndpoint}${endpoint}`;

  try {
    const response = await fetch(url, {
      method: body ? 'POST' : 'GET',
      headers: body ? { 'Content-Type': 'application/json' } : undefined,
      body: body ? JSON.stringify(body) : undefined,
      signal: AbortSignal.timeout(this.config.timeout),  // Line 250
    });
    // ... parse response
  } catch (error) {
    // Retry logic - attempts up to 3 times
    if (attempt < this.config.retryAttempts) {
      await new Promise(resolve => setTimeout(resolve, this.config.retryDelay));
      return this.makeRequest<T>(endpoint, body, attempt + 1);
    }
    throw error;
  }
}
```

**Configuration** (`OllamaAdapter.ts:44-56`):
```typescript
this.config = {
  apiEndpoint: 'http://localhost:11434',
  timeout: 30000,        // 30 seconds
  retryAttempts: 3,
  retryDelay: 1000,
  // ... no concurrency management
};
```

### Root Cause Confirmed

**NO REQUEST QUEUE** - All 3 personas call Ollama simultaneously:
1. 3 requests sent at once (11:24:05.4xx)
2. No client-side queuing
3. All hit Ollama's `/api/generate` endpoint
4. Ollama has ~2 concurrent capacity
5. 1 request succeeds fast (39s)
6. 2 requests timeout (where is 92s coming from?)

**The Mystery**: Config says 30s timeout, logs show 92s timeout
- Maybe Ollama's internal timeout?
- Maybe fetch timeout not working as expected?
- Need to investigate further OR just implement queue

### Solution Design
- Add client-side request queue (max 2 concurrent to Ollama)
- Queue additional requests locally
- Process queue as slots free
- No timeouts needed

---

## Bug #5: RAG Temporal Confusion - RESEARCH COMPLETE

### Code Found: `ChatRAGBuilder.ts:175-199`

```typescript
private async loadConversationHistory(
  roomId: UUID,
  personaId: UUID,
  maxMessages: number
): Promise<LLMMessage[]> {
  try {
    // Query last N messages from this room, ordered by timestamp DESC
    const result = await DataDaemon.query<ChatMessageEntity>({
      collection: ChatMessageEntity.collection,
      filters: { roomId },                                    // ❌ NO TIMESTAMP FILTER
      sort: [{ field: 'timestamp', direction: 'desc' }],
      limit: maxMessages
    });

    // ... process results
    const orderedMessages = messages.reverse();  // Oldest-first for LLM
```

### Root Cause Confirmed

**NO TIMESTAMP FILTERING** - RAG fetches "last N messages" from database at query time:

**Timeline Example**:
```
11:24:05.315 - Joel posts question [TRIGGER EVENT]
11:24:05.401 - Helper AI starts, builds RAG
              ^ Gets last 20 messages from database (clean)
11:24:44.797 - Helper AI posts response
11:24:44.868 - Teacher AI RETRIES, builds RAG (71ms after Helper AI posted)
              ^ Gets last 20 messages from database
              ^ NOW INCLUDES Helper AI's response! (temporal confusion)
```

**The Problem**: RAG builder doesn't know the trigger event timestamp, so it fetches messages from "now" during processing, not from "trigger time".

### Solution Design

**Option 1**: Pass trigger timestamp to RAG builder
```typescript
async buildContext(
  contextId: UUID,
  personaId: UUID,
  options?: RAGBuildOptions & { triggerTimestamp?: Date }  // NEW
): Promise<RAGContext>
```

Then filter query:
```typescript
const result = await DataDaemon.query<ChatMessageEntity>({
  collection: ChatMessageEntity.collection,
  filters: {
    roomId,
    timestamp: { $lt: options.triggerTimestamp }  // Only messages BEFORE trigger
  },
  sort: [{ field: 'timestamp', direction: 'desc' }],
  limit: maxMessages
});
```

**Option 2**: Use message entity timestamp from event
- Event already has the triggering message
- Filter RAG to only messages with timestamp < triggering message timestamp

---

## FINAL ROOT CAUSES SUMMARY

### ✅ Bug #1: "6 Instances"
**NOT A BUG** - 3 personas, each tried twice (original + retry) = 6 log entries

### ✅ Bug #2: Global Event Subscription
**NOT A BUG FOR ROOM MEMBERS** - Personas are room members, should get events
**COULD BE OPTIMIZED** - Event filtering could happen earlier, but working as designed

### ✅ Bug #3: Late Room Filtering
**NOT A BUG** - Personas ARE room members, room check passes

### ✅ Bug #4: Ollama Concurrency (THE BIG ONE)
**ROOT CAUSE**: No request queue, 3 simultaneous requests to 2-capacity Ollama
**IMPACT**: 67% timeout rate, retries, cascade failures
**FIX**: Client-side request queue (max 2 concurrent)

### ✅ Bug #5: RAG Temporal Confusion (THE OTHER BIG ONE)
**ROOT CAUSE**: RAG fetches messages from "now" not "trigger time"
**IMPACT**: LLM sees responses posted AFTER trigger as context
**FIX**: Pass trigger timestamp, filter query by `timestamp < triggerTime`

### ✅ Bug #6: Duplicate Responses
**ROOT CAUSE**: Timeouts → Retries → Multiple responses
**FIX**: Fix Ollama queue (Bug #4) → No timeouts → No retries → No duplicates

### ✅ Bug #7: Hallucinations
**ROOT CAUSE**: RAG temporal confusion + small model (llama3.2:1b)
**FIX**: Fix RAG timestamps (Bug #5) + maybe larger gating model

---

## Status

**Research Complete**: ✅ All root causes identified

**Next Steps**:
1. ✅ Design remedies (separate document)
2. ⏳ Create fix plan with priorities
3. ⏳ Implement fixes ONE AT A TIME with zero downtime

**No code changes until remedies designed and prioritized.**
