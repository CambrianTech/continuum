# Complete Implementation Plan - 2025-10-12
**Goal**: Fix ALL identified issues today
**Status**: 3 phases completed (Ghost Users ✅, Role Assignment ✅, [QUESTION] Markers ✅)

---

## Implementation Phases

### Phase 1: Ghost Users Fix ✅ COMPLETED AND COMMITTED
**Status**: ✅ Fully implemented, tested, verified, and committed (5b74eebf)

**Files Changed**:
- ✅ Created `system/user/shared/UserIdentityResolver.ts` (375 lines)
- ✅ Updated `daemons/session-daemon/server/SessionDaemonServer.ts`
- ✅ Fixed `system/core/detection/AgentDetector.ts` (type errors + detection logic)

**Testing Results**:
```bash
# ✅ Verified stable uniqueId: "claude-code" (not cli-*)
./jtag session/create --isShared=true
# Result: uniqueId="claude-code", displayName="Claude Code", type="agent"

# ✅ Verified no duplicate Claude Code users
./jtag data/list --collection=users --format=json | jq -r '.items[] | select(.displayName == "Claude Code")'
# Result: Only ONE Claude Code user with stable uniqueId

# ✅ Verified AgentDetector properly detects Claude Code
echo "CLAUDECODE=$CLAUDECODE CLAUDE_CODE_ENTRYPOINT=$CLAUDE_CODE_ENTRYPOINT"
# Result: CLAUDECODE=1 CLAUDE_CODE_ENTRYPOINT=cli (detected with 95% confidence)
```

**Commit**: 5b74eebf - "Fix Ghost Users: Implement UserIdentityResolver with stable uniqueIds"

---

### Phase 2: Ollama Request Queue ⏳ NEXT
**Priority**: CRITICAL (fixes 67% timeout rate)
**Estimated Time**: 30 minutes

**Implementation**:
1. Create `OllamaRequestQueue` class in `OllamaAdapter.ts`
2. Add queue to OllamaAdapter constructor
3. Wrap all `makeRequest()` calls with queue.enqueue()
4. Test with concurrent requests

**Code Location**: `daemons/ai-provider-daemon/shared/OllamaAdapter.ts`

**Testing**:
```bash
# Trigger 3 concurrent persona responses
# Monitor Ollama logs for queue behavior
./jtag debug/logs --filterPattern="Ollama|queue" --tailLines=50
```

---

### Phase 3: RAG Temporal Filtering ⏳
**Priority**: CRITICAL (prevents future responses in context)
**Estimated Time**: 20 minutes

**Implementation**:
1. Add `triggerTimestamp?: Date` to `RAGBuildOptions` interface
2. Update `ChatRAGBuilder.buildContext()` line 184 to filter by timestamp
3. Update PersonaUser calls to pass `triggerTimestamp: message.timestamp`

**Code Locations**:
- `system/rag/builders/ChatRAGBuilder.ts:184`
- `system/user/server/PersonaUser.ts:372, 1033`

**Testing**:
```bash
# Send message, verify RAG only includes messages BEFORE trigger
./jtag debug/logs --filterPattern="RAG|buildContext" --tailLines=50
```

---

### Phase 4: Response Validation (BoW) ⏳
**Priority**: HIGH (prevents gibberish storage)
**Estimated Time**: 40 minutes

**Implementation**:
1. Create `validateResponseQuality()` method in PersonaUser
2. Add BoW scoring (hallucinated prefixes, gibberish detection)
3. Call BEFORE storing message (line 491)
4. Discard catastrophic responses, clean up minor issues

**Code Location**: `system/user/server/PersonaUser.ts:483-491`

**Testing**:
```bash
# Trigger persona response, check validation logs
./jtag debug/logs --filterPattern="validation|quality" --tailLines=30
```

---

### Phase 5: Role Assignment Fix ✅ COMPLETED
**Priority**: MEDIUM (improves RAG quality)
**Estimated Time**: 10 minutes (actual: 5 minutes)

**Implementation**:
1. ✅ Updated `ChatRAGBuilder.ts:214-215` to check `msg.senderType`
2. ✅ Mark persona/agent/system messages as 'assistant' role
3. ✅ Mark human messages as 'user' role

**Code Changes**:
- Changed from `isOwnMessage ? 'assistant' : 'user'` to checking `msg.senderType`
- Now properly assigns roles based on sender type, not ownership

**Code Location**: `system/rag/builders/ChatRAGBuilder.ts:214-215`

**Testing**:
```bash
# Verify roles in RAG context
./jtag debug/logs --filterPattern="role.*assistant|role.*user" --tailLines=50
```

**Status**: Deployed in version 1.0.2988

---

### Phase 6: System Prompt Rewrite ⏳
**Priority**: MEDIUM (reduces confusion for small models)
**Estimated Time**: 30 minutes

**Implementation**:
1. Remove "DO NOT" instructions from system prompt
2. Add human vs AI distinction
3. Make model-dependent (small/medium/large)
4. Fix mentions of non-existent formats

**Code Location**: `system/rag/builders/ChatRAGBuilder.ts:158-169`

**Testing**:
```bash
# Trigger persona responses, check for hallucinated prefixes
./jtag debug/logs --filterPattern="Helper AI:|Teacher AI:" --tailLines=50
```

---

### Phase 7: Remove [QUESTION] Markers ✅ COMPLETED
**Priority**: LOW (minor noise reduction)
**Estimated Time**: 5 minutes (actual: 3 minutes)

**Implementation**:
1. ✅ Removed question detection logic (lines 212-217)
2. ✅ Removed `[QUESTION]` marker from message content
3. ✅ Updated content reference from `markedContent` to `messageText`

**Code Changes**:
- Removed `isQuestion` detection logic
- Removed `markedContent` variable with `[QUESTION]` prefix
- Changed `llmMessage.content` to use plain `messageText`

**Code Location**: `system/rag/builders/ChatRAGBuilder.ts:207-226`

**Testing**:
```bash
# Verify markers removed from RAG content
./jtag debug/logs --filterPattern="QUESTION" --tailLines=30
# Should show no [QUESTION] markers in new RAG contexts
```

**Status**: Deployed in version 1.0.2988

---

## Total Estimated Time
- Phase 1 (Ghost Users): ✅ 45 minutes (completed)
- Phase 2 (Ollama Queue): ⏳ 30 minutes
- Phase 3 (RAG Temporal): ⏳ 20 minutes
- Phase 4 (Response Validation): ⏳ 40 minutes
- Phase 5 (Role Assignment): ⏳ 10 minutes
- Phase 6 (System Prompt): ⏳ 30 minutes
- Phase 7 ([QUESTION] Markers): ⏳ 5 minutes

**Total**: ~3 hours for all fixes

---

## Testing After All Fixes

### End-to-End Test
```bash
# 1. Deploy all fixes
npm start

# 2. Clear database and reseed
npm run data:reseed

# 3. Trigger multi-persona response
# Post question in general chat

# 4. Verify metrics
./jtag debug/logs --filterPattern="LLM call|timeout|validation" --tailLines=100

# 5. Check user list
./jtag data/list --collection=users

# 6. Verify no gibberish
./jtag data/list --collection=chat_messages --format=json | grep -i "@@@"
```

### Expected Results
- ✅ 6 users (no ghosts)
- ✅ 2-3 LLM calls per question (not 19+)
- ✅ ~10% timeout rate (not 67%)
- ✅ No gibberish messages stored
- ✅ Proper role assignment in RAG
- ✅ No hallucinated prefixes

---

## Rollback Plan

If any phase breaks the system:

```bash
# 1. Check git status
git status

# 2. Revert specific file
git checkout HEAD -- path/to/file.ts

# 3. Redeploy
npm start

# 4. Verify system works
./jtag ping
```

---

## Success Criteria

### Phase 1 (Ghost Users) ✅
- [x] No duplicate "Claude Code" users
- [x] No "Human Terminal User" ghosts
- [x] Stable uniqueId ("claude-code" not "cli-*")

### Phase 2 (Ollama Queue) ⏳
- [ ] Timeout rate drops from 67% to ~10%
- [ ] No cascade failures
- [ ] Queue logs show proper serialization

### Phase 3 (RAG Temporal) ⏳
- [ ] RAG contexts don't include future messages
- [ ] Personas don't see each other's responses in their context

### Phase 4 (Response Validation) ⏳
- [ ] Gibberish messages discarded
- [ ] Hallucinated prefixes stripped
- [ ] Validation logs show quality scores

### Phase 5 (Role Assignment) ⏳
- [ ] AI messages marked as 'assistant'
- [ ] Human messages marked as 'user'

### Phase 6 (System Prompt) ⏳
- [ ] No hallucinated prefixes in responses
- [ ] Responses stay on topic

### Phase 7 ([QUESTION] Markers) ⏳
- [ ] No [QUESTION] markers in RAG content

---

## Current Status

**Time**: Starting Phase 2 after Ghost Users verification
**Next**: Implement OllamaRequestQueue
**Blocked**: None
**Risk**: None - each phase is isolated and testable
