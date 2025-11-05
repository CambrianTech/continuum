# Ghost Users Issue - 2025-10-12
**Problem**: Duplicate/ghost users appearing in user list
**Status**: Investigation - need to find user creation logic

---

## Evidence from Database

### Current Users (8 total, should be 6)

**Expected Users** (from seeding):
1. ✅ Developer (human) - `uniqueId: "primary-human"`
2. ✅ Helper AI (persona) - `uniqueId: "persona-helper-001"`
3. ✅ Teacher AI (persona) - `uniqueId: "persona-teacher-001"`
4. ✅ CodeReview AI (persona) - `uniqueId: "persona-codereview-001"`
5. ✅ Claude Code (agent) - `uniqueId: "claude-code"`
6. ✅ GeneralAI (agent) - `uniqueId: "general-ai"`

**Ghost Users** (should not exist):
7. ❌ **Claude Code** (agent) - DUPLICATE
   - `uniqueId: "cli-1760293646502-zl546gu0"`
   - `displayName: "Claude Code"`
   - `type: "agent"`
   - Created: During CLI session (timestamp in uniqueId)

8. ❌ **Human Terminal User** (agent) - MYSTERY
   - `uniqueId: "cli-1760289628051-ut1k071j"`
   - `displayName: "Human Terminal User"`
   - `type: "agent"` (WRONG TYPE)
   - Created: During CLI session (earlier timestamp)

---

## Analysis

### Issue #1: Duplicate Claude Code

**uniqueId pattern**: `cli-{timestamp}-{random}`

This suggests CLI commands are creating new agent users instead of using the existing seeded agent.

**Hypothesis**: When running `./jtag` commands, system creates a new "Claude Code" agent user instead of reusing the existing one.

**Need to investigate**:
- Where does CLI create user entities?
- Why isn't it finding the existing "claude-code" agent?
- Look for user creation in CLI initialization

### Issue #2: "Human Terminal User" Ghost

**Problems**:
1. Wrong type: "agent" (should be "human" if anything)
2. Confusing name: "Human Terminal User" (not descriptive)
3. Shouldn't exist: CLI should use existing "Developer" or "Claude Code"

**Hypothesis**: CLI might be creating a user for the terminal session, but:
- Creating it as "agent" type (wrong)
- Giving it a confusing name
- Not cleaning it up after session ends

---

## User Display in Widget

From your screenshot, the widget shows:

```
Users & Agents (8)
├── Claude Code (Agent) ⚫ offline
├── Human Terminal User (Agent) ⚫ offline  ← GHOST
├── CodeReview AI (Persona) 🟢 online
├── Teacher AI (Persona) 🟢 online
├── Helper AI (Persona) 🟢 online
├── GeneralAI (Agent) ⚫ offline
├── Claude Code (Agent) ⚫ offline  ← DUPLICATE
└── Developer (Human) 🟢 online
```

**Expected display**:
```
Users & Agents (6)
├── Developer (Human) 🟢 online
├── Claude Code (Agent) ⚫ offline
├── GeneralAI (Agent) ⚫ offline
├── Helper AI (Persona) 🟢 online
├── Teacher AI (Persona) 🟢 online
└── CodeReview AI (Persona) 🟢 online
```

---

## Root Cause Investigation Needed

### 1. Find CLI user creation logic

**Search for**:
- `uniqueId: "cli-` pattern
- User creation with timestamp in uniqueId
- CLI initialization code

**Possible locations**:
- `system/user/` - User entity creation
- CLI command initialization
- Session creation logic

### 2. Find "Human Terminal User" creation

**Search for**:
- String "Human Terminal User"
- User creation with type "agent" during CLI sessions

### 3. Check user cleanup logic

**Questions**:
- Are CLI users supposed to be temporary?
- Should they be cleaned up after session ends?
- Why are they persisting in database?

---

## Hypothesis: CLI Creates Session Users

**Theory**: When running `./jtag` commands:

1. **CLI session starts**
2. **System doesn't find existing user** (lookup by session ID fails?)
3. **Creates new "agent" user** with `cli-{timestamp}-{random}` uniqueId
4. **Session ends** but user persists in database
5. **Next session creates another** → accumulation of ghost users

**This would explain**:
- Multiple "Claude Code" entries (one per CLI session)
- "Human Terminal User" (maybe from a previous session type?)
- `cli-{timestamp}` pattern in uniqueId

---

## Files to Read

### Priority 1: User Creation
```bash
grep -rn "cli-" system/user/
grep -rn "Human Terminal User" system/
grep -rn "uniqueId.*timestamp" system/
```

### Priority 2: CLI Initialization
```bash
# Look for how jtag command initializes user context
grep -rn "createUser" commands/
grep -rn "UserEntity" system/core/
```

### Priority 3: Session Management
```bash
grep -rn "session.*user" system/
grep -rn "agent.*type" system/user/
```

---

## Impact

### Current Impact: MEDIUM
- Clutters user list (8 users instead of 6)
- Confusing for development (which "Claude Code" is which?)
- May indicate session/user management bug

### Potential Impact: HIGH
- If users accumulate over time → database bloat
- May cause confusion in room membership
- May affect event routing (if events go to ghost users)

---

## Recommended Fix

### Short-term:
1. **Manual cleanup**: Delete ghost users from database
   ```bash
   ./jtag data/delete --collection=users --id={ghost-user-id}
   ```

2. **Document expected users**: Add to seeding documentation

### Long-term:
1. **Find root cause**: Read CLI user creation code
2. **Fix lookup logic**: Ensure CLI reuses existing "Claude Code" agent
3. **Add cleanup**: Remove temporary CLI users after session ends
4. **Add validation**: Prevent duplicate users with same displayName

---

## ROOT CAUSE FOUND ✅

### Location: `daemons/session-daemon/server/SessionDaemonServer.ts:454`

**The Problem**:
```typescript
// Generate synthetic connectionContext for ephemeral clients (CLI, etc.)
uniqueId = `cli-${Date.now()}-${Math.random().toString(36).substring(2, 10)}`;
```

Every CLI connection was generating a **NEW random uniqueId** with timestamp, which meant:
- CLI couldn't find existing "Claude Code" user (different uniqueId every time)
- Created duplicate "Claude Code" with `cli-{timestamp}-{random}` uniqueId
- "Human Terminal User" was fallback detection when Claude wasn't detected

### The Fix: UserIdentityResolver ✅

Created `system/user/shared/UserIdentityResolver.ts`:

**What it does**:
1. **Uses AgentDetector** to identify who/what is connecting (Claude Code, human, CI, etc.)
2. **Generates stable uniqueId** based on agent type:
   - `"claude-code"` for Claude Code (CONSTANT across sessions)
   - `"primary-human"` for human developers
   - `"ci-{platform}"` for CI systems
3. **Looks up existing user BEFORE creating** (prevent duplicates!)
4. **Returns resolved identity** with proper type, displayName, bio, avatar

**Updated flow in SessionDaemonServer**:
```typescript
private async createUser(params: CreateSessionParams): Promise<BaseUser> {
  // STEP 1: Detect and resolve identity
  const resolvedIdentity = await UserIdentityResolver.resolve();

  // STEP 2: If user exists, return it (NO GHOST USERS!)
  if (resolvedIdentity.exists && resolvedIdentity.userId) {
    return await this.getUserById(resolvedIdentity.userId);
  }

  // STEP 3: Create new user with stable uniqueId
  const createParams: UserCreateParams = {
    type: resolvedIdentity.type,
    displayName: resolvedIdentity.displayName,
    uniqueId: resolvedIdentity.uniqueId, // Stable!
    // ...
  };
  return await UserFactory.create(createParams, this.context, this.router);
}
```

### Expected Behavior After Fix

**Before Fix**:
- Session 1: Creates "Claude Code" with `uniqueId: "cli-1760293646502-zl546gu0"`
- Session 2: Creates ANOTHER "Claude Code" with `uniqueId: "cli-1760293999123-abc123xyz"`
- Result: 2+ duplicate "Claude Code" users

**After Fix**:
- Session 1: Creates "Claude Code" with `uniqueId: "claude-code"`
- Session 2: Finds existing "Claude Code" with `uniqueId: "claude-code"` → REUSES IT
- Session 3+: All reuse same "Claude Code" user
- Result: 1 "Claude Code" user (stable identity)

## Next Steps

1. ✅ Found root cause in SessionDaemonServer.ts:454
2. ✅ Created UserIdentityResolver with AgentDetector integration
3. ✅ Updated SessionDaemonServer to use UserIdentityResolver
4. ⏳ Deploy and test (npm start)
5. ⏳ Verify only 6 users exist (no ghosts)
6. ⏳ Delete existing ghost users from database

---

## Related to Other Issues?

**Possibly related to**:
- Session management
- User daemon initialization
- CLI vs browser user handling

**Not related to**:
- Persona responses (ghost users are offline)
- RAG building (ghost users not in conversation)
- Ollama concurrency (ghost users don't use LLM)
