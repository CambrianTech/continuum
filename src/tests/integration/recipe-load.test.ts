#!/usr/bin/env npx tsx
/**
 * Recipe Load Test
 *
 * Tests that workspace/recipe/load command works and properly loads general-chat.json
 */

import { jtag } from '../../server-index';
import type { RecipeLoadResult } from '../../commands/workspace/recipe/load/shared/RecipeLoadTypes';
import { RecipeEntity } from '../../system/data/entities/RecipeEntity';
import type { JTAGPayload } from '../../system/core/types/JTAGTypes';

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
    const loadResult = await client.commands['workspace/recipe/load']({
      context: 'recipe-test',
      sessionId: `recipe-test-${Date.now()}`,
      recipeId: 'general-chat',
      reload: true  // Force reload to test even if already exists
    }) as JTAGPayload & RecipeLoadResult;

    if (!loadResult.success) {
      console.error('❌ recipe/load failed:', loadResult);
      throw new Error(`Recipe load failed: ${JSON.stringify(loadResult)}`);
    }

    console.log(`✅ Recipe load result:`, {
      success: loadResult.success,
      loaded: (loadResult as any).loaded?.length,
      errors: (loadResult as any).errors
    });

    const loaded = (loadResult as any).loaded as RecipeLoadResult['loaded'];
    if (!loaded?.length) {
      throw new Error('Recipe load did not succeed or no recipes loaded');
    }

    const loadedRecipe = loaded[0];
    console.log(`✅ Loaded recipe:`, {
      uniqueId: loadedRecipe.uniqueId,
      name: loadedRecipe.name,
      displayName: loadedRecipe.displayName,
      pipelineSteps: loadedRecipe.pipeline?.length
    });

    // Test 2: Verify recipe is in database
    console.log('\n📋 Test 2: Verifying recipe in database...');
    const listResult = await client.commands['data/list']({
      context: 'recipe-test',
      sessionId: `recipe-test-${Date.now()}`,
      collection: RecipeEntity.collection,
      filter: { uniqueId: 'general-chat' }
    }) as JTAGPayload;

    if (!listResult.success) {
      throw new Error('Failed to query recipe from database');
    }

    const items = (listResult as any).items as RecipeEntity[];
    if (!items?.length) {
      throw new Error('Recipe not found in database after load');
    }

    const dbRecipe = items[0];
    console.log(`✅ Found recipe in database:`, {
      id: dbRecipe.id,
      uniqueId: dbRecipe.uniqueId,
      name: dbRecipe.name,
      displayName: dbRecipe.displayName
    });

    // Test 3: Validate recipe entity structure
    console.log('\n📋 Test 3: Validating recipe entity...');
    // dbRecipe is a plain object from the wire — validate key fields exist
    if (!dbRecipe.id || !dbRecipe.name) {
      throw new Error(`Recipe validation failed: missing id or name`);
    }
    console.log('✅ Recipe entity validation passed');

    // Test 4: Verify uniqueId is present (critical requirement)
    console.log('\n📋 Test 4: Verifying uniqueId requirement...');
    if (!dbRecipe.uniqueId) {
      throw new Error('Recipe uniqueId is missing - required field not enforced');
    }
    console.log(`✅ Recipe uniqueId confirmed: ${dbRecipe.uniqueId}`);

    console.log('\n🎉 ALL RECIPE LOAD TESTS PASSED');
    console.log('✅ workspace/recipe/load command works correctly');
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
