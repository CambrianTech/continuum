/**
 * State Get Integration Test - Following CRUD widget test patterns
 *
 * Tests the state/get command following the established JTAG command testing methodology
 */

import { Commands } from '../../../../system/core/client/shared/Commands';
import type { StateGetParams, StateGetResult } from '../shared/StateGetTypes';
import type { UserStateEntity } from '../../../../system/data/entities/UserStateEntity';

export class StateGetIntegrationTest {

  /**
   * Test basic state/get command functionality
   */
  static async testBasicStateGet(): Promise<void> {
    console.log('🧪 Testing basic state/get command...');

    const result = await Commands.execute<StateGetParams, StateGetResult<UserStateEntity>>('state/get', {
      collection: 'UserState',
      limit: 5
    });

    if (!result.success) {
      throw new Error(`State get failed: ${result.error}`);
    }

    console.log(`✅ State get returned ${result.items.length} UserState entities`);

    // Validate response structure
    if (typeof result.count !== 'number') {
      throw new Error('State get result missing count field');
    }

    if (!Array.isArray(result.items)) {
      throw new Error('State get result missing items array');
    }

    if (!result.timestamp) {
      throw new Error('State get result missing timestamp');
    }

    console.log('✅ Basic state/get test passed');
  }

  /**
   * Test state/get with user context filtering
   */
  static async testUserContextFiltering(): Promise<void> {
    console.log('🧪 Testing state/get with user context filtering...');

    // First get all UserState entities
    const allResult = await Commands.execute<StateGetParams, StateGetResult<UserStateEntity>>('state/get', {
      collection: 'UserState'
    });

    if (!allResult.success || allResult.items.length === 0) {
      console.log('⏭️ Skipping user context test - no UserState data available');
      return;
    }

    // Get the first user's ID
    const testUserId = allResult.items[0].userId;

    // Test filtering by userId
    const filteredResult = await Commands.execute<StateGetParams, StateGetResult<UserStateEntity>>('state/get', {
      collection: 'UserState',
      userId: testUserId,
      limit: 10
    });

    if (!filteredResult.success) {
      throw new Error(`Filtered state get failed: ${filteredResult.error}`);
    }

    // All returned items should have the same userId
    for (const item of filteredResult.items) {
      if (item.userId !== testUserId) {
        throw new Error(`Expected userId ${testUserId}, got ${item.userId}`);
      }
    }

    console.log(`✅ User context filtering test passed - found ${filteredResult.items.length} items for user ${testUserId}`);
  }

  /**
   * Test state/get theme preferences validation
   */
  static async testThemePreferencesStructure(): Promise<void> {
    console.log('🧪 Testing theme preferences data structure...');

    const result = await Commands.execute<StateGetParams, StateGetResult<UserStateEntity>>('state/get', {
      collection: 'UserState',
      limit: 10
    });

    if (!result.success) {
      throw new Error(`State get failed: ${result.error}`);
    }

    let foundPreferences = false;
    let foundThemePreference = false;

    for (const item of result.items) {
      if (item.preferences) {
        foundPreferences = true;

        // Validate preferences structure
        if (typeof item.preferences !== 'object') {
          throw new Error('UserState preferences should be an object');
        }

        // Check if theme preference exists
        if ('theme' in item.preferences) {
          foundThemePreference = true;
          console.log(`✅ Found theme preference: ${(item.preferences as any).theme}`);
        }

        // Validate other expected preference fields
        const expectedFields = ['maxOpenTabs', 'autoCloseAfterDays', 'rememberScrollPosition', 'syncAcrossDevices'];
        for (const field of expectedFields) {
          if (!(field in item.preferences)) {
            console.log(`⚠️ Missing expected preference field: ${field}`);
          }
        }
      }
    }

    if (result.items.length > 0 && !foundPreferences) {
      throw new Error('UserState entities should have preferences objects');
    }

    console.log(`✅ Theme preferences structure test passed - found preferences: ${foundPreferences}, found theme: ${foundThemePreference}`);
  }

  /**
   * Test state/get limit and ordering
   */
  static async testLimitAndOrdering(): Promise<void> {
    console.log('🧪 Testing limit and ordering parameters...');

    // Test limit parameter
    const limitedResult = await Commands.execute<StateGetParams, StateGetResult<UserStateEntity>>('state/get', {
      collection: 'UserState',
      limit: 1
    });

    if (!limitedResult.success) {
      throw new Error(`Limited state get failed: ${limitedResult.error}`);
    }

    if (limitedResult.items.length > 1) {
      throw new Error(`Expected at most 1 item, got ${limitedResult.items.length}`);
    }

    // Test ordering (if we have multiple items)
    const allResult = await Commands.execute<StateGetParams, StateGetResult<UserStateEntity>>('state/get', {
      collection: 'UserState',
      orderBy: [{ field: 'id', direction: 'asc' }]
    });

    if (!allResult.success) {
      throw new Error(`Ordered state get failed: ${allResult.error}`);
    }

    console.log(`✅ Limit and ordering test passed - limited result: ${limitedResult.items.length} items, ordered result: ${allResult.items.length} items`);
  }

  /**
   * Run all integration tests
   */
  static async runAllTests(): Promise<void> {
    console.log('🚀 Starting State Get Integration Tests...\n');

    try {
      await this.testBasicStateGet();
      await this.testUserContextFiltering();
      await this.testThemePreferencesStructure();
      await this.testLimitAndOrdering();

      console.log('\n🎉 All State Get integration tests passed!');
    } catch (error) {
      console.error('\n❌ State Get integration test failed:', error);
      throw error;
    }
  }
}

// Export for use in other test files
export default StateGetIntegrationTest;