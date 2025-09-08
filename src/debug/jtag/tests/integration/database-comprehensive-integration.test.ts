/**
 * Database Comprehensive Integration Test Suite
 * 
 * Tests all database and persistence functionality end-to-end.
 * Validates data-daemon, storage adapters, and cross-collection operations.
 */

import { execSync } from 'child_process';
import path from 'path';

interface DatabaseTestResult {
  testName: string;
  success: boolean;
  details: any;
  timestamp: string;
  error?: string;
}

class DatabaseComprehensiveIntegrationTest {
  private results: DatabaseTestResult[] = [];
  private testCollections = ['users', 'messages', 'sessions', 'personas', 'rooms'];
  private testData: Record<string, any> = {};

  constructor() {
    // Generate test data for each collection
    const timestamp = Date.now();
    this.testData = {
      users: {
        userId: `test_user_${timestamp}`,
        name: 'Integration Test User',
        type: 'human',
        created: new Date().toISOString()
      },
      messages: {
        messageId: `test_msg_${timestamp}`,
        roomId: 'integration_test_room',
        userId: `test_user_${timestamp}`,
        message: 'Database integration test message',
        timestamp: new Date().toISOString()
      },
      sessions: {
        sessionId: `test_session_${timestamp}`,
        userId: `test_user_${timestamp}`,
        active: true,
        created: new Date().toISOString()
      },
      personas: {
        personaId: `test_persona_${timestamp}`,
        name: 'Test Database Persona',
        type: 'ai_assistant',
        capabilities: ['chat', 'analysis']
      },
      rooms: {
        roomId: 'integration_test_room',
        name: 'Database Integration Test Room',
        type: 'public',
        created: new Date().toISOString()
      }
    };
  }

  /**
   * Execute JTAG command and parse result
   */
  private executeJTAGCommand(command: string): any {
    try {
      const fullCommand = `./jtag ${command}`;
      const output = execSync(fullCommand, { 
        cwd: path.resolve('.'),
        encoding: 'utf-8',
        timeout: 30000
      });
      
      // Extract JSON result from CLI output
      const lines = output.split('\n');
      const resultStartIndex = lines.findIndex(line => line.includes('COMMAND RESULT:'));
      if (resultStartIndex === -1) {
        throw new Error('No COMMAND RESULT found in output');
      }
      
      const jsonLines = lines.slice(resultStartIndex + 1);
      const jsonEndIndex = jsonLines.findIndex(line => line.includes('===='));
      const jsonContent = jsonLines.slice(0, jsonEndIndex).join('\n');
      
      return JSON.parse(jsonContent);
    } catch (error) {
      throw new Error(`JTAG command failed: ${command}\nError: ${error.message}`);
    }
  }

  /**
   * Test 1: Basic CRUD operations across all collections
   */
  async testBasicCRUDOperations(): Promise<DatabaseTestResult> {
    const testName = 'Basic CRUD Operations';
    
    try {
      const operations = [];
      
      // CREATE: Insert test data into each collection
      for (const [collection, data] of Object.entries(this.testData)) {
        try {
          const createResult = this.executeJTAGCommand(
            `data/create --collection="${collection}" --data='${JSON.stringify(data)}'`
          );
          operations.push({
            operation: 'CREATE',
            collection,
            success: createResult.success === true,
            result: createResult
          });
        } catch (error) {
          operations.push({
            operation: 'CREATE',
            collection,
            success: false,
            error: error.message
          });
        }
      }
      
      // READ: Retrieve data from each collection
      for (const collection of this.testCollections) {
        try {
          const readResult = this.executeJTAGCommand(
            `data/list --collection="${collection}" --format="json"`
          );
          const hasData = readResult.success === true && 
                         readResult.items && 
                         readResult.items.length > 0;
          
          operations.push({
            operation: 'READ',
            collection,
            success: hasData,
            itemCount: readResult.items?.length || 0
          });
        } catch (error) {
          operations.push({
            operation: 'READ',
            collection,
            success: false,
            error: error.message
          });
        }
      }
      
      // Calculate success rate
      const successfulOps = operations.filter(op => op.success).length;
      const totalOps = operations.length;
      const success = successfulOps === totalOps;
      
      const testResult: DatabaseTestResult = {
        testName,
        success,
        details: {
          operations,
          successRate: `${successfulOps}/${totalOps}`,
          collectionstested: this.testCollections.length
        },
        timestamp: new Date().toISOString()
      };
      
      if (!success) {
        const failedOps = operations.filter(op => !op.success);
        testResult.error = `${failedOps.length} operations failed: ${failedOps.map(op => `${op.operation}:${op.collection}`).join(', ')}`;
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: DatabaseTestResult = {
        testName,
        success: false,
        details: { collections: this.testCollections },
        timestamp: new Date().toISOString(),
        error: error.message
      };
      this.results.push(errorResult);
      return errorResult;
    }
  }

  /**
   * Test 2: Cross-collection relational queries
   */
  async testCrossCollectionQueries(): Promise<DatabaseTestResult> {
    const testName = 'Cross-Collection Queries';
    
    try {
      const queries = [];
      
      // Query 1: Find messages by user
      try {
        const userMessages = this.executeJTAGCommand(
          `data/list --collection="messages" --filter='{"userId":"${this.testData.users.userId}"}'`
        );
        queries.push({
          query: 'Messages by User',
          success: userMessages.success && userMessages.commandResult?.items?.length > 0,
          resultCount: userMessages.commandResult?.items?.length || 0
        });
      } catch (error) {
        queries.push({
          query: 'Messages by User',
          success: false,
          error: error.message
        });
      }
      
      // Query 2: Find sessions by user
      try {
        const userSessions = this.executeJTAGCommand(
          `data/list --collection="sessions" --filter='{"userId":"${this.testData.users.userId}"}'`
        );
        queries.push({
          query: 'Sessions by User',
          success: userSessions.success && userSessions.commandResult?.items?.length > 0,
          resultCount: userSessions.commandResult?.items?.length || 0
        });
      } catch (error) {
        queries.push({
          query: 'Sessions by User',
          success: false,
          error: error.message
        });
      }
      
      // Query 3: Find messages in room
      try {
        const roomMessages = this.executeJTAGCommand(
          `data/list --collection="messages" --filter='{"roomId":"${this.testData.messages.roomId}"}'`
        );
        queries.push({
          query: 'Messages in Room',
          success: roomMessages.success && roomMessages.commandResult?.items?.length > 0,
          resultCount: roomMessages.commandResult?.items?.length || 0
        });
      } catch (error) {
        queries.push({
          query: 'Messages in Room',
          success: false,
          error: error.message
        });
      }
      
      const successfulQueries = queries.filter(q => q.success).length;
      const success = successfulQueries === queries.length;
      
      const testResult: DatabaseTestResult = {
        testName,
        success,
        details: {
          queries,
          successRate: `${successfulQueries}/${queries.length}`,
          relationalQueriesWorking: success
        },
        timestamp: new Date().toISOString()
      };
      
      if (!success) {
        const failedQueries = queries.filter(q => !q.success);
        testResult.error = `${failedQueries.length} cross-collection queries failed: ${failedQueries.map(q => q.query).join(', ')}`;
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: DatabaseTestResult = {
        testName,
        success: false,
        details: {},
        timestamp: new Date().toISOString(),
        error: error.message
      };
      this.results.push(errorResult);
      return errorResult;
    }
  }

  /**
   * Test 3: Data persistence across system restart simulation
   */
  async testDataPersistence(): Promise<DatabaseTestResult> {
    const testName = 'Data Persistence';
    
    try {
      // First, verify our test data exists
      const beforeCheck = this.executeJTAGCommand(
        `data/list --collection="users" --filter='{"userId":"${this.testData.users.userId}"}'`
      );
      
      const dataExistsBefore = beforeCheck.success && 
                              beforeCheck.commandResult?.items?.length > 0;
      
      if (!dataExistsBefore) {
        throw new Error('Test data not found before persistence test - run basic CRUD first');
      }
      
      // Simulate persistence by checking data still exists after some time
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      // Re-query the data
      const afterCheck = this.executeJTAGCommand(
        `data/list --collection="users" --filter='{"userId":"${this.testData.users.userId}"}'`
      );
      
      const dataExistsAfter = afterCheck.success && 
                             afterCheck.commandResult?.items?.length > 0;
      
      // Verify data integrity
      const retrievedData = afterCheck.commandResult?.items?.[0];
      const dataIntegrityOK = retrievedData && 
                             retrievedData.userId === this.testData.users.userId &&
                             retrievedData.name === this.testData.users.name;
      
      const success = dataExistsBefore && dataExistsAfter && dataIntegrityOK;
      
      const testResult: DatabaseTestResult = {
        testName,
        success,
        details: {
          dataExistsBefore,
          dataExistsAfter,
          dataIntegrityOK,
          retrievedData,
          originalData: this.testData.users
        },
        timestamp: new Date().toISOString()
      };
      
      if (!success) {
        if (!dataExistsBefore) {
          testResult.error = 'Test data not found initially';
        } else if (!dataExistsAfter) {
          testResult.error = 'Data not persisted - lost after time delay';
        } else if (!dataIntegrityOK) {
          testResult.error = 'Data corrupted during persistence';
        }
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: DatabaseTestResult = {
        testName,
        success: false,
        details: { testUserId: this.testData.users.userId },
        timestamp: new Date().toISOString(),
        error: error.message
      };
      this.results.push(errorResult);
      return errorResult;
    }
  }

  /**
   * Test 4: Update and Delete operations
   */
  async testUpdateDeleteOperations(): Promise<DatabaseTestResult> {
    const testName = 'Update & Delete Operations';
    
    try {
      const operations = [];
      
      // UPDATE: Modify existing data
      const updatedData = {
        ...this.testData.users,
        name: 'Updated Integration Test User',
        lastModified: new Date().toISOString()
      };
      
      try {
        const updateResult = this.executeJTAGCommand(
          `data/update --collection="users" --filter='{"userId":"${this.testData.users.userId}"}' --data='${JSON.stringify(updatedData)}'`
        );
        
        const updateSuccess = updateResult.success && updateResult.commandResult?.success;
        
        // Verify update
        let verifySuccess = false;
        if (updateSuccess) {
          const verifyResult = this.executeJTAGCommand(
            `data/list --collection="users" --filter='{"userId":"${this.testData.users.userId}"}'`
          );
          const retrievedUser = verifyResult.commandResult?.items?.[0];
          verifySuccess = retrievedUser && retrievedUser.name === updatedData.name;
        }
        
        operations.push({
          operation: 'UPDATE',
          success: updateSuccess && verifySuccess,
          details: { updateSuccess, verifySuccess }
        });
      } catch (error) {
        operations.push({
          operation: 'UPDATE',
          success: false,
          error: error.message
        });
      }
      
      // DELETE: Remove test data
      for (const [collection, data] of Object.entries(this.testData)) {
        try {
          let filter = {};
          
          // Create appropriate filter for each collection
          if (collection === 'users') filter = { userId: data.userId };
          else if (collection === 'messages') filter = { messageId: data.messageId };
          else if (collection === 'sessions') filter = { sessionId: data.sessionId };
          else if (collection === 'personas') filter = { personaId: data.personaId };
          else if (collection === 'rooms') filter = { roomId: data.roomId };
          
          const deleteResult = this.executeJTAGCommand(
            `data/delete --collection="${collection}" --filter='${JSON.stringify(filter)}'`
          );
          
          operations.push({
            operation: 'DELETE',
            collection,
            success: deleteResult.success && deleteResult.commandResult?.success,
            filter
          });
        } catch (error) {
          operations.push({
            operation: 'DELETE',
            collection,
            success: false,
            error: error.message
          });
        }
      }
      
      const successfulOps = operations.filter(op => op.success).length;
      const success = successfulOps === operations.length;
      
      const testResult: DatabaseTestResult = {
        testName,
        success,
        details: {
          operations,
          successRate: `${successfulOps}/${operations.length}`,
          updateDeleteWorking: success
        },
        timestamp: new Date().toISOString()
      };
      
      if (!success) {
        const failedOps = operations.filter(op => !op.success);
        testResult.error = `${failedOps.length} update/delete operations failed`;
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: DatabaseTestResult = {
        testName,
        success: false,
        details: {},
        timestamp: new Date().toISOString(),
        error: error.message
      };
      this.results.push(errorResult);
      return errorResult;
    }
  }

  /**
   * Run complete database integration test suite
   */
  async runCompleteDatabaseIntegrationTest(): Promise<{
    overallSuccess: boolean;
    results: DatabaseTestResult[];
    summary: {
      total: number;
      passed: number;
      failed: number;
      criticalGaps: string[];
    };
  }> {
    console.log('🗄️  Starting Database Comprehensive Integration Test Suite');
    
    // Run all tests in sequence
    await this.testBasicCRUDOperations();
    await this.testCrossCollectionQueries();
    await this.testDataPersistence();
    await this.testUpdateDeleteOperations();
    
    // Calculate summary
    const passed = this.results.filter(r => r.success).length;
    const failed = this.results.filter(r => !r.success).length;
    const overallSuccess = failed === 0;
    
    // Identify critical gaps
    const criticalGaps: string[] = [];
    this.results.forEach(result => {
      if (!result.success && result.error) {
        criticalGaps.push(result.error);
      }
    });
    
    const summary = {
      total: this.results.length,
      passed,
      failed,
      criticalGaps
    };
    
    // Report results
    console.log('\n📊 DATABASE INTEGRATION TEST RESULTS:');
    this.results.forEach((result, index) => {
      const status = result.success ? '✅' : '❌';
      console.log(`   ${index + 1}. ${status} ${result.testName}`);
      if (!result.success && result.error) {
        console.log(`      Error: ${result.error}`);
      }
    });
    
    console.log(`\n🎯 Summary: ${passed}/${this.results.length} tests passed`);
    
    if (criticalGaps.length > 0) {
      console.log('\n🚨 DATABASE INTEGRATION GAPS:');
      criticalGaps.forEach((gap, index) => {
        console.log(`   ${index + 1}. ${gap}`);
      });
    }
    
    return {
      overallSuccess,
      results: this.results,
      summary
    };
  }
}

/**
 * Main test execution
 */
async function main() {
  try {
    const dbTest = new DatabaseComprehensiveIntegrationTest();
    const results = await dbTest.runCompleteDatabaseIntegrationTest();
    
    if (results.overallSuccess) {
      console.log('\n🎉 ALL DATABASE INTEGRATION TESTS PASSED!');
      process.exit(0);
    } else {
      console.log('\n❌ DATABASE INTEGRATION TESTS FAILED');
      console.log('Review results above for specific issues');
      process.exit(1);
    }
  } catch (error) {
    console.error('💥 Database integration test execution failed:', error);
    process.exit(1);
  }
}

// Run test if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}

export { DatabaseComprehensiveIntegrationTest };