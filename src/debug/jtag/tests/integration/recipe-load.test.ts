#!/usr/bin/env npx tsx
/**
 * Recipe Load Test
 *
 * Tests that recipe/load command works and properly loads general-chat.json
 */

import { jtag } from '../../server-index';
import type { RecipeLoadResult } from '../../commands/recipe/load/shared/RecipeLoadTypes';
import type { CommandSuccessResponse } from '../../daemons/command-daemon/shared/CommandResponseTypes';
import { RecipeEntity } from '../../system/data/entities/RecipeEntity';
import type { DataListResult } from '../../commands/data/list/shared/DataListTypes';

async function testRecipeLoad(): Promise<void> {
  console.log('🧪 RECIPE LOAD TEST');
  console.log('==================');

  let client = null;

  try {
    // Connect to JTAG system
    console.log('🔗 Connecting to JTAG system...');
    client = await jtag.connect();
    console.log('✅ Connected');

    // Test 1: Load general-chat recipe
    console.log('\n📋 Test 1: Loading general-chat recipe...');
    const loadResponse: CommandSuccessResponse = await client.commands['recipe/load']({
      context: 'recipe-test',
      sessionId: `recipe-test-${Date.now()}`,
      recipeId: 'general-chat',
      reload: true  // Force reload to test even if already exists
    });

    if (!loadResponse.success) {
      console.error('❌ recipe/load failed:', loadResponse);
      throw new Error(`Recipe load failed: ${JSON.stringify(loadResponse)}`);
    }

    const loadResult = loadResponse.commandResult as RecipeLoadResult;
    console.log(`✅ Recipe load result:`, {
      success: loadResult.success,
      loaded: loadResult.loaded?.length,
      errors: loadResult.errors
    });

    if (!loadResult.success || !loadResult.loaded?.length) {
      throw new Error('Recipe load did not succeed or no recipes loaded');
    }

    const loadedRecipe = loadResult.loaded[0];
    console.log(`✅ Loaded recipe:`, {
      uniqueId: loadedRecipe.uniqueId,
      name: loadedRecipe.name,
      displayName: loadedRecipe.displayName,
      pipelineSteps: loadedRecipe.pipeline?.length
    });

    // Test 2: Verify recipe is in database
    console.log('\n📋 Test 2: Verifying recipe in database...');
    const listResponse: CommandSuccessResponse = await client.commands['data/list']({
      context: 'recipe-test',
      sessionId: `recipe-test-${Date.now()}`,
      collection: RecipeEntity.collectionName,
      filter: { uniqueId: 'general-chat' }
    });

    if (!listResponse.success) {
      throw new Error('Failed to query recipe from database');
    }

    const listResult = listResponse.commandResult as DataListResult<RecipeEntity>;
    if (!listResult.items?.length) {
      throw new Error('Recipe not found in database after load');
    }

    const dbRecipe = listResult.items[0];
    console.log(`✅ Found recipe in database:`, {
      id: dbRecipe.id,
      uniqueId: dbRecipe.uniqueId,
      name: dbRecipe.name,
      displayName: dbRecipe.displayName
    });

    // Test 3: Validate recipe entity structure
    console.log('\n📋 Test 3: Validating recipe entity...');
    const validation = dbRecipe.validate();
    if (!validation.success) {
      throw new Error(`Recipe validation failed: ${validation.error}`);
    }
    console.log('✅ Recipe entity validation passed');

    // Test 4: Verify uniqueId is present (critical requirement)
    console.log('\n📋 Test 4: Verifying uniqueId requirement...');
    if (!dbRecipe.uniqueId) {
      throw new Error('Recipe uniqueId is missing - required field not enforced');
    }
    console.log(`✅ Recipe uniqueId confirmed: ${dbRecipe.uniqueId}`);

    console.log('\n🎉 ALL RECIPE LOAD TESTS PASSED');
    console.log('✅ recipe/load command works correctly');
    console.log('✅ Recipe loaded from JSON file');
    console.log('✅ Recipe persisted to database');
    console.log('✅ Recipe entity validation passed');
    console.log('✅ uniqueId requirement enforced');

  } catch (error) {
    console.error('❌ Recipe load test failed:', error);
    throw error;
  } finally {
    if (client) {
      try {
        await client.disconnect();
      } catch (disconnectError) {
        console.error('Disconnect error:', disconnectError);
      }
    }
  }
}

// Run test and exit
testRecipeLoad().then(() => {
  console.log('✅ Recipe load test completed successfully');
  process.exit(0);
}).catch(error => {
  console.error('🚨 Recipe load test failed:', error);
  process.exit(1);
});
