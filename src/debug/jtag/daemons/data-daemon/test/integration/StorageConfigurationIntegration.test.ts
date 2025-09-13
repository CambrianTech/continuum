#!/usr/bin/env tsx
/**
 * Storage Configuration Integration Tests
 * 
 * Tests the professional storage configuration system we built following
 * Rust-like conventions with defaults next to types.
 * 
 * Category: database - part of npm run test:database
 * Uses the reusable command pattern - all tests use actual JTAG commands
 */

import { jtag } from '../../../../server-index';
import type { JTAGClientServer } from '../../../../system/core/client/server/JTAGClientServer';
import { DEFAULT_STORAGE_CONFIG } from '../../../../system/shared/SecureConfigTypes';

interface StorageConfigTestResult {
  testName: string;
  success: boolean;
  duration: number;
  error?: string;
}

console.log('🗄️ Storage Configuration Integration Tests');

function assert(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

/**
 * Storage Configuration Integration Tests
 */
class StorageConfigurationValidator {
  private results: StorageConfigTestResult[] = [];
  private client: JTAGClientServer | null = null;

  /**
   * TEST 1: Storage Configuration Defaults Validation
   */
  async testStorageConfigurationDefaults(): Promise<StorageConfigTestResult> {
    const testName = 'Storage Configuration Defaults Validation';
    const startTime = Date.now();
    
    try {
      // Test that defaults are properly defined next to types (Rust-like convention)
      assert(DEFAULT_STORAGE_CONFIG.strategy === 'file', 'Default storage strategy is file');
      assert(DEFAULT_STORAGE_CONFIG.backend === 'file', 'Default storage backend is file');
      assert(DEFAULT_STORAGE_CONFIG.paths.data === '.continuum/jtag/data', 'Default data path is correct');
      assert(DEFAULT_STORAGE_CONFIG.paths.backups === '.continuum/jtag/backups', 'Default backup path is correct');
      assert(DEFAULT_STORAGE_CONFIG.features?.enableCaching === true, 'Default enables caching');
      assert(DEFAULT_STORAGE_CONFIG.features?.enableTransactions === false, 'Default disables transactions');
      
      console.log('   ✅ All storage configuration defaults are correct');
      
      return {
        testName,
        success: true,
        duration: Date.now() - startTime
      };
      
    } catch (error) {
      return {
        testName,
        success: false,
        duration: Date.now() - startTime,
        error: (error as Error).message
      };
    }
  }

  /**
   * TEST 2: Real Storage Configuration via JTAG Commands
   */
  async testRealStorageConfiguration(): Promise<StorageConfigTestResult> {
    const testName = 'Real Storage Configuration via JTAG Commands';
    const startTime = Date.now();
    
    try {
      // Connect to JTAG system
      this.client = await jtag.connect();
      if (!this.client) {
        throw new Error('Failed to connect to JTAG system for storage configuration test');
      }
      
      console.log('⚡ Testing real storage configuration via data/create command...');
      
      const testData = {
        message: 'Real storage config test',
        timestamp: new Date().toISOString(),
        strategy: 'file',
        configuredProperly: true
      };
      
      // Use actual JTAG command - this tests the real storage configuration
      const createResult = await this.client.commands['data/create']({
        collection: 'storage_config_integration',
        data: testData
      });
      
      assert(createResult.success === true, 'Real storage create succeeded');
      assert(createResult.id !== undefined, 'Real storage create returned valid ID');
      
      console.log('⚡ Testing real storage configuration via data/list command...');
      
      const listResult = await this.client.commands['data/list']({
        collection: 'storage_config_integration'
      });
      
      assert(listResult.success === true, 'Real storage list succeeded');
      assert(listResult.items && listResult.items.length > 0, 'Real storage list returned data');
      
      return {
        testName,
        success: true,
        duration: Date.now() - startTime
      };
      
    } catch (error) {
      return {
        testName,
        success: false,
        duration: Date.now() - startTime,
        error: (error as Error).message
      };
    }
  }

  /**
   * TEST 3: System Storage Configuration Integration
   */
  async testSystemStorageConfigurationIntegration(): Promise<StorageConfigTestResult> {
    const testName = 'System Storage Configuration Integration';
    const startTime = Date.now();
    
    try {
      // Connect to JTAG system
      this.client = await jtag.connect();
      if (!this.client) {
        throw new Error('Failed to connect to JTAG system for storage configuration test');
      }
      
      // Test that storage configuration is properly loaded in system context
      console.log('⚡ Testing system configuration access via ping command...');
      
      const pingResult = await this.client.commands.ping({});
      
      assert(pingResult.success === true, 'Ping succeeded');
      
      // Type-safe access to config
      const context = pingResult.context as { config?: { server?: { storage?: typeof DEFAULT_STORAGE_CONFIG } } };
      const storageConfig = context?.config?.server?.storage;
      
      assert(storageConfig !== undefined, 'Storage config is available in system context');
      
      if (storageConfig) {
        // Verify our configuration defaults are loaded
        assert(storageConfig.strategy === 'file', 'System uses file storage strategy');
        assert(storageConfig.backend === 'file', 'System uses file storage backend');
        assert(storageConfig.paths.data === '.continuum/jtag/data', 'System uses correct data path');
        assert(storageConfig.features?.enableCaching === true, 'System has caching enabled');
        assert(storageConfig.features?.enableTransactions === false, 'System has transactions disabled');
      }
      
      console.log('   ✅ Storage configuration properly integrated into system context');
      
      return {
        testName,
        success: true,
        duration: Date.now() - startTime
      };
      
    } catch (error) {
      return {
        testName,
        success: false,
        duration: Date.now() - startTime,
        error: (error as Error).message
      };
    }
  }

  /**
   * Run all storage configuration tests
   */
  async runAllTests(): Promise<void> {
    console.log('🚀 Starting Storage Configuration Integration Tests\n');
    
    try {
      // Run all tests
      this.results.push(await this.testStorageConfigurationDefaults());
      this.results.push(await this.testRealStorageConfiguration()); 
      this.results.push(await this.testSystemStorageConfigurationIntegration());
      
      // Analyze results
      const passed = this.results.filter(r => r.success).length;
      const failed = this.results.filter(r => !r.success).length;
      const totalTime = this.results.reduce((sum, r) => sum + r.duration, 0);
      
      console.log('\n📊 Storage Configuration Test Results:');
      console.log(`   Total Tests: ${this.results.length}`);
      console.log(`   ✅ Passed: ${passed}`);
      console.log(`   ❌ Failed: ${failed}`);
      console.log(`   ⏱️ Total Time: ${totalTime}ms`);
      
      if (failed > 0) {
        console.log('\n❌ Failed Tests:');
        this.results.filter(r => !r.success).forEach(r => {
          console.log(`   - ${r.testName}: ${r.error}`);
        });
        process.exit(1);
      }
      
      console.log('\n🎉 ALL STORAGE CONFIGURATION TESTS PASSED!');
      console.log('📋 Validated:');
      console.log('  ✅ Rust-like configuration defaults (types + defaults together)');
      console.log('  ✅ Real JTAG command execution (data/create, data/list)');
      console.log('  ✅ System-wide storage configuration integration');
      console.log('  ✅ Professional storage abstraction architecture');
      
    } catch (error) {
      console.error('\n❌ Storage configuration tests failed:', (error as Error).message);
      process.exit(1);
    }
  }
}

// Export for test runner
export async function runAllStorageConfigurationTests(): Promise<void> {
  const validator = new StorageConfigurationValidator();
  await validator.runAllTests();
}

// Run if called directly
if (require.main === module) {
  const validator = new StorageConfigurationValidator();
  validator.runAllTests();
}

/**
 * TEST CLASSIFICATION:
 * - Category: database 
 * - Type: integration
 * - Uses reusable commands: data/create, data/list, ping
 * - Tests real system configuration integration
 * - Validates Rust-like configuration architecture
 * - Part of npm run test:database
 */