# Anonymization Plan: Remove Hardcoded Personal Data

**Date**: 2025-10-21
**Purpose**: Replace hardcoded "Joel" references with dynamic/anonymous identifiers

## Philosophy

Per Joel's guidance:
- Use home directory name as default uniqueId for first user (like how Claude data identifies Claude)
- Call things "anonymous", "human", or "developer" - not "joel"
- Be clever, not hardcoded
- Principle: Don't leak private data even if minor

## Files Requiring Updates

### 1. Data Seeding System (api/data-seed/)

**api/data-seed/seed-users-orm.ts**
```typescript
// CURRENT:
const joelData = {
  displayName: "Joel",
  email: "joel@continuum.dev",
}

// PROPOSED:
const defaultHumanData = {
  displayName: process.env.USER || process.env.USERNAME || "Developer",
  email: `${(process.env.USER || 'developer').toLowerCase()}@continuum.dev`,
}
```

**api/data-seed/RoomDataSeed.ts**
```typescript
// CURRENT (lines 146, 164, 182):
welcome.senderName = 'Joel';

// PROPOSED:
welcome.senderName = process.env.USER || process.env.USERNAME || 'Developer';
```

**api/data-seed/DataSeeder.ts**
- Lines 10, 110, 260: Update console.log messages
- Replace "joel" with "default human user" or "system owner"

---

### 2. Fake/Seed Data Files (data/)

**data/fake-users.json**
```json
// CURRENT:
{
  "userId": "user-joel-12345",
  "displayName": "Joel",
  "email": "joel@example.com"
}

// PROPOSED:
{
  "userId": "user-human-12345",
  "displayName": "Human",
  "email": "human@example.com"
}
```

**data/seed-data.json**
- Replace `joel-human-12345` → `user-human-default`
- Replace `joel-session-67890` → `session-human-default`
- Replace `joel@continuum.dev` → `human@continuum.dev`

**data/seed/currentData.json**
- Same replacements as seed-data.json
- Replace `senderId: "user-joel-12345"` → `"user-human-default"`

**data/seed/currentData.ts**
- Same as currentData.json

**data/seed/generatedSeedData.json**
- Same replacements

**data/seed/generatedSeedData.ts**
- Same replacements

**data/seed/seedData.ts**
- Replace `displayName: 'Joel'` → `'Human'`

**data/seed/users.ts**
- Replace `displayName: 'Joel'` → `'Human'`

---

### 3. Session Type Documentation

**daemons/session-daemon/shared/SessionTypes.ts:17**
```typescript
// CURRENT:
displayName: string; // "Claude", "Joel", etc. - passed from connect()

// PROPOSED:
displayName: string; // "Claude", "Human", etc. - passed from connect()
```

---

### 4. Documentation Files (*.md)

**Strategy**: Most documentation uses "Joel" as an example name in architecture discussions. These should be updated to use generic names.

**Replacement Strategy**:
- Architecture examples: `Joel` → `Human` or `Developer`
- Example user IDs: `user-joel-12345` → `user-human-12345`
- Example emails: `joel@continuum.dev` → `human@continuum.dev`
- Keep quoted conversations/testimonials as-is (historical context)

**Files to Update**:
- `ACADEMY_ARCHITECTURE.md`
- `AI-HUMAN-USER-INTEGRATION.md`
- `AI-TRANSPARENCY-COMMANDS.md`
- `CRUD-EVENT-TEST-ARCHITECTURE.md`
- `EVENTS_UNIFICATION_PLAN.md`
- `RECIPE-SYSTEM-REQUIREMENTS.md`
- `USER_CREATION_DESIGN.md`
- `api/data-seed/README.md`
- And ~40 other .md files (full list in /tmp/joel-refs.txt)

---

## Implementation Strategy

### Phase 1: Data Seeding System (CRITICAL - Affects Runtime)
1. Update `api/data-seed/seed-users-orm.ts` to use environment variables
2. Update `api/data-seed/RoomDataSeed.ts` to use dynamic names
3. Update `api/data-seed/DataSeeder.ts` console logs
4. Test: `npm start` and verify default user created with system username

### Phase 2: Static Seed Data (Test Data)
1. Update all `data/` JSON files with anonymous IDs
2. Update all `data/seed/` TypeScript files
3. Test: Verify seed data loads correctly

### Phase 3: Type Documentation (Minor)
1. Update `SessionTypes.ts` comment
2. Update any other type definition comments

### Phase 4: Documentation (Low Priority)
1. Bulk replace in documentation files
2. Preserve historical quotes/testimonials
3. Focus on architecture examples and code snippets

---

## Dynamic User Creation Pattern

**Proposed Pattern** (matches Joel's guidance):
```typescript
// Get system username like how we identify Claude
const getDefaultUsername = (): string => {
  return process.env.USER || process.env.USERNAME || 'developer';
};

const getDefaultUserId = (): string => {
  const username = getDefaultUsername();
  return `user-${username}-${Date.now()}`;
};

// Usage in seed-users-orm.ts:
const defaultHumanData = {
  id: getDefaultUserId(),
  displayName: getDefaultUsername().charAt(0).toUpperCase() + getDefaultUsername().slice(1),
  email: `${getDefaultUsername().toLowerCase()}@continuum.dev`,
  userType: 'human' as const,
  // ... rest of user data
};
```

---

## Testing Checklist

After updates:
- [ ] `npm start` creates default user with system username
- [ ] Seed data references anonymous IDs consistently
- [ ] No "joel" string in actual runtime code (excluding docs)
- [ ] System works identically regardless of host machine username
- [ ] Documentation examples use generic names

---

## Files NOT Requiring Changes

### Safe to Keep "Joel":
1. **Git commit metadata** - Co-Authored-By lines (historical)
2. **CLAUDE.md testimonials** - Quoted insights from Joel
3. **.github/dependabot.yml** - GitHub username (intentional)
4. **README.md conversation examples** - Illustrative dialogue
5. **Historical quotes in docs** - Attributed insights ("Joel said...")
6. **Architecture decision records** - Historical attribution

### Gitignored (Machine-Specific):
1. `agent-scripts/` - Local scripts
2. `agents/` - Local workspace
3. `.claude/settings.local.json` - Local config

---

## Success Criteria

✅ No hardcoded "joel" in runtime code
✅ Data seeding uses environment variables
✅ System works on any developer's machine
✅ Documentation uses generic examples
✅ Historical context preserved where appropriate
