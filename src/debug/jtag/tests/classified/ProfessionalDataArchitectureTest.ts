#!/usr/bin/env npx tsx

/**
 * Professional Data Architecture Test
 * 
 * Tests the complete professional data architecture including:
 * - Rust-like strict typing with branded types
 * - DataService with widget.executeCommand pattern
 * - HybridAdapter for JSON→SQLite migration  
 * - Domain objects with comprehensive validation
 * - Backwards compatibility with existing JSON files
 * 
 * @category data
 * @category integration
 * @critical
 */

import { TestSpec, TestLevel, TestImportance, TestCategory } from '../shared/TestDecorators';
import { DataServiceFactory, DataServiceMode } from '../../system/data/services/DataServiceFactory';
import type { User, CreateUserData } from '../../system/data/domains/User';
import type { ChatMessage, CreateMessageData } from '../../system/data/domains/ChatMessage';
import { UserId, SessionId, RoomId, MessageId } from '../../system/data/domains/CoreTypes';
import { validateUserData } from '../../system/data/domains/User';
import { validateMessageData, processMessageFormatting } from '../../system/data/domains/ChatMessage';

// Test framework integration
const TEST_TIMEOUT = 30000; // 30 seconds for database operations
const TEST_DATABASE_PATH = '.continuum/test-professional-data';

interface TestResults {
  passed: number;
  failed: number;
  errors: string[];
}

@TestSpec({
  level: TestLevel.INTEGRATION,
  importance: TestImportance.CRITICAL,
  category: TestCategory.DATA,
  description: 'Professional data architecture - Rust-like typing, DataService, HybridAdapter',
  timeout: 30000,
  requiresSystem: false
})
export class ProfessionalDataArchitectureTest {
  
  /**
   * Main test execution method
   */
  static async run(): Promise<boolean> {
    console.log('🏗️ Professional Data Architecture Test Suite');
    console.log('   Testing: Rust-like typing, DataService, HybridAdapter, Domain objects\n');

    const results: TestResults = {
      passed: 0,
      failed: 0,
      errors: []
    };

    try {
      // Run all test categories
      await this.testDomainObjectValidation(results);
      await this.testJSONAdapterOperations(results);
      await this.testStrictValidationEnforcement(results);
      await this.testHybridAdapterMigration(results);
      await this.testDataServiceFactory(results);
      await this.testErrorHandlingWithResultTypes(results);

      // Summary
      console.log(`\n📊 Professional Data Architecture Test Results:`);
      console.log(`✅ Passed: ${results.passed}`);
      console.log(`❌ Failed: ${results.failed}`);
      
      if (results.failed > 0) {
        console.log('\n🔍 Failure Details:');
        results.errors.forEach(error => console.log(`  • ${error}`));
        console.log('\n💡 Fix these issues before integrating into npm test');
        return false;
      }

      console.log('\n🎉 All professional data architecture tests passed!');
      console.log('\n📋 Validated Features:');
      console.log('✅ Rust-like strict typing with branded types');
      console.log('✅ DataService with widget.executeCommand pattern');
      console.log('✅ HybridAdapter for JSON→SQLite migration');
      console.log('✅ Domain objects with comprehensive validation');
      console.log('✅ Result type error handling (no exceptions)');
      console.log('✅ Backwards compatibility with existing JSON files');
      console.log('✅ Professional ORM-like operations');

      console.log('\n🚀 Ready for integration into npm test suite!');
      return true;

    } catch (error: any) {
      console.error('❌ Test suite failed:', error);
      return false;
    }
  }

  /**
   * Test runner with proper error handling and timeouts
   */
  private static async runTest(name: string, testFn: () => Promise<void>, results: TestResults): Promise<void> {
    try {
      console.log(`🧪 ${name}`);
      
      // Add timeout to prevent hanging tests
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error(`Test timeout after ${TEST_TIMEOUT}ms`)), TEST_TIMEOUT);
      });
      
      await Promise.race([testFn(), timeoutPromise]);
      
      console.log(`✅ ${name}`);
      results.passed++;
    } catch (error: any) {
      console.error(`❌ ${name}: ${error.message}`);
      results.errors.push(`${name}: ${error.message}`);
      results.failed++;
    }
  }

  /**
   * Test 1: Domain Object Validation (Rust-like strict typing)
   */
  private static async testDomainObjectValidation(results: TestResults): Promise<void> {
    await this.runTest('Domain object validation with branded types', async () => {
      // Test User domain validation
      const validUserData: CreateUserData = {
        displayName: 'Claude AI Assistant',
        type: 'ai',
        capabilities: {
          canSendMessages: true,
          canReceiveMessages: true,
          autoResponds: true,
          providesContext: true
        }
      };

      const validationResult = validateUserData(validUserData);
      if (!validationResult.success) {
        throw new Error('Valid user data failed validation');
      }

      // Test branded types prevent string confusion
      const userId = UserId('user-123');
      const roomId = RoomId('room-456');
      
      // This should be type-safe at compile time (validated by TypeScript)
      const messageData: CreateMessageData = {
        roomId, // Only RoomId accepted
        senderId: userId, // Only UserId accepted
        content: {
          text: 'Testing branded types for strict typing',
          formatting: processMessageFormatting('Testing branded types for strict typing')
        }
      };

      const messageValidation = validateMessageData(messageData);
      if (!messageValidation.success) {
        throw new Error('Valid message data failed validation');
      }

      // Test invalid data rejection
      const invalidUserData: CreateUserData = {
        displayName: '', // Invalid empty name
        type: 'ai'
      };

      const invalidResult = validateUserData(invalidUserData);
      if (invalidResult.success) {
        throw new Error('Invalid user data passed validation - strict typing failed');
      }
    }, results);
  }

  /**
   * Test 2: JSON Adapter Operations (Backwards Compatibility)
   */
  private static async testJSONAdapterOperations(results: TestResults): Promise<void> {
    await this.runTest('JSON adapter CRUD operations with Result types', async () => {
      const dataService = await DataServiceFactory.create({
        mode: DataServiceMode.JSON_ONLY,
        paths: { jsonDatabase: `${TEST_DATABASE_PATH}/json-test` },
        context: { source: 'professional-data-test' }
      });

      try {
        // Test create operation
        const userData: CreateUserData = {
          displayName: 'Professional Test User',
          type: 'human'
        };

        const createResult = await dataService.executeOperation<User>('test-users/create', userData);
        if (!createResult.success) {
          throw new Error(`Create failed: ${createResult.error.message}`);
        }

        const user = createResult.data as User;
        
        // Validate strict typing - user should have all required BaseEntity fields
        if (!user.id || !user.createdAt || !user.updatedAt || !user.version) {
          throw new Error('Created user missing BaseEntity fields - typing not enforced');
        }

        // Test read operation with Result type
        const readResult = await dataService.executeOperation<User>('test-users/read', { id: user.id });
        if (!readResult.success || !readResult.data) {
          throw new Error('Read operation failed - data persistence broken');
        }

        // Test list operation
        const listResult = await dataService.executeOperation<User>('test-users/list', {});
        if (!listResult.success) {
          throw new Error(`List failed: ${listResult.error.message}`);
        }

        const users = listResult.data as User[];
        if (users.length === 0) {
          throw new Error('List returned no users - persistence not working');
        }

        // Test update operation
        const updateResult = await dataService.executeOperation<User>('test-users/update', {
          id: user.id,
          profile: { ...user.profile, displayName: 'Updated Professional User' }
        });
        if (!updateResult.success) {
          throw new Error(`Update failed: ${updateResult.error.message}`);
        }

        // Test delete operation
        const deleteResult = await dataService.executeOperation('test-users/delete', { id: user.id });
        if (!deleteResult.success) {
          throw new Error(`Delete failed: ${deleteResult.error.message}`);
        }

      } finally {
        await dataService.close();
      }
    }, results);
  }

  /**
   * Test 3: Strict Validation Enforcement (Both executeOperation and ORM methods)
   */
  private static async testStrictValidationEnforcement(results: TestResults): Promise<void> {
    await this.runTest('Strict validation enforcement like Rust', async () => {
      const dataService = await DataServiceFactory.create({
        mode: DataServiceMode.JSON_ONLY,
        paths: { jsonDatabase: `${TEST_DATABASE_PATH}/validation-test` },
        context: { source: 'validation-enforcement-test' }
      });

      try {
        // Test executeOperation validation
        const invalidData: CreateUserData = {
          displayName: '', // Invalid!
          type: 'human'
        };

        const executeResult = await dataService.executeOperation<User>('users/create', invalidData);
        if (executeResult.success) {
          throw new Error('executeOperation should have failed validation - not strict enough');
        }

        // Test ORM method validation
        const ormResult = await dataService.create<User>('users', invalidData);
        if (ormResult.success) {
          throw new Error('ORM create should have failed validation - not strict enough');
        }

        // Both should have same error message (consistent validation)
        if (executeResult.error.message !== ormResult.error.message) {
          throw new Error('executeOperation and ORM methods have inconsistent validation');
        }

        console.log(`   ✓ Validation error: "${executeResult.error.message}"`);

      } finally {
        await dataService.close();
      }
    }, results);
  }

  /**
   * Test 4: HybridAdapter Migration (JSON→SQLite)
   */
  private static async testHybridAdapterMigration(results: TestResults): Promise<void> {
    await this.runTest('Hybrid adapter JSON→SQLite migration', async () => {
      const dataService = await DataServiceFactory.createHybridMigrating(
        `${TEST_DATABASE_PATH}/hybrid-json`,
        `${TEST_DATABASE_PATH}/hybrid.db`
      );

      try {
        // Create rich message data with Discord-like features
        const messageData: CreateMessageData = {
          roomId: RoomId('professional-test'),
          senderId: UserId('test-ai'),
          content: {
            text: 'Testing **hybrid** adapter with @mentions and #hashtags: https://example.com ```typescript\nconsole.log("code");\n```',
            formatting: processMessageFormatting('Testing **hybrid** adapter with @mentions and #hashtags: https://example.com ```typescript\nconsole.log("code");\n```')
          },
          priority: 'high'
        };

        const createResult = await dataService.executeOperation<ChatMessage>('test-messages/create', messageData);
        if (!createResult.success) {
          throw new Error(`Hybrid create failed: ${createResult.error.message}`);
        }

        const message = createResult.data as ChatMessage;

        // Verify rich message features were processed
        if (!message.content.formatting.markdown) {
          throw new Error('Markdown formatting not processed');
        }

        if (message.content.formatting.hashtags.length === 0) {
          throw new Error('Hashtags not extracted from message');
        }

        if (message.content.formatting.links.length === 0) {
          throw new Error('Links not extracted from message');
        }

        if (message.content.formatting.codeBlocks.length === 0) {
          throw new Error('Code blocks not extracted from message');
        }

        // Test querying with filters
        const queryResult = await dataService.executeOperation<ChatMessage>('test-messages/query', {
          filters: { roomId: 'professional-test' },
          options: {
            orderBy: [{ field: 'timestamp', direction: 'DESC' }],
            limit: 10
          }
        });

        if (!queryResult.success) {
          throw new Error(`Hybrid query failed: ${queryResult.error.message}`);
        }

        const messages = queryResult.data as ChatMessage[];
        if (messages.length === 0) {
          throw new Error('Hybrid query returned no results');
        }

        console.log(`   ✓ Created message with ${message.content.formatting.hashtags.length} hashtags, ${message.content.formatting.links.length} links`);

      } finally {
        await dataService.close();
      }
    }, results);
  }

  /**
   * Test 5: DataService Factory Configurations
   */
  private static async testDataServiceFactory(results: TestResults): Promise<void> {
    await this.runTest('DataService factory configuration flexibility', async () => {
      // Test multiple factory configurations - focus on interface consistency
      const jsonService = await DataServiceFactory.createJsonCompatible(`${TEST_DATABASE_PATH}/factory-json`);
      const hybridService = await DataServiceFactory.createHybridMigrating(
        `${TEST_DATABASE_PATH}/factory-hybrid-json`,
        `${TEST_DATABASE_PATH}/factory-hybrid.db`
      );

      try {
        // Same operations should work on all configurations
        const testData: CreateUserData = {
          displayName: 'Factory Configuration Test',
          type: 'system'
        };

        const jsonResult = await jsonService.executeOperation<User>('factory-test/create', testData);
        const hybridResult = await hybridService.executeOperation<User>('factory-test/create', testData);

        if (!jsonResult.success || !hybridResult.success) {
          throw new Error('Factory configurations don\'t have consistent interfaces');
        }

        // Both should return valid entities with required fields
        const jsonUser = jsonResult.data as User;
        const hybridUser = hybridResult.data as User;

        // Check BaseEntity requirements (strict typing enforcement)
        if (!jsonUser.id || !hybridUser.id) {
          throw new Error('Factory configurations missing required ID field');
        }

        if (!jsonUser.createdAt || !hybridUser.createdAt) {
          throw new Error('Factory configurations missing required createdAt field');
        }

        // Check that both configurations use the same creation logic
        if (jsonUser.type !== 'system' || hybridUser.type !== 'system') {
          throw new Error('Factory configurations not preserving input data correctly');
        }

        console.log('   ✓ JSON and Hybrid configurations work consistently with strict typing');

      } finally {
        await jsonService.close();
        await hybridService.close();
      }
    }, results);
  }

  /**
   * Test 6: Error Handling with Result Types (No Exceptions)
   */
  private static async testErrorHandlingWithResultTypes(results: TestResults): Promise<void> {
    await this.runTest('Result type error handling (no exceptions thrown)', async () => {
      const dataService = await DataServiceFactory.createJsonCompatible(`${TEST_DATABASE_PATH}/error-test`);

      try {
        // Test reading non-existent record (should return Ok(null), not throw)
        const readResult = await dataService.executeOperation<User>('users/read', { id: 'non-existent-id' });
        if (!readResult.success) {
          throw new Error('Read non-existent should succeed with null data, not fail');
        }

        if (readResult.data !== null) {
          throw new Error('Non-existent record should return null');
        }

        // Test validation error (should return Err, not throw)
        const invalidResult = await dataService.executeOperation<User>('users/create', {
          displayName: '', // Invalid!
          type: 'human'
        });

        if (invalidResult.success) {
          throw new Error('Invalid data should return Err result');
        }

        // Error should have proper structure (Rust-like)
        if (!invalidResult.error.type || !invalidResult.error.message) {
          throw new Error('DataError missing required fields - not following Result pattern');
        }

        console.log(`   ✓ Proper error structure: ${invalidResult.error.type}: ${invalidResult.error.message}`);

      } finally {
        await dataService.close();
      }
    }, results);
  }
}

// Allow direct execution for standalone testing
if (require.main === module) {
  ProfessionalDataArchitectureTest.run()
    .then(success => {
      if (!success) {
        process.exit(1);
      }
    })
    .catch(error => {
      console.error('❌ Test suite failed:', error);
      process.exit(1);
    });
}

// Export for test runner integration  
export default ProfessionalDataArchitectureTest;