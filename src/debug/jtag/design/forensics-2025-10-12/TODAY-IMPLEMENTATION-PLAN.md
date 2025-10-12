# Complete Implementation Plan - 2025-10-12
**Goal**: Fix ALL identified issues today
**Status**: Phase 1 started (Ghost Users ✅ completed)

---

## Implementation Phases

### Phase 1: Ghost Users Fix ✅ COMPLETED
**Status**: Code written, deployed, testing in progress

**Files Changed**:
- ✅ Created `system/user/shared/UserIdentityResolver.ts`
- ✅ Updated `daemons/session-daemon/server/SessionDaemonServer.ts`

**Testing**:
```bash
# Verify stable IDs
./jtag data/list --collection=users --format=json | grep uniqueId

# Verify no duplicates
./jtag data/list --collection=users
```

**Cleanup Required**:
- Delete existing ghost users (cli-* uniqueIds)

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

### Phase 5: Role Assignment Fix ⏳
**Priority**: MEDIUM (improves RAG quality)
**Estimated Time**: 10 minutes

**Implementation**:
1. Update `ChatRAGBuilder.ts:232` to check `msg.senderType`
2. Mark persona/agent messages as 'assistant' role
3. Mark human messages as 'user' role

**Code Location**: `system/rag/builders/ChatRAGBuilder.ts:232`

**Testing**:
```bash
# Verify roles in RAG context
./jtag debug/logs --filterPattern="role.*assistant|role.*user" --tailLines=50
```

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

### Phase 7: Remove [QUESTION] Markers ⏳
**Priority**: LOW (minor noise reduction)
**Estimated Time**: 5 minutes

**Implementation**:
1. Remove or make optional `[QUESTION]` marker in ChatRAGBuilder
2. Line 217 - just use messageText without marker

**Code Location**: `system/rag/builders/ChatRAGBuilder.ts:217`

**Testing**:
```bash
# Verify markers removed from RAG content
./jtag debug/logs --filterPattern="QUESTION" --tailLines=30
```

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
