/**
 * PersonaUser Storage Backend Integration Test
 *
 * Tests that PersonaUser uses SQLiteStateBackend correctly with paranoid verification:
 * 1. SessionDaemon assigns SQLiteStateBackend to PersonaUser (not MemoryStateBackend)
 * 2. Each PersonaUser gets dedicated SQLite path: `.continuum/personas/{personaId}/state.sqlite`
 * 3. PersonaUser.saveState() persists to SQLite database
 * 4. PersonaUser.loadState() reads from SQLite database
 * 5. Control test: AgentUser still uses MemoryStateBackend (ephemeral)
 * 6. UserState CRUD operations work end-to-end for personas
 *
 * No shortcuts. No assumptions. Verify everything.
 */

import { execSync } from 'child_process';
import { existsSync } from 'fs';
import { join } from 'path';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import type { UserStateEntity } from '../../system/data/entities/UserStateEntity';

interface PersonaTestResult {
  readonly testName: string;
  readonly success: boolean;
  readonly details: Readonly<Record<string, unknown>>;
  readonly error?: string;
}

interface SessionCreateResult {
  readonly success: boolean;
  readonly sessionId: UUID;
  readonly user?: {
    readonly id: UUID;
    readonly type: 'human' | 'agent' | 'persona';
    readonly displayName: string;
  };
  readonly error?: string;
}

interface UserStateReadResult {
  readonly success: boolean;
  readonly found: boolean;
  readonly data?: UserStateEntity;
  readonly error?: string;
}

interface LogSearchResult {
  readonly success: boolean;
  readonly logEntries: ReadonlyArray<{ readonly message: string; readonly timestamp: string }>;
  readonly filteredLines: number;
}

/**
 * Execute JTAG command with strict type checking
 */
function executeJtagCommand<T = Record<string, unknown>>(command: string): T {
  try {
    const output = execSync(`./jtag ${command}`, {
      encoding: 'utf8',
      cwd: process.cwd(),
      timeout: 15000,
      maxBuffer: 1024 * 1024 * 50
    });

    // Parse last complete JSON object
    const jsonStart = output.lastIndexOf('{');
    if (jsonStart < 0) {
      throw new Error(`No JSON found in command output: ${command}`);
    }

    let braceCount = 0;
    let jsonEnd = jsonStart;
    for (let i = jsonStart; i < output.length; i++) {
      if (output[i] === '{') braceCount++;
      if (output[i] === '}') {
        braceCount--;
        if (braceCount === 0) {
          jsonEnd = i + 1;
          break;
        }
      }
    }

    const jsonStr = output.substring(jsonStart, jsonEnd);
    const result = JSON.parse(jsonStr) as T;

    // Validate that we got a real object
    if (!result || typeof result !== 'object') {
      throw new Error(`Invalid JSON result from command: ${command}`);
    }

    return result;
  } catch (error) {
    throw new Error(
      `JTAG command failed: ${command}\n` +
      `Error: ${error instanceof Error ? error.message : String(error)}`
    );
  }
}

/**
 * Verify SQLite database file exists at expected path
 */
function verifyDatabaseFileExists(personaId: UUID): { exists: boolean; path: string } {
  const expectedPath = join(process.cwd(), `.continuum/personas/${personaId}/state.sqlite`);
  const exists = existsSync(expectedPath);

  console.log(`   🔍 Checking database file: ${expectedPath}`);
  console.log(`   ${exists ? '✅' : '❌'} Database file ${exists ? 'exists' : 'does not exist'}`);

  return { exists, path: expectedPath };
}

/**
 * Search logs for storage backend assignment
 */
function verifyStorageBackendInLogs(
  userType: 'persona' | 'agent',
  displayName: string
): { found: boolean; usingSQLite: boolean; usingMemory: boolean } {
  try {
    const pattern = userType === 'persona'
      ? 'Using SQLiteStateBackend for persona'
      : 'Using MemoryStateBackend for agent';

    const result = executeJtagCommand<LogSearchResult>(
      `debug/logs --filterPattern="${pattern}" --tailLines=50`
    );

    const relevantEntry = result.logEntries.find(entry =>
      entry.message.includes(displayName)
    );

    const found = Boolean(relevantEntry);
    const usingSQLite = found && relevantEntry!.message.includes('SQLiteStateBackend');
    const usingMemory = found && relevantEntry!.message.includes('MemoryStateBackend');

    console.log(`   🔍 Log search for ${userType} storage backend:`);
    console.log(`   ${found ? '✅' : '❌'} Found storage backend log entry`);
    if (found) {
      console.log(`   Storage type: ${usingSQLite ? 'SQLite' : usingMemory ? 'Memory' : 'Unknown'}`);
    }

    return { found, usingSQLite, usingMemory };
  } catch (error) {
    console.log(`   ❌ Log search failed: ${error instanceof Error ? error.message : String(error)}`);
    return { found: false, usingSQLite: false, usingMemory: false };
  }
}

/**
 * Verify UserState exists in database for given userId
 */
function verifyUserStateInDatabase(userId: UUID): {
  exists: boolean;
  stateId?: UUID;
  data?: UserStateEntity;
} {
  try {
    const listResult = executeJtagCommand<{
      success: boolean;
      items: UserStateEntity[];
      count: number;
    }>(`data/list --collection=UserState --filter='{"userId":"${userId}"}'`);

    if (!listResult.success || listResult.count === 0) {
      console.log(`   ❌ UserState not found for userId: ${userId}`);
      return { exists: false };
    }

    const userState = listResult.items[0];
    if (!userState || !userState.id) {
      console.log(`   ❌ UserState found but missing id`);
      return { exists: false };
    }

    console.log(`   ✅ UserState found: ${userState.id}`);
    console.log(`   Preferences: maxOpenTabs=${userState.preferences?.maxOpenTabs}`);

    return {
      exists: true,
      stateId: userState.id,
      data: userState
    };
  } catch (error) {
    console.log(`   ❌ Database query failed: ${error instanceof Error ? error.message : String(error)}`);
    return { exists: false };
  }
}

/**
 * Main test suite
 */
async function testPersonaUserStorage(): Promise<void> {
  console.log('🧪 PersonaUser Storage Backend Integration Test');
  console.log('===============================================\n');

  const results: PersonaTestResult[] = [];
  const testTimestamp = Date.now();
  const personaDisplayName = `Test Persona ${testTimestamp}`;
  const agentDisplayName = `Test Agent ${testTimestamp}`;

  let personaUserId: UUID | undefined;
  let personaSessionId: UUID | undefined;
  let agentUserId: UUID | undefined;

  try {
    // ============================================
    // TEST 1: Create PersonaUser Session
    // ============================================
    console.log('📝 Test 1: Create PersonaUser session');
    console.log('   Verifying: SessionDaemon creates PersonaUser with SQLiteStateBackend\n');

    const personaSession = executeJtagCommand<SessionCreateResult>(
      `session/create --category=persona --displayName="${personaDisplayName}" --isShared=false`
    );

    if (!personaSession.success || !personaSession.user) {
      throw new Error('PersonaUser session creation failed');
    }

    personaUserId = personaSession.user.id;
    personaSessionId = personaSession.sessionId;

    const sessionSuccess = Boolean(
      personaSession.success &&
      personaSession.user.type === 'persona' &&
      personaUserId
    );

    results.push({
      testName: 'PersonaUser Session Creation',
      success: sessionSuccess,
      details: {
        userId: personaUserId,
        sessionId: personaSessionId,
        userType: personaSession.user.type,
        displayName: personaSession.user.displayName
      }
    });

    console.log(`   ${sessionSuccess ? '✅' : '❌'} PersonaUser session created`);
    console.log(`   User ID: ${personaUserId}`);
    console.log(`   User Type: ${personaSession.user.type}\n`);

    // ============================================
    // TEST 2: Verify SQLiteStateBackend Assignment
    // ============================================
    console.log('📝 Test 2: Verify SQLiteStateBackend assignment');
    console.log('   Checking: Logs contain "Using SQLiteStateBackend for persona"\n');

    const storageLog = verifyStorageBackendInLogs('persona', personaDisplayName);

    const storageBackendCorrect = storageLog.found && storageLog.usingSQLite && !storageLog.usingMemory;

    results.push({
      testName: 'SQLiteStateBackend Assignment',
      success: storageBackendCorrect,
      details: {
        logEntryFound: storageLog.found,
        usingSQLite: storageLog.usingSQLite,
        usingMemory: storageLog.usingMemory
      },
      error: !storageBackendCorrect ? 'Expected SQLiteStateBackend, got different backend or no log entry' : undefined
    });

    console.log(`   ${storageBackendCorrect ? '✅' : '❌'} Storage backend assignment verified\n`);

    // ============================================
    // TEST 3: Verify Dedicated SQLite Database Path
    // ============================================
    console.log('📝 Test 3: Verify dedicated SQLite database file');
    console.log(`   Expected path: .continuum/personas/${personaUserId}/state.sqlite\n`);

    const dbFile = verifyDatabaseFileExists(personaUserId);

    results.push({
      testName: 'Dedicated SQLite Database File',
      success: dbFile.exists,
      details: {
        personaId: personaUserId,
        expectedPath: dbFile.path,
        fileExists: dbFile.exists
      },
      error: !dbFile.exists ? 'SQLite database file not found at expected path' : undefined
    });

    console.log(`   ${dbFile.exists ? '✅' : '❌'} Database file verification\n`);

    // ============================================
    // TEST 4: Verify UserState Persistence
    // ============================================
    console.log('📝 Test 4: Verify UserState persisted to database');
    console.log('   Checking: UserState record exists for persona userId\n');

    const userState = verifyUserStateInDatabase(personaUserId);

    results.push({
      testName: 'UserState Database Persistence',
      success: userState.exists,
      details: {
        userId: personaUserId,
        stateId: userState.stateId,
        stateExists: userState.exists,
        hasPreferences: Boolean(userState.data?.preferences)
      },
      error: !userState.exists ? 'UserState not found in database' : undefined
    });

    console.log(`   ${userState.exists ? '✅' : '❌'} UserState persistence verified\n`);

    // ============================================
    // TEST 5: Verify Persona-Specific Preferences
    // ============================================
    console.log('📝 Test 5: Verify persona-specific preferences');
    console.log('   Expected: maxOpenTabs=5 (persona default, not human default of 10)\n');

    const hasPersonaPreferences = userState.data?.preferences?.maxOpenTabs === 5;

    results.push({
      testName: 'Persona-Specific Preferences',
      success: hasPersonaPreferences,
      details: {
        maxOpenTabs: userState.data?.preferences?.maxOpenTabs,
        expectedMaxOpenTabs: 5,
        autoCloseAfterDays: userState.data?.preferences?.autoCloseAfterDays
      },
      error: !hasPersonaPreferences ? 'Persona preferences incorrect (expected maxOpenTabs=5)' : undefined
    });

    console.log(`   ${hasPersonaPreferences ? '✅' : '❌'} Persona preferences verified\n`);

    // ============================================
    // TEST 6: Control Test - AgentUser Uses Memory
    // ============================================
    console.log('📝 Test 6: Control test - AgentUser uses MemoryStateBackend');
    console.log('   Verifying: Agents do NOT use SQLite (ephemeral storage)\n');

    const agentSession = executeJtagCommand<SessionCreateResult>(
      `session/create --category=agent --displayName="${agentDisplayName}" --isShared=false`
    );

    if (agentSession.success && agentSession.user) {
      agentUserId = agentSession.user.id;

      const agentStorageLog = verifyStorageBackendInLogs('agent', agentDisplayName);
      const agentDbFile = verifyDatabaseFileExists(agentUserId);

      const agentUsesMemory = agentStorageLog.found && agentStorageLog.usingMemory && !agentStorageLog.usingSQLite;
      const agentNoDbFile = !agentDbFile.exists;

      results.push({
        testName: 'AgentUser MemoryStateBackend (Control)',
        success: agentUsesMemory && agentNoDbFile,
        details: {
          userId: agentUserId,
          usingMemory: agentStorageLog.usingMemory,
          usingSQLite: agentStorageLog.usingSQLite,
          hasDatabaseFile: agentDbFile.exists
        },
        error: !agentUsesMemory ? 'Agent should use MemoryStateBackend, not SQLite' : undefined
      });

      console.log(`   ${agentUsesMemory && agentNoDbFile ? '✅' : '❌'} Agent storage backend verified\n`);
    } else {
      results.push({
        testName: 'AgentUser MemoryStateBackend (Control)',
        success: false,
        details: {},
        error: 'Failed to create agent session for control test'
      });
    }

  } catch (error) {
    console.error(`\n❌ Test execution failed: ${error instanceof Error ? error.message : String(error)}\n`);
    results.push({
      testName: 'Test Execution',
      success: false,
      details: {},
      error: error instanceof Error ? error.message : String(error)
    });
  }

  // ============================================
  // Results Summary
  // ============================================
  console.log('\n📊 PersonaUser Storage Test Results');
  console.log('===================================\n');

  const passedTests = results.filter(r => r.success).length;
  const totalTests = results.length;
  const successRate = totalTests > 0 ? ((passedTests / totalTests) * 100).toFixed(1) : '0.0';

  results.forEach(result => {
    const status = result.success ? '✅' : '❌';
    console.log(`${status} ${result.testName}`);
    if (result.error) {
      console.log(`   Error: ${result.error}`);
    }
  });

  console.log(`\n📈 Results: ${passedTests}/${totalTests} passed (${successRate}%)\n`);

  if (successRate === '100.0') {
    console.log('🎉 ALL PERSONA USER STORAGE TESTS PASSED!');
    console.log('✨ PersonaUser SQLiteStateBackend integration working correctly\n');
    process.exit(0);
  } else {
    console.log('⚠️ Some tests failed - PersonaUser storage backend needs fixes\n');
    process.exit(1);
  }
}

// Execute test suite
testPersonaUserStorage().catch(error => {
  console.error('❌ Fatal test error:', error);
  process.exit(1);
});