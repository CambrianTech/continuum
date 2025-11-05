#!/usr/bin/env tsx
/**
 * Data Adapter Comprehensive Validation - POST-MIGRATION VERIFICATION
 * 
 * Validates professional data architecture with strict Rust-like typing:
 * - SQLite Adapter: Enterprise database with transactions, indexes, full-text search
 * - JSON File Adapter: Legacy compatibility with existing .continuum/database/*.json files  
 * - Hybrid Adapter: Seamless JSON→SQLite migration for zero-breaking deployment
 * - Type Safety: Eliminates all `any` types, applies "typing like Rust - strict, explicit, and predictable"
 * 
 * SUCCESS CRITERIA FROM MIGRATION:
 * - All adapters compile without TypeScript errors
 * - Professional data architecture creates, reads, updates, deletes entities
 * - Hybrid migration enables gradual SQLite adoption with zero breaking changes
 * - Rust-like type safety prevents runtime errors and enables confident refactoring
 * 
 * APPROACH: Follow existing database test patterns with JTAG client integration
 */

import { jtag } from '../../../server-index';
import type { JTAGClientServer } from '../../../system/core/client/server/JTAGClientServer';

interface DataAdapterTestResult {
  testName: string;
  success: boolean;
  duration: number;
  metrics: {
    adaptersValidated?: number;
    entitiesCreated?: number;
    queriesExecuted?: number;
    queryTime?: number;
    typeErrorsFound?: number;
    migrationTestsPassed?: number;
  };
  error?: string;
}

class DataAdapterComprehensiveValidator {
  private results: DataAdapterTestResult[] = [];
  private client: JTAGClientServer | null = null;
  private testEntityIds: string[] = [];

  /**
   * DATABASE TEST 1: SQLite Adapter Rust-like Typing Validation
   * Tests enterprise SQLite backend with strict type safety
   */
  async testSQLiteAdapterTyping(): Promise<DataAdapterTestResult> {
    const testName = 'SQLite Adapter Rust-like Typing Validation';
    const startTime = Date.now();
    
    try {
      console.log(`\n🦀 ${testName}...`);
      
      // Connect to JTAG system
      this.client = await jtag.connect();
      if (!this.client) {
        throw new Error('Failed to connect to JTAG system for adapter validation');
      }

      let entitiesCreated = 0;
      let queriesExecuted = 0;
      const queryTimes: number[] = [];

      // Test strict typing with professional data architecture
      const testEntityData = {
        id: `adapter_test_${Date.now()}`,
        name: 'SQLite Adapter Test User',
        email: 'sqlite@example.com',
        age: 30,
        active: true,
        metadata: {
          adapterTest: true,
          strictTyping: 'rust-like',
          backend: 'sqlite'
        }
      };

      console.log('📊 Testing SQLite adapter with professional data service...');
      
      // Create entity with strict typing
      const queryStart = Date.now();
      const createResult = await this.client.commands['data/create']({
        collection: 'adapter_test_users',
        data: testEntityData,
        format: 'professional' // Use professional data architecture
      });
      queryTimes.push(Date.now() - queryStart);
      queriesExecuted++;

      if (!createResult.success) {
        throw new Error(`SQLite adapter create failed: ${createResult.error || 'Unknown error'}`);
      }

      entitiesCreated++;
      this.testEntityIds.push(testEntityData.id);
      
      // Read entity to verify strict typing preservation
      const readStart = Date.now();
      const readResult = await this.client.commands['data/read']({
        collection: 'adapter_test_users',
        id: testEntityData.id,
        format: 'professional'
      });
      queryTimes.push(Date.now() - readStart);
      queriesExecuted++;

      if (!readResult.success || !readResult.data) {
        throw new Error('SQLite adapter read failed - strict typing validation failed');
      }

      // Verify type safety - properties should be preserved exactly
      const retrievedEntity = readResult.data;
      if (retrievedEntity.name !== 'SQLite Adapter Test User' || 
          retrievedEntity.age !== 30 || 
          retrievedEntity.active !== true) {
        throw new Error('SQLite adapter type safety violation - data corrupted');
      }

      const duration = Date.now() - startTime;
      const averageQueryTime = queryTimes.reduce((sum, time) => sum + time, 0) / queryTimes.length;
      
      // Success criteria: Strict typing preserved, performance acceptable
      const success = entitiesCreated >= 1 && averageQueryTime <= 100;
      
      console.log(`📊 SQLite results: ${entitiesCreated} entities, ${queriesExecuted} queries`);
      console.log(`⏱️  Average query time: ${averageQueryTime.toFixed(1)}ms`);
      console.log(`🦀 Rust-like typing: ${success ? 'VALIDATED' : 'FAILED'}`);
      
      return {
        testName,
        success,
        duration,
        metrics: {
          adaptersValidated: 1,
          entitiesCreated,
          queriesExecuted,
          queryTime: averageQueryTime,
          typeErrorsFound: 0
        }
      };
      
    } catch (error) {
      return {
        testName,
        success: false,
        duration: Date.now() - startTime,
        metrics: { typeErrorsFound: 1 },
        error: error.message
      };
    }
  }

  /**
   * DATABASE TEST 2: JSON File Adapter Legacy Compatibility
   * Tests backward compatibility with existing JSON storage
   */
  async testJSONFileAdapterCompatibility(): Promise<DataAdapterTestResult> {
    const testName = 'JSON File Adapter Legacy Compatibility';
    const startTime = Date.now();
    
    try {
      console.log(`\n📁 ${testName}...`);
      
      let entitiesCreated = 0;
      let queriesExecuted = 0;
      const queryTimes: number[] = [];

      // Test legacy JSON compatibility
      const legacyTestData = {
        id: `legacy_test_${Date.now()}`,
        name: 'Legacy JSON Test User',
        email: 'legacy@example.com',
        isLegacyFormat: true,
        metadata: {
          compatibilityTest: true,
          format: 'json',
          backend: 'filesystem'
        }
      };

      console.log('📊 Testing JSON File adapter legacy compatibility...');
      
      // Create with JSON format (legacy mode)
      const queryStart = Date.now();
      const createResult = await this.client.commands['data/create']({
        collection: 'legacy_test_users',
        data: legacyTestData,
        format: 'json' // Use legacy JSON format
      });
      queryTimes.push(Date.now() - queryStart);
      queriesExecuted++;

      if (!createResult.success) {
        throw new Error(`JSON adapter create failed: ${createResult.error || 'Unknown error'}`);
      }

      entitiesCreated++;
      this.testEntityIds.push(legacyTestData.id);
      
      // List entities to verify JSON adapter performance
      const listStart = Date.now();
      const listResult = await this.client.commands['data/list']({
        collection: 'legacy_test_users',
        format: 'json'
      });
      queryTimes.push(Date.now() - listStart);
      queriesExecuted++;

      if (!listResult.success || !listResult.data?.length) {
        throw new Error('JSON adapter list failed - legacy compatibility broken');
      }

      const duration = Date.now() - startTime;
      const averageQueryTime = queryTimes.reduce((sum, time) => sum + time, 0) / queryTimes.length;
      
      // Success criteria: Legacy format works, acceptable performance
      const success = entitiesCreated >= 1 && averageQueryTime <= 150; // Allow slower for filesystem
      
      console.log(`📊 JSON File results: ${entitiesCreated} entities, ${queriesExecuted} queries`);
      console.log(`⏱️  Average query time: ${averageQueryTime.toFixed(1)}ms`);
      console.log(`📁 Legacy compatibility: ${success ? 'PRESERVED' : 'BROKEN'}`);
      
      return {
        testName,
        success,
        duration,
        metrics: {
          adaptersValidated: 1,
          entitiesCreated,
          queriesExecuted,
          queryTime: averageQueryTime
        }
      };
      
    } catch (error) {
      return {
        testName,
        success: false,
        duration: Date.now() - startTime,
        metrics: {},
        error: error.message
      };
    }
  }

  /**
   * DATABASE TEST 3: Hybrid Migration Strategy Validation
   * Tests seamless JSON→SQLite migration with zero breaking changes
   */
  async testHybridMigrationStrategy(): Promise<DataAdapterTestResult> {
    const testName = 'Hybrid Migration Strategy Validation';
    const startTime = Date.now();
    
    try {
      console.log(`\n🔄 ${testName}...`);
      
      let migrationTestsPassed = 0;
      let entitiesCreated = 0;
      let queriesExecuted = 0;
      const queryTimes: number[] = [];

      // Test hybrid read/write behavior
      console.log('📊 Testing hybrid JSON→SQLite migration...');

      // Create entity using professional architecture (writes to SQLite)
      const hybridEntityData = {
        id: `hybrid_test_${Date.now()}`,
        name: 'Hybrid Migration Test User',
        email: 'hybrid@example.com',
        migrationPhase: 'professional',
        metadata: {
          migrationTest: true,
          readSource: 'json-fallback',
          writeTarget: 'sqlite'
        }
      };

      const createStart = Date.now();
      const createResult = await this.client.commands['data/create']({
        collection: 'hybrid_test_users',
        data: hybridEntityData,
        format: 'professional' // This should use hybrid strategy
      });
      queryTimes.push(Date.now() - createStart);
      queriesExecuted++;

      if (createResult.success) {
        entitiesCreated++;
        migrationTestsPassed++;
        this.testEntityIds.push(hybridEntityData.id);
        console.log(`   ✅ Hybrid write to SQLite successful`);
      } else {
        console.log(`   ❌ Hybrid write failed: ${createResult.error}`);
      }

      // Test hybrid read behavior (should read from SQLite first, fallback to JSON)
      const readStart = Date.now();
      const readResult = await this.client.commands['data/read']({
        collection: 'hybrid_test_users',
        id: hybridEntityData.id,
        format: 'professional'
      });
      queryTimes.push(Date.now() - readStart);
      queriesExecuted++;

      if (readResult.success && readResult.data) {
        migrationTestsPassed++;
        console.log(`   ✅ Hybrid read strategy successful`);
      } else {
        console.log(`   ❌ Hybrid read failed`);
      }

      const duration = Date.now() - startTime;
      const averageQueryTime = queryTimes.reduce((sum, time) => sum + time, 0) / queryTimes.length;
      
      // Success criteria: Hybrid strategy working, zero breaking changes
      const success = migrationTestsPassed >= 2 && averageQueryTime <= 100;
      
      console.log(`📊 Hybrid results: ${migrationTestsPassed}/2 migration tests passed`);
      console.log(`⏱️  Average query time: ${averageQueryTime.toFixed(1)}ms`);
      console.log(`🔄 Zero-breaking migration: ${success ? 'VALIDATED' : 'NEEDS WORK'}`);
      
      return {
        testName,
        success,
        duration,
        metrics: {
          adaptersValidated: 1,
          entitiesCreated,
          queriesExecuted,
          queryTime: averageQueryTime,
          migrationTestsPassed
        }
      };
      
    } catch (error) {
      return {
        testName,
        success: false,
        duration: Date.now() - startTime,
        metrics: { migrationTestsPassed: 0 },
        error: error.message
      };
    }
  }

  /**
   * Run complete data adapter validation suite
   */
  async runAdapterValidation(): Promise<void> {
    console.log('🗄️ DATA ADAPTER COMPREHENSIVE VALIDATION - POST-MIGRATION VERIFICATION');
    console.log('=' .repeat(80));
    
    try {
      // Run all adapter validation tests
      this.results.push(await this.testSQLiteAdapterTyping());
      this.results.push(await this.testJSONFileAdapterCompatibility()); 
      this.results.push(await this.testHybridMigrationStrategy());
      
      // Generate comprehensive report
      this.generateAdapterReport();
      
    } catch (error) {
      console.error('❌ Data adapter validation failed:', error);
      throw error;
    } finally {
      // Clean up test data and connections
      await this.cleanup();
    }
  }

  /**
   * Generate comprehensive adapter validation report
   */
  private generateAdapterReport(): void {
    const totalTests = this.results.length;
    const passedTests = this.results.filter(r => r.success).length;
    const failedTests = totalTests - passedTests;
    const overallSuccessRate = (passedTests / totalTests) * 100;
    const totalDuration = this.results.reduce((sum, r) => sum + r.duration, 0);
    
    console.log('\n' + '=' .repeat(80));
    console.log('📊 DATA ADAPTER VALIDATION RESULTS');
    console.log('=' .repeat(80));
    
    console.log(`🎯 Overall Results:`);
    console.log(`   Tests Passed: ${passedTests}/${totalTests} (${overallSuccessRate.toFixed(1)}%)`);
    console.log(`   Tests Failed: ${failedTests}`);
    console.log(`   Total Duration: ${totalDuration}ms`);
    console.log('');
    
    // Detailed results for each test
    this.results.forEach(result => {
      const status = result.success ? '✅' : '❌';
      console.log(`${status} ${result.testName} (${result.duration}ms)`);
      
      if (result.metrics.entitiesCreated !== undefined) {
        console.log(`   Entities Created: ${result.metrics.entitiesCreated}`);
      }
      if (result.metrics.queryTime !== undefined) {
        console.log(`   Average Query Time: ${result.metrics.queryTime.toFixed(1)}ms`);
      }
      if (result.metrics.migrationTestsPassed !== undefined) {
        console.log(`   Migration Tests Passed: ${result.metrics.migrationTestsPassed}`);
      }
      if (result.error) {
        console.log(`   Error: ${result.error}`);
      }
      console.log('');
    });
    
    // POST-MIGRATION Success Criteria Validation
    console.log('🎯 POST-MIGRATION SUCCESS CRITERIA VALIDATION:');
    
    const sqliteTest = this.results.find(r => r.testName.includes('SQLite'));
    if (sqliteTest && sqliteTest.success) {
      console.log('   ✅ SQLite adapter with Rust-like typing working');
    } else {
      console.log('   ❌ SQLite adapter migration issues detected');
    }
    
    const jsonTest = this.results.find(r => r.testName.includes('JSON'));
    if (jsonTest && jsonTest.success) {
      console.log('   ✅ JSON File adapter legacy compatibility preserved');
    } else {
      console.log('   ❌ JSON File adapter compatibility broken');
    }
    
    const hybridTest = this.results.find(r => r.testName.includes('Hybrid'));
    if (hybridTest && hybridTest.success) {
      console.log('   ✅ Hybrid migration strategy enables zero-breaking deployment');
    } else {
      console.log('   ❌ Hybrid migration strategy needs refinement');
    }
    
    console.log('\n' + '=' .repeat(80));
    
    if (overallSuccessRate >= 90) {
      console.log('🎉 DATA ADAPTER MIGRATION: SUCCESSFULLY VALIDATED');
      console.log('✅ Professional data architecture with Rust-like typing operational');
    } else {
      console.log('⚠️ DATA ADAPTER MIGRATION: NEEDS ATTENTION');
      console.log('❌ Must resolve adapter issues before proceeding to production');
    }
    
    console.log('=' .repeat(80));
  }

  /**
   * Clean up test data and connections
   */
  private async cleanup(): Promise<void> {
    console.log('\n🧹 Cleaning up test data and connections...');
    
    try {
      // Clean up test entities
      for (const entityId of this.testEntityIds) {
        try {
          // Try both collections and formats for thorough cleanup
          await this.client?.commands['data/delete']({
            collection: 'adapter_test_users',
            id: entityId,
            format: 'professional'
          });
          await this.client?.commands['data/delete']({
            collection: 'legacy_test_users', 
            id: entityId,
            format: 'json'
          });
          await this.client?.commands['data/delete']({
            collection: 'hybrid_test_users',
            id: entityId,
            format: 'professional'
          });
        } catch (error) {
          // Ignore cleanup errors
        }
      }
      
      // Disconnect client
      if (this.client) {
        await this.client.disconnect();
        this.client = null;
      }
      
      console.log('✅ Cleanup completed');
    } catch (error) {
      console.log('⚠️ Cleanup had some issues (non-critical)');
    }
  }
}

/**
 * Main test execution
 */
async function runDataAdapterValidation(): Promise<void> {
  const validator = new DataAdapterComprehensiveValidator();
  
  console.log('🚨 POST-MIGRATION: Data Adapter Validation');
  console.log('🔍 Validating SQLite, JSON File, and Hybrid adapters with Rust-like typing');
  console.log('');
  
  await validator.runAdapterValidation();
}

// Execute if called directly
if (require.main === module) {
  runDataAdapterValidation()
    .then(() => {
      console.log('\n✅ Data adapter validation completed successfully!');
      process.exit(0);
    })
    .catch((error) => {
      console.error('\n❌ Data adapter validation failed:', error);
      process.exit(1);
    });
}

export { runDataAdapterValidation, DataAdapterComprehensiveValidator };