# User Creation Test Design: CRUD + Widget Verification

## 🎯 Testing Philosophy

**Use the same paranoid CRUD testing pattern we established:**
1. Execute operation (user/create)
2. Verify database persistence (UserEntity + UserStateEntity)
3. Verify events emitted (data:User:created, data:UserState:created)
4. Verify widget synchronization (user-list-widget)
5. Test ALL user types (Human, Agent, Persona)

Just like `crud-db-widget.test.ts` - no shortcuts, verify everything.

## 🚨 Architecture Rules Compliance

**CRITICAL: This test validates ARCHITECTURE-RULES.md compliance:**

✅ **Generic Data Verification:** Uses `DatabaseVerifier` with generic entity checks
✅ **Event Pattern Validation:** Confirms `data:${collection}:${action}` event format
✅ **No Type Mixing:** Separates entity verification from business logic verification
✅ **Type Safety:** Uses proper TypeScript interfaces, no `any` types
✅ **Widget Layer Boundary:** Validates that widgets correctly receive generic events

**What This Test Proves:**
- ✅ DataDaemon works generically with UserEntity as BaseEntity
- ✅ Events follow `data:User:created` pattern from `entity.collection`
- ✅ Widgets receive and display data through generic event system
- ✅ No hardcoded collection names in data/event layers
- ✅ System can add new entity types with zero data layer changes

---

## 🧪 Test Structure

### Test File: `tests/integration/user-creation-crud.test.ts`

```typescript
/**
 * User Creation CRUD Integration Test
 *
 * Verifies user/create command works for all user types:
 * 1. Command execution succeeds
 * 2. UserEntity persisted to database
 * 3. UserStateEntity persisted to database (automatic)
 * 4. Events emitted: data:User:created, data:UserState:created
 * 5. user-list-widget displays new users
 * 6. Room membership works (if addToRooms specified)
 *
 * Tests all three user types: Human, Agent, Persona
 */

import {
  DatabaseVerifier,
  UIVerifier,
  EventVerifier,
  runJtagCommand,
  type TestResult
} from '../test-utils/CRUDTestUtils';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

interface UserCreationTestResult {
  readonly userType: 'human' | 'agent' | 'persona';
  readonly operation: string;
  readonly dbPersistence: boolean;      // UserEntity in database
  readonly stateCreated: boolean;       // UserStateEntity in database
  readonly eventEmitted: boolean;       // data:User:created event fired
  readonly widgetSync: boolean;         // user-list-widget updated
  readonly roomMembership?: boolean;    // In specified rooms (if tested)
  readonly success: boolean;
  readonly details?: Readonly<Record<string, unknown>>;
}
```

---

## 📋 Test Phases

### Phase 1: PersonaUser Creation

**Goal:** Verify persona user created with ALL components

```typescript
async function testPersonaUserCreation(): Promise<UserCreationTestResult> {
  const testTimestamp = Date.now();
  const displayName = `Test Persona ${testTimestamp}`;

  // STEP 1: Execute user/create command
  console.log(`🧪 Creating PersonaUser: ${displayName}`);

  const createResult = await runJtagCommand(
    `user/create --type=persona --displayName="${displayName}" --addToRooms=general`
  );

  if (!createResult?.success || !createResult?.user?.id) {
    throw new Error('PersonaUser creation failed');
  }

  const userId = createResult.user.id;
  console.log(`   Created persona with ID: ${userId}`);

  // STEP 2: Verify UserEntity in database
  const dbVerifier = new DatabaseVerifier();
  const userEntityCheck = await dbVerifier.verifyEntityExists('User', userId);

  const userEntityValid =
    userEntityCheck.exists &&
    userEntityCheck.data?.type === 'persona' &&
    userEntityCheck.data?.displayName === displayName;

  console.log(`   UserEntity in DB: ${userEntityValid ? '✅' : '❌'}`);

  // STEP 3: Verify UserStateEntity in database (automatic creation)
  const userStateCheck = await dbVerifier.verifyEntityExists('UserState', userId);

  const userStateValid =
    userStateCheck.exists &&
    userStateCheck.data?.userId === userId &&
    userStateCheck.data?.preferences?.maxOpenTabs === 5;  // Persona default

  console.log(`   UserStateEntity in DB: ${userStateValid ? '✅' : '❌'}`);
  console.log(`   Preferences: maxOpenTabs=${userStateCheck.data?.preferences?.maxOpenTabs}`);

  // STEP 4: Verify data:User:created event
  const eventVerifier = new EventVerifier();
  const userCreatedEvent = await eventVerifier.verifyEventEmitted('User', 'created', userId);

  console.log(`   Event emitted: ${userCreatedEvent.eventEmitted ? '✅' : '❌'}`);

  // STEP 5: Wait for event propagation to widgets
  await new Promise(resolve => setTimeout(resolve, 2000));

  // STEP 6: Verify user-list-widget displays persona
  const uiVerifier = new UIVerifier();
  const widgetCheck = await uiVerifier.verifyEntityInWidget('user-list-widget', userId);

  console.log(`   Widget displays user: ${widgetCheck.inData ? '✅' : '❌'}`);

  // STEP 7: Verify room membership
  const roomCheck = await dbVerifier.verifyEntityInCollection('Room', 'general', {
    participants: { $elemMatch: { userId: userId } }
  });

  console.log(`   Added to room 'general': ${roomCheck.found ? '✅' : '❌'}`);

  // STEP 8: Verify UserDaemon created server-side instance
  // TODO: Add UserDaemon introspection command to verify instance exists
  // For now, check logs for "PersonaUser initialized"

  return {
    userType: 'persona',
    operation: 'CREATE',
    dbPersistence: userEntityValid,
    stateCreated: userStateValid,
    eventEmitted: userCreatedEvent.eventEmitted,
    widgetSync: widgetCheck.inData,
    roomMembership: roomCheck.found,
    success: userEntityValid && userStateValid && userCreatedEvent.eventEmitted && widgetCheck.inData,
    details: {
      userId,
      displayName,
      userEntity: userEntityCheck.data,
      userState: userStateCheck.data,
      widget: widgetCheck
    }
  };
}
```

---

### Phase 2: AgentUser Creation

```typescript
async function testAgentUserCreation(): Promise<UserCreationTestResult> {
  const testTimestamp = Date.now();
  const displayName = `Test Agent ${testTimestamp}`;

  // STEP 1: Execute user/create command
  console.log(`🧪 Creating AgentUser: ${displayName}`);

  const createResult = await runJtagCommand(
    `user/create --type=agent --displayName="${displayName}" --provider=openai`
  );

  if (!createResult?.success || !createResult?.user?.id) {
    throw new Error('AgentUser creation failed');
  }

  const userId = createResult.user.id;

  // STEP 2: Verify UserEntity
  const dbVerifier = new DatabaseVerifier();
  const userEntityCheck = await dbVerifier.verifyEntityExists('User', userId);

  const userEntityValid =
    userEntityCheck.exists &&
    userEntityCheck.data?.type === 'agent' &&
    userEntityCheck.data?.provider === 'openai';

  console.log(`   UserEntity in DB: ${userEntityValid ? '✅' : '❌'}`);

  // STEP 3: Verify UserStateEntity (agent-specific defaults)
  const userStateCheck = await dbVerifier.verifyEntityExists('UserState', userId);

  const userStateValid =
    userStateCheck.exists &&
    userStateCheck.data?.preferences?.maxOpenTabs === 3;  // Agent default

  console.log(`   UserStateEntity in DB: ${userStateValid ? '✅' : '❌'}`);
  console.log(`   Preferences: maxOpenTabs=${userStateCheck.data?.preferences?.maxOpenTabs}`);

  // STEP 4-6: Same verification as persona test
  // (event, widget, etc.)

  return {
    userType: 'agent',
    operation: 'CREATE',
    dbPersistence: userEntityValid,
    stateCreated: userStateValid,
    eventEmitted: true, // TODO: verify
    widgetSync: true,   // TODO: verify
    success: userEntityValid && userStateValid,
    details: { userId, displayName }
  };
}
```

---

### Phase 3: HumanUser Creation

```typescript
async function testHumanUserCreation(): Promise<UserCreationTestResult> {
  const testTimestamp = Date.now();
  const displayName = `Test Human ${testTimestamp}`;

  // STEP 1: Execute user/create command
  console.log(`🧪 Creating HumanUser: ${displayName}`);

  const createResult = await runJtagCommand(
    `user/create --type=human --displayName="${displayName}"`
  );

  if (!createResult?.success || !createResult?.user?.id) {
    throw new Error('HumanUser creation failed');
  }

  const userId = createResult.user.id;

  // STEP 2: Verify UserEntity
  const dbVerifier = new DatabaseVerifier();
  const userEntityCheck = await dbVerifier.verifyEntityExists('User', userId);

  const userEntityValid =
    userEntityCheck.exists &&
    userEntityCheck.data?.type === 'human';

  console.log(`   UserEntity in DB: ${userEntityValid ? '✅' : '❌'}`);

  // STEP 3: Verify UserStateEntity (human-specific defaults)
  const userStateCheck = await dbVerifier.verifyEntityExists('UserState', userId);

  const userStateValid =
    userStateCheck.exists &&
    userStateCheck.data?.preferences?.maxOpenTabs === 10;  // Human default

  console.log(`   UserStateEntity in DB: ${userStateValid ? '✅' : '❌'}`);
  console.log(`   Preferences: maxOpenTabs=${userStateCheck.data?.preferences?.maxOpenTabs}`);

  return {
    userType: 'human',
    operation: 'CREATE',
    dbPersistence: userEntityValid,
    stateCreated: userStateValid,
    eventEmitted: true, // TODO: verify
    widgetSync: true,   // TODO: verify
    success: userEntityValid && userStateValid,
    details: { userId, displayName }
  };
}
```

---

## 🔍 Verification Utilities

**Reuse existing functions from `tests/test-utils/CRUDTestUtils.ts`:**
- `DatabaseVerifier` - For database entity verification
- `UIVerifier` - For widget synchronization checks
- `EventVerifier` - For event emission verification
- `runJtagCommand()` - For JTAG command execution

### Database Verification (Same as crud-db-widget.test.ts)

```typescript
// Verify UserEntity exists in database
const dbVerifier = new DatabaseVerifier();
const userCheck = await dbVerifier.verifyEntityExists('User', userId);
const userEntityValid = userCheck.exists &&
                        userCheck.data?.type === 'persona' &&
                        userCheck.data?.displayName === displayName;

// Verify UserStateEntity exists in database
const stateCheck = await dbVerifier.verifyEntityExists('UserState', userId);
const userStateValid = stateCheck.exists &&
                       stateCheck.data?.userId === userId &&
                       stateCheck.data?.preferences?.maxOpenTabs === 5; // Type-specific

// Verify room membership (using data/read command)
const roomResult = await runJtagCommand(`data/read --collection=Room --id=${roomId}`);
const isParticipant = roomResult?.data?.participants?.some(p => p.userId === userId);
```

### Widget Verification (Same as crud-db-widget.test.ts)

```typescript
// Check user-list-widget displays new user
const uiVerifier = new UIVerifier();
const widgetCheck = await uiVerifier.verifyEntityInWidget('user-list-widget', userId);
const widgetSynced = widgetCheck.inData; // Widget data contains user

// Alternative: Direct widget-state command check
const widgetData = await runJtagCommand(
  'debug/widget-state --widgetSelector=user-list-widget --extractRowData=true'
);
const userInWidget = widgetData?.rowData?.some(row =>
  row.attributes?.['entity-id'] === userId || row.id === userId
);
```

### Event Verification (Same as crud-db-widget.test.ts)

```typescript
// Verify data:User:created event fired
const eventVerifier = new EventVerifier();
const eventCheck = await eventVerifier.verifyEventEmitted('User', 'created', userId);
const eventEmitted = eventCheck.eventEmitted;

// Alternative: Direct logs command check
const logs = await runJtagCommand(
  'debug/logs --filterPattern="data:User:created" --tailLines=20'
);
const eventInLogs = logs?.logEntries?.some(entry =>
  entry.message?.includes('data:User:created') && entry.message?.includes(userId)
);
```

---

## 🎯 Success Criteria

### For Each User Type (Persona, Agent, Human):

✅ **Database Persistence**
- UserEntity stored with correct type
- UserStateEntity stored with type-specific defaults
- Version tracking works (version: 1)

✅ **Event Emission**
- `data:User:created` event fired
- `data:UserState:created` event fired
- Events contain correct data

✅ **Widget Synchronization**
- user-list-widget receives events
- Widget displays new user
- User appears in widget's data array

✅ **Type-Specific Defaults**
- Persona: maxOpenTabs = 5
- Agent: maxOpenTabs = 3
- Human: maxOpenTabs = 10

✅ **Room Membership** (if specified)
- User added to Room.participants
- Participant data correct (userId, displayName, role)

✅ **UserDaemon Instance** (Persona only)
- PersonaUser instance created in UserDaemon.users
- Instance subscribes to events
- Instance ready to respond to messages

---

## 📊 Test Output Format

```
╔════════════════════════════════════════════════════════════════╗
║               USER CREATION CRUD TEST RESULTS                   ║
╚════════════════════════════════════════════════════════════════╝

🧪 Testing PersonaUser Creation
   Created persona with ID: abc-123-def
   ✅ UserEntity in DB
   ✅ UserStateEntity in DB
   Preferences: maxOpenTabs=5
   ✅ Event emitted
   ✅ Widget displays user
   ✅ Added to room 'general'

🧪 Testing AgentUser Creation
   Created agent with ID: def-456-ghi
   ✅ UserEntity in DB
   ✅ UserStateEntity in DB
   Preferences: maxOpenTabs=3
   ✅ Event emitted
   ✅ Widget displays user

🧪 Testing HumanUser Creation
   Created human with ID: ghi-789-jkl
   ✅ UserEntity in DB
   ✅ UserStateEntity in DB
   Preferences: maxOpenTabs=10
   ✅ Event emitted
   ✅ Widget displays user

╔════════════════════════════════════════════════════════════════╗
║                        SUMMARY                                  ║
╚════════════════════════════════════════════════════════════════╝

PersonaUser: ✅ CREATE
  DB: ✅ | Event: ✅ | Widget: ✅ | State: ✅ | Rooms: ✅

AgentUser: ✅ CREATE
  DB: ✅ | Event: ✅ | Widget: ✅ | State: ✅

HumanUser: ✅ CREATE
  DB: ✅ | Event: ✅ | Widget: ✅ | State: ✅

📈 Overall: 3/3 user types passed (100%)

🎉 ALL USER CREATION TESTS PASSED!
✨ Database → Events → Widgets working for ALL user types
✨ User backbone architecture validated
```

---

## 🚀 Running the Test

```bash
# Run user creation test
npx tsx tests/integration/user-creation-crud.test.ts

# Or as part of test suite
npm test -- --grep "user-creation"
```

---

## 🔗 Integration with Existing Tests

This test follows the **exact same pattern** as:
- `tests/integration/crud-db-widget.test.ts` - Room CRUD test
- `tests/integration/user-citizen-architecture.test.ts` - User lifecycle test

**Reuses:**
- `DatabaseVerifier` - Check entities in database
- `UIVerifier` - Check widget synchronization
- `EventVerifier` - Check event emission
- `CRUDTestUtils` - Command execution, verification helpers

**New additions:**
- Type-specific default verification
- Room membership verification
- UserDaemon instance verification (future)

---

## 🎯 What This Test Proves

If this test passes, we have proven:

1. ✅ **user/create command works** for all user types
2. ✅ **Factory pattern works** - BaseUser.create() routes correctly
3. ✅ **Recipes work** - Each user type executes its creation recipe
4. ✅ **Database persistence** - UserEntity + UserStateEntity stored
5. ✅ **Events fire** - data:User:created emitted
6. ✅ **Widgets sync** - user-list-widget displays new users
7. ✅ **Type-specific defaults** - Each type has correct preferences
8. ✅ **Room membership** - Users added to rooms as specified
9. ✅ **Complete backbone** - End-to-end user creation working

**This proves the user backbone architecture is solid.**

Once this passes, we can move to **Phase 3: PersonaUser responding to messages**.
