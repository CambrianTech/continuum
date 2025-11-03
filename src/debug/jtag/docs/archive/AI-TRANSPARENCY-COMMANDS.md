# AI Transparency Commands - Full Inspection at Every Step

## Problem Statement

**RAG IS EVERYTHING.** If it's not in the RAG, the AI doesn't know it exists.

Every AI decision depends ENTIRELY on the RAG context they receive. We need commands to inspect:
1. What RAG did they get?
2. What decision did they make?
3. What did they generate?
4. Can we test their inference ourselves?

## Command Enhancements

### 1. `rag/inspect` - See what an AI sees

**Enhanced to accept defaults:**
```bash
# Uses first persona + first room by default
./jtag rag/inspect

# Specific persona and room by ID
./jtag rag/inspect --personaId=<UUID> --roomId=<UUID>

# With options
./jtag rag/inspect --maxMessages=50 --verbose
```

**What it shows:**
- Persona identity (name, bio, system prompt)
- Room members list
- Conversation history (last N messages)
- Artifacts (images, files)
- Private memories
- Token count estimate
- Warnings (empty history, missing context, etc.)

**Implementation:**
- `commands/rag/inspect/shared/RAGInspectTypes.ts` - Made roomId and personaId optional
- `commands/rag/inspect/server/RAGInspectServerCommand.ts` - Added getFirstRoom() and getFirstPersona() defaults

### 2. `ai/logs` - See AI decision history

**Already exists, needs no changes:**
```bash
# See all AI decisions
./jtag ai/logs --tailLines=50

# Filter by AI name
./jtag ai/logs --filterPattern="Grok"

# Show only errors
./jtag ai/logs --includeErrorsOnly=true
```

### 3. `ai/generate` - Test AI inference yourself

**Already supports what we need:**
```bash
# Generate with RAG context (uses room history)
./jtag ai/generate --personaId=<UUID> --roomId=<UUID>

# Preview mode - see RAG without calling LLM
./jtag ai/generate --personaId=<UUID> --roomId=<UUID> --preview=true

# Direct message mode (no RAG)
./jtag ai/generate --messages='[{"role":"user","content":"test"}]' --model="llama3.2:3b"
```

### 4. `ai/should-respond` - Test gating decision

**Already exists:**
```bash
# Test if AI would respond to a message
./jtag ai/should-respond --personaId=<UUID> --roomId=<UUID> --message="test question"
```

### 5. `ai/validate-response` - Test post-generation validation

**NEW COMMAND (just created):**
```bash
# Validate if a response answers the question
./jtag ai/validate-response \
  --generatedResponse="Python was invented by Guido van Rossum" \
  --originalQuestion="Who invented Python?" \
  --questionSender="Joel" \
  --verbose=true
```

Returns: `SUBMIT` | `CLARIFY` | `SILENT`

### 6. `ai/report` - Performance metrics

**Already exists:**
```bash
# Overall AI coordination metrics
./jtag ai/report
```

## Full Transparency Workflow

### Test Grok's Complete Pipeline:

```bash
# 1. See what RAG Grok gets (no inference)
./jtag ai/generate --personaId=<GROK_ID> --roomId=general --preview=true

# 2. Test "should I respond?" decision
./jtag ai/should-respond --personaId=<GROK_ID> --roomId=general --message="What year did Rome fall?"

# 3. Generate Grok's response
./jtag ai/generate --personaId=<GROK_ID> --roomId=general

# 4. Validate if response is relevant
./jtag ai/validate-response \
  --generatedResponse="<GROK_RESPONSE>" \
  --originalQuestion="What year did Rome fall?" \
  --questionSender="Joel"

# 5. See Grok's decision history
./jtag ai/logs --filterPattern="Grok" --tailLines=20

# 6. See overall metrics
./jtag ai/report
```

## UUID Validation Fix (2025-10-18)

### Problem Discovered
During RAG inspection testing, I used a malformed UUID and the system accepted it silently, returning empty results instead of throwing an error. This revealed a critical architectural flaw:

**Malformed UUID used**: `5e71a0c8-0303-4eb8-a478-3a121248bbae` (actually valid - I was confused)
**Real issue**: Type system defined `UUID` as just `string` with no runtime enforcement

### Solution Implemented
Added `isValidUUID()` validation to all DataDaemon CRUD operations:

**Files Modified**:
- `daemons/data-daemon/shared/DataDaemon.ts` (lines 14, 151-153, 200-202, 278-280, 309-311, 732-734, 756-758, 781-783)

**Validation Added To**:
1. `DataDaemon.read()` - Static and instance methods
2. `DataDaemon.update()` - Static and instance methods
3. `DataDaemon.remove()` / `delete()` - Static and instance methods
4. `DataDaemon.create()` - Instance method (if ID provided)

**Error Message Format**:
```
Invalid UUID format: "{id}" - must be RFC 4122 UUIDv4 (xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx)
```

**Testing Confirmed**:
```bash
# Malformed UUIDs now throw clear errors
./jtag data/read --collection=Room --id=notauuid
./jtag data/read --collection=Room --id=5e71a0c8-SHORT
./jtag data/update --collection=Room --id=bad-uuid-format --data='{"name":"test"}'

# All return: "Invalid UUID format: ... - must be RFC 4122 UUIDv4"
```

**Architectural Impact**:
- ✅ No more silent failures from malformed UUIDs
- ✅ Consistent error messages across all data operations
- ✅ Rust-like type safety with runtime enforcement
- ✅ Fail-fast debugging - catches mistakes immediately

## What's Missing

### Problem: Commands require UUIDs, not friendly names

**Current (painful):**
```bash
./jtag rag/inspect --personaId=27cf3e63-f6bb-449e-b59d-7143f02a2a3a --roomId=5e71a0c8-...
```

**Desired (easy):**
```bash
./jtag rag/inspect --persona=Grok --room=general
```

**Solution:** Add name resolution to all AI commands
- Query users by displayName or uniqueId
- Query rooms by name or uniqueId
- Fall back to UUID if not found

### Next Steps

1. ✅ Enhanced `rag/inspect` with smart defaults
2. ✅ Fixed `ai/validate-response` compilation errors
3. ✅ **CRITICAL FIX**: Added UUID validation to DataDaemon - now enforces RFC 4122 UUIDv4 format
4. ⏳ Add name resolution to all AI commands
5. ⏳ Test full transparency workflow
6. ⏳ Verify RAG contains everything AIs need

## Critical RAG Requirements

**What MUST be in RAG for AIs to function:**

1. ✅ Recent conversation history (messages with timestamps)
2. ✅ Room members list (who's here)
3. ✅ Persona identity (who am I)
4. ✅ System prompt (what's my purpose)
5. ❓ Room metadata (room name, topic, description) - CHECK THIS
6. ❓ Sender details (who is asking) - CHECK THIS
7. ❓ Current timestamp (when is this) - CHECK THIS
8. ❓ Active responders (who else is generating) - NOT IMPLEMENTED

**Next: Verify what's actually in the RAG by testing rag/inspect after deployment**
