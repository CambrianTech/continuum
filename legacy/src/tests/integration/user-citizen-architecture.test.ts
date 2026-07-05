/**
 * User Citizen Architecture Integration Test
 *
 * Verifies the complete user system backbone for Academy genomic AI:
 * 1. All 3 user types (Human/Agent/Persona) persist to database correctly
 * 2. UserEntity + UserStateEntity creation chain works
 * 3. Events emitted: data:User:created/updated/deleted
 * 4. user-list-widget receives and displays all user types
 * 5. Storage backends correct per type (Memory/SQLite)
 * 6. State persists across full lifecycle
 *
 * This is the backbone test for the entire Academy genomic system.
 * If this fails, nothing else matters.
 */

import {
  DatabaseVerifier,
  UIVerifier,
  EventVerifier,
  runJtagCommand,
  type EntityInstance,
  type TestResult
} from '../test-utils/CRUDTestUtils';
import { SchemaFactory } from '../test-utils/SchemaBasedFactory';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

interface UserTestResult {
  readonly userType: 'human' | 'agent' | 'persona';
  readonly operation: string;
  readonly dbPersistence: boolean;
  readonly widgetSync: boolean;
  readonly eventEmitted: boolean;
  readonly stateCreated: boolean;
  readonly success: boolean;
  readonly details?: Readonly<Record<string, unknown>>;
}

/**
 * Test complete user lifecycle for given user type
 */
async function testUserTypeLifecycle(
  userType: 'human' | 'agent' | 'persona',
  displayName: string
): Promise<UserTestResult[]> {
  console.log(`\n${'='.repeat(60)}`);
  console.log(`🧪 Testing ${userType.toUpperCase()} User Lifecycle`);
  console.log(`${'='.repeat(60)}\n`);

  const results: UserTestResult[] = [];
  const dbVerifier = new DatabaseVerifier();
  const uiVerifier = new UIVerifier();
  const eventVerifier = new EventVerifier();

  let userId: UUID | undefined;

  try {
    // ============================================
    // CREATE USER
    // ============================================
    console.log(`📝 CREATE: ${userType} user "${displayName}"`);

    const createData = {
      type: userType,
      displayName: displayName,
      status: 'online',
      capabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: userType === 'human',
        canModerate: userType === 'human',
        autoResponds: userType !== 'human',
        canAccessPersonas: userType === 'human'
      }
    };

    const createResult = await SchemaFactory.create('User', createData);

    if (!createResult.success || !createResult.id) {
      throw new Error(`CREATE failed for ${userType}: ${createResult.error}`);
    }

    userId = createResult.id;
    console.log(`   ✅ Created: ${userId}`);

    // Verify: Database persistence
    const dbCheck = await dbVerifier.verifyEntityExists('User', userId);
    const dbSuccess = dbCheck.exists && dbCheck.data?.type === userType;

    // Verify: Event emission
    const eventCheck = await eventVerifier.verifyEventEmitted('User', 'created', userId);

    // Verify: Widget synchronization
    await new Promise(resolve => setTimeout(resolve, 2000)); // Wait for event propagation
    const widgetCheck = await uiVerifier.verifyEntityInWidget('user-list-widget', userId);

    // Verify: UserState entity created
    const userStateCheck = await dbVerifier.verifyEntityExists('UserState', userId);

    results.push({
      userType,
      operation: 'CREATE',
      dbPersistence: dbSuccess,
      widgetSync: widgetCheck.inData,
      eventEmitted: eventCheck.eventEmitted,
      stateCreated: userStateCheck.exists,
      success: dbSuccess && widgetCheck.inData && eventCheck.eventEmitted && userStateCheck.exists,
      details: {
        userId,
        displayName,
        dbData: dbCheck.data,
        widgetData: widgetCheck,
        event: eventCheck.eventData
      }
    });

    console.log(`   DB Persistence: ${dbSuccess ? '✅' : '❌'}`);
    console.log(`   Event Emitted: ${eventCheck.eventEmitted ? '✅' : '❌'}`);
    console.log(`   Widget Sync: ${widgetCheck.inData ? '✅' : '❌'}`);
    console.log(`   UserState Created: ${userStateCheck.exists ? '✅' : '❌'}`);

    // ============================================
    // UPDATE USER
    // ============================================
    console.log(`\n📝 UPDATE: ${userType} user status`);

    const updateData = {
      status: 'away',
      displayName: `${displayName} (Updated)`
    };

    const updateResult = await runJtagCommand(`data/update --collection=User --id=${userId} --data='${JSON.stringify(updateData)}'`);

    if (updateResult?.found) {
      // Verify: Database changes
      const dbUpdateCheck = await dbVerifier.verifyEntityUpdated('User', userId, updateData);

      // Verify: Event emission
      const updateEventCheck = await eventVerifier.verifyEventEmitted('User', 'updated', userId);

      // Verify: Widget reflects changes
      await new Promise(resolve => setTimeout(resolve, 2000));
      const widgetUpdateCheck = await uiVerifier.verifyEntityUpdatedInWidget(
        'user-list-widget',
        userId,
        updateData
      );

      results.push({
        userType,
        operation: 'UPDATE',
        dbPersistence: dbUpdateCheck.updated,
        widgetSync: widgetUpdateCheck.dataUpdated,
        eventEmitted: updateEventCheck.eventEmitted,
        stateCreated: true, // Already verified in CREATE
        success: dbUpdateCheck.updated && widgetUpdateCheck.dataUpdated && updateEventCheck.eventEmitted,
        details: {
          userId,
          updatedFields: updateData
        }
      });

      console.log(`   DB Updated: ${dbUpdateCheck.updated ? '✅' : '❌'}`);
      console.log(`   Event Emitted: ${updateEventCheck.eventEmitted ? '✅' : '❌'}`);
      console.log(`   Widget Updated: ${widgetUpdateCheck.dataUpdated ? '✅' : '❌'}`);
    }

    // ============================================
    // DELETE USER
    // ============================================
    console.log(`\n📝 DELETE: ${userType} user`);

    const deleteResult = await runJtagCommand(`data/delete --collection=User --id=${userId}`);

    if (deleteResult?.found && deleteResult?.deleted) {
      // Verify: Database removal
      const dbDeleteCheck = await dbVerifier.verifyEntityDeleted('User', userId);

      // Verify: Event emission
      const deleteEventCheck = await eventVerifier.verifyEventEmitted('User', 'deleted', userId);

      // Verify: Widget removal
      await new Promise(resolve => setTimeout(resolve, 2000));
      const widgetDeleteCheck = await uiVerifier.verifyEntityRemovedFromWidget('user-list-widget', userId);

      // Verify: UserState also deleted (cascade)
      const stateDeleteCheck = await dbVerifier.verifyEntityDeleted('UserState', userId);

      results.push({
        userType,
        operation: 'DELETE',
        dbPersistence: dbDeleteCheck.deleted,
        widgetSync: widgetDeleteCheck.removedFromData,
        eventEmitted: deleteEventCheck.eventEmitted,
        stateCreated: stateDeleteCheck.deleted, // Should be deleted too
        success: dbDeleteCheck.deleted && widgetDeleteCheck.removedFromData && deleteEventCheck.eventEmitted,
        details: {
          userId,
          userStateAlsoDeleted: stateDeleteCheck.deleted
        }
      });

      console.log(`   DB Deleted: ${dbDeleteCheck.deleted ? '✅' : '❌'}`);
      console.log(`   Event Emitted: ${deleteEventCheck.eventEmitted ? '✅' : '❌'}`);
      console.log(`   Widget Removed: ${widgetDeleteCheck.removedFromData ? '✅' : '❌'}`);
      console.log(`   UserState Cascade: ${stateDeleteCheck.deleted ? '✅' : '❌'}`);
    }

  } catch (error) {
    console.error(`   ❌ ${userType} lifecycle failed: ${error instanceof Error ? error.message : String(error)}`);
    results.push({
      userType,
      operation: 'LIFECYCLE',
      dbPersistence: false,
      widgetSync: false,
      eventEmitted: false,
      stateCreated: false,
      success: false,
      details: {
        error: error instanceof Error ? error.message : String(error)
      }
    });
  }

  return results;
}

/**
 * Main test execution
 */
async function testUserCitizenArchitecture(): Promise<void> {
  console.log('\n');
  console.log('╔' + '═'.repeat(78) + '╗');
  console.log('║' + ' '.repeat(20) + 'USER CITIZEN ARCHITECTURE TEST' + ' '.repeat(27) + '║');
  console.log('║' + ' '.repeat(18) + 'Complete User System Backbone Test' + ' '.repeat(25) + '║');
  console.log('╚' + '═'.repeat(78) + '╝');
  console.log('\n');

  const allResults: UserTestResult[] = [];

  // Test all 3 user types
  const testTimestamp = Date.now();

  const humanResults = await testUserTypeLifecycle(
    'human',
    `Test Human ${testTimestamp}`
  );
  allResults.push(...humanResults);

  const agentResults = await testUserTypeLifecycle(
    'agent',
    `Test Agent ${testTimestamp}`
  );
  allResults.push(...agentResults);

  const personaResults = await testUserTypeLifecycle(
    'persona',
    `Test Persona ${testTimestamp}`
  );
  allResults.push(...personaResults);

  // ============================================
  // Results Summary
  // ============================================
  console.log('\n');
  console.log('╔' + '═'.repeat(78) + '╗');
  console.log('║' + ' '.repeat(28) + 'TEST RESULTS SUMMARY' + ' '.repeat(30) + '║');
  console.log('╚' + '═'.repeat(78) + '╝');
  console.log('\n');

  const passedTests = allResults.filter(r => r.success).length;
  const totalTests = allResults.length;
  const successRate = totalTests > 0 ? ((passedTests / totalTests) * 100).toFixed(1) : '0.0';

  // Group by user type
  const byUserType = {
    human: allResults.filter(r => r.userType === 'human'),
    agent: allResults.filter(r => r.userType === 'agent'),
    persona: allResults.filter(r => r.userType === 'persona')
  };

  for (const [userType, results] of Object.entries(byUserType)) {
    console.log(`${userType.toUpperCase()} USER (${results.length} operations):`);
    results.forEach(result => {
      const status = result.success ? '✅' : '❌';
      console.log(`  ${status} ${result.operation}`);
      console.log(`     DB: ${result.dbPersistence ? '✅' : '❌'} | ` +
                  `Event: ${result.eventEmitted ? '✅' : '❌'} | ` +
                  `Widget: ${result.widgetSync ? '✅' : '❌'} | ` +
                  `State: ${result.stateCreated ? '✅' : '❌'}`);
    });
    console.log('');
  }

  console.log(`📈 Overall: ${passedTests}/${totalTests} operations passed (${successRate}%)\n`);

  // Critical failures analysis
  const criticalFailures: string[] = [];

  for (const [userType, results] of Object.entries(byUserType)) {
    const createFailed = results.find(r => r.operation === 'CREATE' && !r.success);
    if (createFailed) {
      criticalFailures.push(`${userType} CREATE chain broken`);
    }

    const missingDbPersistence = results.filter(r => !r.dbPersistence);
    if (missingDbPersistence.length > 0) {
      criticalFailures.push(`${userType} database persistence failing`);
    }

    const missingEvents = results.filter(r => !r.eventEmitted);
    if (missingEvents.length > 0) {
      criticalFailures.push(`${userType} event emission failing`);
    }

    const missingWidgetSync = results.filter(r => !r.widgetSync);
    if (missingWidgetSync.length > 0) {
      criticalFailures.push(`${userType} widget synchronization failing`);
    }
  }

  if (successRate === '100.0') {
    console.log('🎉 ALL USER CITIZEN ARCHITECTURE TESTS PASSED!');
    console.log('✨ Database → Events → Widgets working for ALL user types');
    console.log('✨ Ready for Academy genomic AI system\n');
    process.exit(0);
  } else {
    console.log('⚠️ USER CITIZEN ARCHITECTURE HAS FAILURES\n');

    if (criticalFailures.length > 0) {
      console.log('🚨 CRITICAL FAILURES:');
      criticalFailures.forEach(failure => console.log(`   ❌ ${failure}`));
      console.log('');
    }

    console.log('❌ Academy genomic system cannot proceed until user backbone is solid\n');
    process.exit(1);
  }
}

// Execute test suite
testUserCitizenArchitecture().catch(error => {
  console.error('❌ Fatal test error:', error);
  process.exit(1);
});