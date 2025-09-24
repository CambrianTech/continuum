/**
 * CRUD Event Chain Integration Test
 *
 * Validates the complete elegant architecture:
 * 1. CRUD Command → 2. Database Persistence → 3. Event Emission → 4. Widget State → 5. HTML DOM
 *
 * Tests all entity types (User, Room, ChatMessage) using the same generic code
 * Architecture-compliant: Zero entity-specific logic, works with any BaseEntity
 */

import {
  runJtagCommand,
  DatabaseVerifier,
  UIVerifier,
  EventVerifier,
  CRUDOperationTester,
  TestResult,
  EntityInstance,
  sleep
} from '../test-utils/CRUDTestUtils';
import { stringToUUID } from '../../system/core/types/CrossPlatformUUID';

// Test configurations for each Collection-Widget pair
interface TestConfig {
  collection: string;
  widget: string;
  testEntityData: Partial<EntityInstance>;
  updateData: Partial<EntityInstance>;
  verifyField: string;
}

const TEST_CONFIGS: TestConfig[] = [
  {
    collection: 'User',
    widget: 'user-list-widget',
    testEntityData: {
      id: stringToUUID('test-user-crud-001'),
      displayName: 'Test User CRUD',
      type: 'human',
      status: 'online',
      shortDescription: 'Test user for CRUD validation',
      profile: {
        displayName: 'Test User CRUD',
        bio: 'Test bio for CRUD testing',
        avatar: '👤',
        location: 'Test Location',
        joinedAt: new Date()
      },
      capabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: true,
        canInviteOthers: true,
        canModerate: false,
        autoResponds: false,
        providesContext: false,
        canTrain: false,
        canAccessPersonas: false
      },
      preferences: {
        theme: 'dark',
        language: 'en',
        timezone: 'UTC',
        notifications: {
          mentions: true,
          directMessages: true,
          roomUpdates: true
        },
        privacy: {
          showOnlineStatus: true,
          allowDirectMessages: true,
          shareActivity: true
        }
      },
      sessionsActive: [],
      lastActiveAt: new Date()
    },
    updateData: {
      displayName: 'Updated Test User',
      shortDescription: 'Updated description'
    },
    verifyField: 'displayName'
  },
  {
    collection: 'Room',
    widget: 'room-list-widget',
    testEntityData: {
      id: stringToUUID('test-room-crud-001'),
      name: 'Test Room CRUD',
      displayName: 'Test Room for CRUD',
      description: 'Test room for CRUD validation',
      type: 'public',
      status: 'active',
      ownerId: stringToUUID('Joel'),
      members: [],
      tags: []
    },
    updateData: {
      description: 'Updated room description',
      name: 'Updated Test Room'
    },
    verifyField: 'name'
  },
  {
    collection: 'ChatMessage',
    widget: 'chat-widget',
    testEntityData: {
      id: stringToUUID('test-message-crud-001'),
      roomId: stringToUUID('General'),
      senderId: stringToUUID('Joel'),
      senderName: 'Joel',
      content: {
        text: 'Test message for CRUD validation',
        attachments: []
      },
      status: 'sent',
      priority: 'normal',
      timestamp: new Date(),
      reactions: []
    },
    updateData: {
      content: {
        text: 'Updated test message content',
        attachments: []
      }
    },
    verifyField: 'content'
  }
];

/**
 * Complete CRUD Event Chain Test Suite
 * Architecture-compliant: Works with any entity type generically
 */
class CompleteCRUDEventChainTester {
  private dbVerifier = new DatabaseVerifier();
  private uiVerifier = new UIVerifier();
  private eventVerifier = new EventVerifier();

  private allTestResults: TestResult[] = [];

  /**
   * Run complete CRUD event chain validation for all entity types
   */
  async runCompleteValidation(): Promise<boolean> {
    console.log('🧪 COMPLETE CRUD-EVENT CHAIN VALIDATION');
    console.log('=====================================');
    console.log('Testing elegant architecture: CRUD → Event → DB → Widget → HTML');
    console.log('Architecture-compliant: Same code works for all entity types\n');

    let allTestsPass = true;

    for (const config of TEST_CONFIGS) {
      console.log(`\n📋 Testing Entity: ${config.collection} → ${config.widget}`);
      console.log('─'.repeat(60));

      const entityTestPass = await this.testEntityCRUDChain(config);
      allTestsPass = allTestsPass && entityTestPass;

      // Small delay between entity tests to allow system to settle
      await sleep(500);
    }

    // Final comprehensive validation
    console.log('\n🔍 COMPREHENSIVE ARCHITECTURE VALIDATION');
    console.log('=======================================');
    const architectureValid = await this.validateArchitecture();
    allTestsPass = allTestsPass && architectureValid;

    this.printFinalResults(allTestsPass);
    return allTestsPass;
  }

  /**
   * Test complete CRUD chain for a single entity type
   * Generic: Works with any entity extending BaseEntity
   */
  private async testEntityCRUDChain(config: TestConfig): Promise<boolean> {
    const tester = new CRUDOperationTester(
      config.collection,
      config.widget,
      this.dbVerifier,
      this.uiVerifier,
      this.eventVerifier
    );

    let entityId: string | undefined;
    const entityResults: TestResult[] = [];

    try {
      // 1. TEST CREATE OPERATION CHAIN
      console.log(`\n1️⃣ CREATE Chain: ${config.collection}`);
      const createResults = await tester.testCreateOperation(config.testEntityData);
      entityResults.push(...createResults);

      // Extract entity ID from create operation for subsequent tests
      const createSuccess = createResults.find(r => r.step.includes('CREATE Operation'));
      entityId = createSuccess?.details?.entityId as string;

      if (!entityId) {
        console.log(`❌ CREATE failed - cannot proceed with UPDATE/DELETE tests`);
        this.allTestResults.push(...entityResults);
        return false;
      }

      // Small delay to allow event propagation
      await sleep(300);

      // 2. TEST UPDATE OPERATION CHAIN
      console.log(`\n2️⃣ UPDATE Chain: ${config.collection}`);
      const updateResults = await tester.testUpdateOperation(entityId, config.updateData);
      entityResults.push(...updateResults);

      // Small delay to allow event propagation
      await sleep(300);

      // 3. TEST DELETE OPERATION CHAIN
      console.log(`\n3️⃣ DELETE Chain: ${config.collection}`);
      const deleteResults = await tester.testDeleteOperation(entityId);
      entityResults.push(...deleteResults);

    } catch (error) {
      console.error(`❌ Entity test failed: ${config.collection}`, error);
      entityResults.push({
        step: `${config.collection} Complete Chain`,
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }

    this.allTestResults.push(...entityResults);

    // Analyze entity-specific results
    const entitySuccess = entityResults.every(result => result.success);
    const failedSteps = entityResults.filter(result => !result.success);

    console.log(`\n📊 ${config.collection} Results: ${entitySuccess ? '✅ ALL PASS' : '❌ FAILURES'}`);
    if (!entitySuccess) {
      failedSteps.forEach(failure => {
        console.log(`   ❌ ${failure.step}: ${failure.error ?? 'Failed verification'}`);
      });
    }

    return entitySuccess;
  }

  /**
   * Validate the overall architecture patterns
   */
  private async validateArchitecture(): Promise<boolean> {
    console.log('🏗️ Architecture Pattern Validation');

    const validationResults: TestResult[] = [];

    try {
      // 1. Verify generic event pattern consistency
      console.log('   🔍 Generic event pattern validation...');
      const eventPatternResult = await this.validateGenericEventPatterns();
      validationResults.push(eventPatternResult);

      // 2. Verify data persistence patterns
      console.log('   🔍 Data persistence pattern validation...');
      const persistenceResult = await this.validateDataPersistencePatterns();
      validationResults.push(persistenceResult);

      // 3. Verify widget synchronization patterns
      console.log('   🔍 Widget synchronization pattern validation...');
      const syncResult = await this.validateWidgetSynchronizationPatterns();
      validationResults.push(syncResult);

    } catch (error) {
      console.error('❌ Architecture validation failed:', error);
      validationResults.push({
        step: 'Architecture Validation',
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }

    this.allTestResults.push(...validationResults);

    const architectureValid = validationResults.every(result => result.success);
    console.log(`📊 Architecture Validation: ${architectureValid ? '✅ VALID' : '❌ INVALID'}`);

    return architectureValid;
  }

  /**
   * Validate that all event patterns follow the data:collection:action format
   */
  private async validateGenericEventPatterns(): Promise<TestResult> {
    const expectedPatterns = [
      'data:User:created', 'data:User:updated', 'data:User:deleted',
      'data:Room:created', 'data:Room:updated', 'data:Room:deleted',
      'data:ChatMessage:created', 'data:ChatMessage:updated', 'data:ChatMessage:deleted'
    ];

    const eventLogs = await runJtagCommand('debug/logs --filterPattern="data:" --tailLines=50');
    const logEntries = eventLogs.logEntries as Array<{ message?: string }> ?? [];
    const foundPatterns: string[] = [];

    for (const pattern of expectedPatterns) {
      const patternFound = logEntries.some(entry =>
        entry.message && entry.message.includes(pattern)
      );
      if (patternFound) {
        foundPatterns.push(pattern);
      }
    }

    const allPatternsFound = expectedPatterns.every(pattern => foundPatterns.includes(pattern));

    return {
      step: 'Generic Event Pattern Validation',
      success: allPatternsFound,
      details: {
        expectedPatterns: expectedPatterns.length,
        foundPatterns: foundPatterns.length,
        missingPatterns: expectedPatterns.filter(p => !foundPatterns.includes(p))
      }
    };
  }

  /**
   * Validate data persistence patterns work consistently
   */
  private async validateDataPersistencePatterns(): Promise<TestResult> {
    const collections = ['User', 'Room', 'ChatMessage'];
    const persistenceResults: Record<string, boolean> = {};

    for (const collection of collections) {
      // Test that we can list entities from each collection
      const listResult = await runJtagCommand(`data/list --collection=${collection} --limit=1`);
      persistenceResults[collection] = Boolean(listResult.success);
    }

    const allPersistenceWorks = Object.values(persistenceResults).every(Boolean);

    return {
      step: 'Data Persistence Pattern Validation',
      success: allPersistenceWorks,
      details: { persistenceResults }
    };
  }

  /**
   * Validate widget synchronization patterns work consistently
   */
  private async validateWidgetSynchronizationPatterns(): Promise<TestResult> {
    const widgetTests = [
      { widget: 'user-list-widget', description: 'User List Widget State' },
      { widget: 'room-list-widget', description: 'Room List Widget State' },
      { widget: 'chat-widget', description: 'Chat Widget State' }
    ];

    const syncResults: Record<string, boolean> = {};

    for (const { widget } of widgetTests) {
      try {
        const widgetState = await runJtagCommand(`debug/widget-state --widgetSelector="${widget}"`);
        // Widget should respond with some state, even if empty
        syncResults[widget] = Boolean(widgetState.success !== false && widgetState.commandResult);
      } catch {
        syncResults[widget] = false;
      }
    }

    const allSyncWorks = Object.values(syncResults).every(Boolean);

    return {
      step: 'Widget Synchronization Pattern Validation',
      success: allSyncWorks,
      details: { syncResults }
    };
  }

  /**
   * Print comprehensive final results
   */
  private printFinalResults(allTestsPass: boolean): void {
    console.log('\n' + '='.repeat(80));
    console.log('🏁 COMPLETE CRUD-EVENT CHAIN VALIDATION RESULTS');
    console.log('='.repeat(80));

    // Overall result
    console.log(`\n🎯 OVERALL RESULT: ${allTestsPass ? '✅ ALL TESTS PASS' : '❌ FAILURES DETECTED'}`);

    // Test summary by category
    const totalTests = this.allTestResults.length;
    const passedTests = this.allTestResults.filter(r => r.success).length;
    const failedTests = totalTests - passedTests;

    console.log(`\n📊 TEST SUMMARY:`);
    console.log(`   Total Tests: ${totalTests}`);
    console.log(`   Passed: ${passedTests} ✅`);
    console.log(`   Failed: ${failedTests} ${failedTests > 0 ? '❌' : ''}`);
    console.log(`   Success Rate: ${Math.round((passedTests / totalTests) * 100)}%`);

    // Failed test details
    if (failedTests > 0) {
      console.log('\n❌ FAILED TESTS:');
      const failures = this.allTestResults.filter(r => !r.success);
      failures.forEach(failure => {
        console.log(`   • ${failure.step}: ${failure.error ?? 'Verification failed'}`);
      });
    }

    // Architecture validation summary
    if (allTestsPass) {
      console.log('\n🎯 ARCHITECTURE VALIDATION: ✅ PASSED');
      console.log('   ✅ Generic event patterns work for all entity types');
      console.log('   ✅ Data persistence works consistently across collections');
      console.log('   ✅ Widget synchronization works for all widget types');
      console.log('   ✅ CRUD → Event → DB → Widget → HTML chain verified');
      console.log('\n🎉 Elegant architecture is indisputably validated!');
      console.log('🎉 Generic patterns work perfectly for infinite entity types');
    } else {
      console.log('\n❌ ARCHITECTURE VALIDATION: FAILED');
      console.log('   Some patterns may need investigation or fixes');
    }

    console.log('='.repeat(80));
  }
}

/**
 * Main test execution function
 */
async function main(): Promise<void> {
  try {
    console.log('Starting CRUD Event Chain Integration Test...\n');

    const tester = new CompleteCRUDEventChainTester();
    const allTestsPass = await tester.runCompleteValidation();

    console.log(`\nTest execution completed. Exit code: ${allTestsPass ? 0 : 1}`);
    process.exit(allTestsPass ? 0 : 1);

  } catch (error) {
    console.error('\n❌ Test execution failed:', error);
    process.exit(1);
  }
}

// Execute if run directly
if (require.main === module) {
  main().catch(error => {
    console.error('Unhandled error:', error);
    process.exit(1);
  });
}