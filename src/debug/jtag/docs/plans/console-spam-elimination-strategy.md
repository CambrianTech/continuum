# Console Spam Elimination Strategy - UPDATED

**Date**: 2025-12-01
**Status**: Analysis Complete - Ready for Implementation
**Context**: npm-start.log analysis showing runtime console spam patterns + persona log investigation

## Executive Summary

Analyzed npm-start.log and discovered TWO major categories of console spam:

### Category A: Infrastructure Spam (Identified in npm-start.log)
- PersonaState "Rested" messages (CRITICAL - 90% volume)
- Events daemon subscription spam
- WebSocket transport logging
- Data daemon operations
- Router timeout errors (REAL BUGS - separate from spam)

### Category B: Missing Persona Logs (NEW - Discovered via user.log investigation)
- Cognition system messages
- CognitionLogger outputs
- WorkingMemory operations
- PersonaInbox messages
- Data operations related to personas
- Misplaced log files (cognition.log, ai-decisions.log)

**User Observation**: persona user.log contains only 22 lines of initialization messages - missing ALL runtime cognitive activity logs.

---

## Philosophy: Log Segregation by Concern

**"Keep logs that help debug, remove logs that just say 'I did my job'"**

### Infrastructure Logs
Located at: `.continuum/jtag/system/logs/`
- `system.log` - System initialization, daemon startup, critical lifecycle
- `sql.log` - Database operations (already converted via Logger system)
- `tools.log` - PersonaToolExecutor operations (already converted)
- `websocket.log` (NEW) - WebSocket transport layer events
- `ai-adapters.log` (NEW) - AI provider adapter operations

### Persona-Specific Logs
Located at: `.continuum/personas/{uniqueId}/logs/`
- `user.log` - Main persona activity (initialization, message handling)
- `mind.log` - Mind subsystem (planning, reasoning)
- `body.log` - Body subsystem (tool execution, actions)
- `soul.log` - Soul subsystem (emotional state, values)
- `cns.log` - Central nervous system coordination
- `cognition.log` (CONSOLIDATE) - Should merge into mind.log or user.log
- `hippocampus.log` - Memory consolidation operations
- `state.log` (NEW) - PersonaState energy/mood tracking
- `inbox.log` (NEW) - PersonaInbox queue operations
- `working-memory.log` (NEW) - WorkingMemory operations

### Conditional Debug Logging
Use `log.shouldLog('debug')` to avoid expensive operations when debug disabled:
```typescript
if (this.log.shouldLog('debug')) {
  this.log.debug(`Complex calculation: ${expensiveOperation()}`);
}
```

---

## CRITICAL SPAM SOURCE #1: PersonaState "Rested" Messages

**Current Behavior**:
```
[Helper AI:State] 💤 Rested: energy=1.00, attention=1.00, mood=active
[Teacher AI:State] 💤 Rested: energy=1.00, attention=1.00, mood=active
[CodeReview AI:State] 💤 Rested: energy=1.00, attention=1.00, mood=active
...repeats every 3-10 seconds for ALL 11 personas...
```

**Volume**:
- 11 personas × every 3-10 seconds = ~66-220 messages per minute
- **90%+ of all console spam**

**Location**: `system/user/server/modules/PersonaState.ts`

**Strategy**: MOVE TO FILE + CONDITIONAL DEBUG

**Destination**: `.continuum/personas/{uniqueId}/logs/state.log`

**Log Level**: DEBUG (not INFO)

**Rationale**:
- This is normal operation noise - only useful when debugging energy/mood issues
- Per-persona files allow correlating state changes with other behaviors
- Debug level means zero spam by default (LOG_LEVEL=warn)
- Can enable per-persona if investigating specific behavior

**Implementation**:
1. PersonaState already has `this.log` (SubsystemLogger instance)
2. Change `console.log` statements to `this.log.debug()`
3. Ensure SubsystemLogger configured with `logDir: 'personas/{uniqueId}/logs'`
4. Verify state.log creation with LOG_LEVEL=debug

---

## HIGH SPAM SOURCE #2: Events Daemon Subscription Spam

**Current Behavior**:
```
🎯 Events: Triggered 1 subscription(s) for data:users:created
🎯 Events: Triggered 3 subscription(s) for data:chat_messages:created
🎯 Events: Triggered 2 subscription(s) for data:rooms:updated
```

**Volume**:
- Every data operation × number of subscribers = exponential
- Example: 100 data operations × 10 subscribers = 1,000 messages

**Location**: `daemons/events-daemon/server/EventsDaemonServer.ts`

**Strategy**: CONDITIONAL DEBUG

**Destination**: Console (when LOG_LEVEL=debug) OR system.log

**Log Level**: DEBUG

**Rationale**:
- Normal operation - only interesting when debugging event flow
- High volume makes console unusable
- Needed for tracing event propagation during development

**Implementation**:
1. Add Logger instance to EventsDaemonServer
2. Replace console.log with conditional debug:
   ```typescript
   if (this.log.shouldLog('debug')) {
     this.log.debug(`Triggered ${handlers.length} subscription(s) for ${eventName}`);
   }
   ```

---

## MEDIUM SPAM SOURCE #3: Data Daemon Operations

**Current Behavior**:
```
✅ DATA SERVER: Listed 15 items from rooms via DataDaemon
🗄️ DATA SERVER: Query completed: SELECT * FROM users WHERE ...
```

**Volume**: Every query/list/create operation

**Location**: Multiple files in `daemons/data-daemon/server/`

**Strategy**: VERIFY LOGGER CONVERSION COMPLETE

**Status**: SqliteStorageAdapter, SqliteQueryExecutor, SqliteWriteManager already converted to Logger system (commit 9502fbb1), but console spam still appearing

**Investigation Needed**:
```bash
grep -r "console\.log" daemons/data-daemon/server/
```

**Destination**: `.continuum/jtag/system/logs/sql.log` (already exists)

**Rationale**:
- Logger conversion already done, but leaks remain
- Find and convert remaining console.log statements
- Should be ZERO data daemon messages in console after fix

---

## MEDIUM SPAM SOURCE #4: WebSocket Transport Logging

**Current Behavior**:
```
🔌 WebSocket client connected from ::1
📤 Sending message to client: {"type":"response",...}
```

**Volume**: Every message sent/received × number of connections

**Location**: `system/transports/websocket-transport/`

**Strategy**: ADD LOGGER (currently using console.log)

**Destination**: `.continuum/jtag/system/logs/websocket.log` (NEW)

**Log Level**:
- Connection/disconnection: INFO
- Message send/receive: DEBUG

**Rationale**:
- Critical infrastructure logs (connections) need visibility
- Message-level logs too verbose for console
- Separate file allows tracing connection issues

**Implementation**:
1. Add Logger.create('websocket', 'system') to WebSocket transport classes
2. Replace all console.log with log.info() or log.debug()
3. Connection events → INFO, Message details → DEBUG

---

## CRITICAL BUT NOT SPAM: Router Timeout Errors

**Current Behavior**:
```
⏰ JTAGRouter[server]: WARNING - Message to data/list took 5023ms (timeout: 5000ms)
❌ Error: Request timeout after 5000ms
```

**Strategy**: KEEP AS-IS (these are REAL BUGS)

**Rationale**:
- NOT spam - indicates actual performance problems
- Needs visibility in console to force fixing
- Related to SQLite bottleneck (docs/plans/bottleneck-removal.md)

**Action**: Fix underlying issues (connection pooling, worker threads) rather than hiding symptoms

---

## LOW SPAM SOURCE #5: Health Checks

**Current Behavior**:
```
✅ Health check passed
```

**Volume**: Every 30 seconds (low impact)

**Strategy**: CONDITIONAL DEBUG OR KEEP

**Rationale**:
- Low volume, not causing significant problems
- Can convert to debug level if needed
- Lower priority than high-volume spam sources

---

## CRITICAL NEW DISCOVERY #6: Cognition System Messages

**Expected Location**: `.continuum/personas/{uniqueId}/logs/user.log` or `mind.log`

**Current Behavior**: Going to CONSOLE instead

**Console Spam Examples**:
```
✅ GPT Assistant: COGNITION - Plan completed successfully with 4 steps
✅ Claude Assistant: COGNITION - Generated execution context for plan abc123
🧠 Helper AI: COGNITION - Evaluating task complexity: medium
```

**Volume**: Every cognition cycle × 11 personas = HIGH

**Location**: `system/user/server/modules/cognition/` (multiple files likely)

**Strategy**: ROUTE TO PERSONA LOGS

**Destination**: `.continuum/personas/{uniqueId}/logs/mind.log` (cognition is mind subsystem)

**Log Level**: INFO (important cognitive events) or DEBUG (detailed steps)

**Rationale**:
- Cognition is core persona behavior - belongs in persona logs
- Per-persona files allow debugging individual AI reasoning
- Console spam makes it impossible to see other issues

**Implementation Investigation Needed**:
```bash
grep -r "COGNITION" system/user/server/modules/cognition/
```
Find all cognition logging statements and verify they use SubsystemLogger for mind.log

---

## CRITICAL NEW DISCOVERY #7: CognitionLogger Output

**Expected Location**: `.continuum/personas/{uniqueId}/logs/user.log` or dedicated log

**Current Behavior**: Going to CONSOLE instead

**Console Spam Examples**:
```
✅ CognitionLogger: Logged plan completion for persona Helper AI
📝 CognitionLogger: Persisted 12 thought records to database
🗄️ CognitionLogger: Archived cognition session abc123
```

**Volume**: Every cognition logging operation

**Location**: `system/user/server/modules/cognition/CognitionLogger.ts` (likely)

**Strategy**: ROUTE TO PERSONA LOGS

**Destination**: `.continuum/personas/{uniqueId}/logs/mind.log`

**Log Level**: DEBUG (operational logging - only needed when debugging cognition storage)

**Rationale**:
- CognitionLogger is infrastructure supporting cognition - debug level appropriate
- Still persona-specific, so belongs in persona logs not system logs
- High volume when active

**Implementation**:
1. Verify CognitionLogger has access to persona uniqueId
2. Add SubsystemLogger instance targeting mind.log
3. Convert all console statements to log.debug()

---

## CRITICAL NEW DISCOVERY #8: WorkingMemory Operations

**Expected Location**: `.continuum/personas/{uniqueId}/logs/working-memory.log` or `mind.log`

**Current Behavior**: Going to CONSOLE instead

**Console Spam Examples**:
```
🗑️ [WorkingMemory] Cleared 1 thoughts from Helper AI working memory
💭 [WorkingMemory] Added thought: "Analyze user request" (priority: 0.8)
📊 [WorkingMemory] Working memory usage: 3/10 thoughts
```

**Volume**: Every thought add/clear operation × active personas

**Location**: `system/user/server/modules/cognition/WorkingMemory.ts` (likely)

**Strategy**: ROUTE TO PERSONA LOGS

**Destination**: `.continuum/personas/{uniqueId}/logs/working-memory.log` (NEW) or merge into `mind.log`

**Log Level**: DEBUG (detailed memory operations)

**Rationale**:
- Working memory is frequent, detailed cognitive operation
- Only useful when debugging specific memory issues
- Per-persona file allows tracing thought accumulation

**Implementation Options**:

**Option A**: Separate working-memory.log file
- Cleaner separation of concerns
- Easier to monitor memory-specific issues
- More files per persona

**Option B**: Merge into mind.log
- Fewer files
- All cognitive activity in one place
- Harder to filter for memory issues

**Recommendation**: Start with Option B (mind.log) - can split later if mind.log gets too large

---

## CRITICAL NEW DISCOVERY #9: PersonaInbox Messages

**Expected Location**: `.continuum/personas/{uniqueId}/logs/inbox.log` or `user.log`

**Current Behavior**: Going to CONSOLE instead

**Console Spam Examples**:
```
[DeepSeek Assistant:Inbox] 📭 Popped message (priority: 0.65, inbox size: 2, mood: active)
[Helper AI:Inbox] 📥 Enqueued message (priority: 0.45, inbox size: 3, mood: resting)
[GPT Assistant:Inbox] 🗑️ Dropped low-priority message (priority: 0.1, mood: tired)
```

**Volume**: Every message enqueue/dequeue × 11 personas = VERY HIGH

**Location**: `system/user/server/modules/PersonaInbox.ts`

**Strategy**: ROUTE TO PERSONA LOGS + CONDITIONAL DEBUG

**Destination**: `.continuum/personas/{uniqueId}/logs/inbox.log` (NEW)

**Log Level**: DEBUG (frequent operational logging)

**Rationale**:
- High volume - every message interaction generates log
- Useful for debugging inbox queue behavior
- Per-persona file allows tracking message processing per AI

**Implementation**:
1. PersonaInbox should have access to persona uniqueId
2. Add SubsystemLogger instance with logDir pointing to persona logs
3. Convert console statements to log.debug()
4. Use conditional debug: `if (this.log.shouldLog('debug'))`

---

## CRITICAL NEW DISCOVERY #10: Data Operations Related to Personas

**Expected Location**: `.continuum/jtag/system/logs/sql.log` (infrastructure) OR `.continuum/personas/{uniqueId}/logs/user.log` (if persona-initiated)

**Current Behavior**: Going to CONSOLE instead

**Console Spam Examples**:
```
🗄️ DATA SERVER: Listing cognition_plan_records entities (count: 12)
🗄️ DATA SERVER: Creating thought_record for persona Helper AI
🗄️ DATA SERVER: Querying memories table for persona Claude Assistant
```

**Volume**: Every persona data operation (thoughts, plans, memories, decisions)

**Location**: Multiple places - data daemon operations triggered by personas

**Strategy**: ALREADY CONVERTED BUT VERIFY

**Status**: Data daemon should already route to sql.log, but console spam still appearing

**Investigation Needed**:
```bash
# Find remaining console.log in data daemon
grep -r "console\.log" daemons/data-daemon/server/

# Check if cognition-related queries logging elsewhere
grep -r "cognition_plan_records\|thought_record" system/
```

**Rationale**:
- Data daemon operations should go to sql.log (already implemented)
- If persona-initiated queries appearing in console, it's a leak
- May be coming from caller (cognition modules) not data daemon

---

## CRITICAL NEW DISCOVERY #11: Misplaced Log Files

**Current Reality**: Log files appearing in WRONG locations

### Issue 11A: cognition.log Exists Somewhere

**User Report**: "there's even a cognition.log"

**Problem**: Separate cognition.log file shouldn't exist - cognition logs should go to mind.log

**Investigation Needed**:
```bash
find .continuum -name "cognition.log" -type f
```

**Root Cause**: Some cognition module likely creating its own log file instead of using SubsystemLogger

**Strategy**: CONSOLIDATE INTO mind.log

**Implementation**:
1. Find where cognition.log is being created
2. Replace with SubsystemLogger pointing to mind.log
3. Remove cognition.log file creation code

### Issue 11B: ai-decisions.log in Wrong Directory

**Unexpected Location**: `.continuum/jtag/sessions/system/00000000-0000-0000-0000-000000000000/logs/ai-decisions.log`

**Expected Location**: `.continuum/personas/{uniqueId}/logs/user.log` or dedicated decisions.log per persona

**Problem**:
- System session directory (UUID 00000000...) is wrong place for persona decisions
- Should be per-persona, not system-wide
- Session-based path is confusing (decisions aren't session-specific)

**Investigation Needed**:
```bash
grep -r "ai-decisions.log" system/
grep -r "decisions\.log" system/
```

**Strategy**: ROUTE TO PERSONA LOGS

**Destination**: `.continuum/personas/{uniqueId}/logs/decisions.log` (NEW)

**Rationale**:
- Decision-making is per-persona behavior
- Should be in persona directory for traceability
- System session path makes no sense for this

---

## Implementation Priority Order

Based on volume impact and user expectations:

### Phase 1: CRITICAL (90%+ volume reduction)
1. ✅ **PersonaState → state.log** (90% of all spam)
2. ✅ **PersonaInbox → inbox.log** (very high volume)
3. ✅ **Cognition System → mind.log** (user expectation)
4. ✅ **WorkingMemory → mind.log** (user expectation)
5. ✅ **CognitionLogger → mind.log** (user expectation)

### Phase 2: HIGH (remaining infrastructure spam)
6. **Events Daemon → conditional debug**
7. **WebSocket Transport → websocket.log**
8. **Data Daemon Leaks → sql.log** (verify existing conversion)

### Phase 3: MEDIUM (cleanup and consolidation)
9. **Consolidate cognition.log → mind.log**
10. **Move ai-decisions.log → per-persona decisions.log**
11. **Health Checks → conditional debug** (low priority)

---

## Decision Matrix: Move to File vs Conditional Debug vs Keep

| Spam Source | Current Location | Strategy | New Location | Log Level | Rationale |
|-------------|------------------|----------|--------------|-----------|-----------|
| PersonaState "Rested" | Console | MOVE + DEBUG | `personas/{uniqueId}/logs/state.log` | DEBUG | 90% volume, per-persona behavior |
| PersonaInbox | Console | MOVE + DEBUG | `personas/{uniqueId}/logs/inbox.log` | DEBUG | Very high volume, per-persona queue |
| Cognition System | Console | MOVE | `personas/{uniqueId}/logs/mind.log` | INFO/DEBUG | User expectation, core behavior |
| WorkingMemory | Console | MOVE + DEBUG | `personas/{uniqueId}/logs/mind.log` | DEBUG | Frequent operations, cognitive detail |
| CognitionLogger | Console | MOVE + DEBUG | `personas/{uniqueId}/logs/mind.log` | DEBUG | Operational logging |
| cognition.log file | Wrong location | CONSOLIDATE | `personas/{uniqueId}/logs/mind.log` | - | Merge into proper location |
| ai-decisions.log | System session | RELOCATE | `personas/{uniqueId}/logs/decisions.log` | INFO | Per-persona, not system-wide |
| Events Daemon | Console | CONDITIONAL | Console (debug) or system.log | DEBUG | Infrastructure, debug-only |
| WebSocket Transport | Console | MOVE | `websocket.log` | INFO/DEBUG | Infrastructure, connection tracing |
| Data Daemon Leaks | Console | VERIFY FIX | `sql.log` | DEBUG | Already converted, find remaining |
| Router Timeouts | Console | KEEP | Console | ERROR | Real bugs, need visibility |
| Health Checks | Console | CONDITIONAL | Console (debug) | DEBUG | Low volume, low priority |

---

## Verification Commands (After Implementation)

### Test Console Silence (Default LOG_LEVEL=warn)
```bash
npm start
# Wait 30 seconds
tail -100 .continuum/jtag/system/logs/npm-start.log | grep -i "rested\|enqueued\|cognition\|working"
# Should return ZERO results
```

### Test Persona Logs Populated
```bash
# Find a persona uniqueId
./jtag data/list --collection=users --filter='{"type":"persona"}' --limit=1

# Check logs for that persona
ls -lh .continuum/personas/helper-ai-12345678/logs/
# Should show: user.log, mind.log, body.log, soul.log, cns.log, state.log, inbox.log

# Verify content
tail -50 .continuum/personas/helper-ai-12345678/logs/mind.log
# Should show cognition operations

tail -50 .continuum/personas/helper-ai-12345678/logs/state.log
# Should show energy/mood updates (if LOG_LEVEL=debug)
```

### Test Debug Level Verbosity
```bash
# Set debug level
export LOG_LEVEL=debug
npm start
# Wait 30 seconds

# Check state.log populated
tail -50 .continuum/personas/helper-ai-12345678/logs/state.log
# Should show detailed state updates

# Check inbox.log populated
tail -50 .continuum/personas/helper-ai-12345678/logs/inbox.log
# Should show message enqueue/dequeue operations
```

### Verify Misplaced Files Gone
```bash
# cognition.log should not exist
find .continuum -name "cognition.log" -type f
# Should return: (nothing)

# ai-decisions.log should not be in system session
find .continuum/jtag/sessions/system -name "ai-decisions.log"
# Should return: (nothing)

# ai-decisions.log should be per-persona (if decisions system active)
find .continuum/personas -name "decisions.log"
# Should return: personas/{uniqueId}/logs/decisions.log (multiple)
```

---

## Success Criteria

### Console (LOG_LEVEL=warn, default)
- ✅ Zero PersonaState messages
- ✅ Zero PersonaInbox messages
- ✅ Zero Cognition system messages
- ✅ Zero WorkingMemory messages
- ✅ Zero CognitionLogger messages
- ✅ Zero Events daemon subscription messages
- ✅ Zero Data daemon query messages
- ✅ Zero WebSocket message-level logs
- ⚠️ Router timeout errors STILL VISIBLE (bugs, not spam)

### Persona Logs (per uniqueId)
- ✅ user.log contains initialization + major lifecycle events
- ✅ mind.log contains cognition + WorkingMemory + CognitionLogger operations
- ✅ state.log contains energy/mood updates (when LOG_LEVEL=debug)
- ✅ inbox.log contains message queue operations (when LOG_LEVEL=debug)
- ✅ decisions.log contains decision-making events (when decisions system active)

### System Logs (infrastructure)
- ✅ system.log contains daemon initialization + critical events
- ✅ sql.log contains ALL database operations
- ✅ websocket.log contains connection events + message details (debug)
- ✅ tools.log contains PersonaToolExecutor operations (already implemented)

### Misplaced Files
- ✅ cognition.log does NOT exist anywhere
- ✅ ai-decisions.log NOT in system session directory
- ✅ All persona-specific logs in `.continuum/personas/{uniqueId}/logs/`

### Volume Reduction
- ✅ 95%+ reduction in console message count
- ✅ npm-start.log readable (only warnings/errors/critical events)
- ✅ Per-persona logs enable debugging individual AI behavior

---

## File Summary Needed for Each Source

When implementing, document:
- **File path** where console.log exists
- **Line numbers** of console statements
- **Message format** (what's being logged)
- **Logger instance** to use (create new or use existing this.log)
- **Log category** (system, sql, persona-specific)
- **Log level** (info, warn, error, debug)
- **Destination file** after conversion

Example format:
```markdown
### PersonaState.ts (system/user/server/modules/PersonaState.ts)

**Lines to convert**: 125, 147, 203

**Current**:
```typescript
console.log(`[${this.persona.displayName}:State] 💤 Rested: energy=${this.energy.toFixed(2)}`);
```

**After**:
```typescript
this.log.debug(`Rested: energy=${this.energy.toFixed(2)}, attention=${this.attention.toFixed(2)}, mood=${this.mood}`);
```

**Logger**: Already has `this.log` (SubsystemLogger)
**Category**: persona-specific (state)
**Destination**: `.continuum/personas/{uniqueId}/logs/state.log`
**Level**: DEBUG
```

---

## End of Strategy Document

**Status**: Analysis complete, ready for implementation approval
**Next Step**: User approval → Begin Phase 1 implementation
**Estimated Impact**: 95%+ console spam reduction in Phase 1

