#!/usr/bin/env tsx
/**
 * Test Recipe Load Command
 *
 * Direct test without going through CLI/session infrastructure
 */

import { Commands } from './system/core/shared/Commands';
import type { RecipeLoadParams, RecipeLoadResult } from './commands/recipe/load/shared/RecipeLoadTypes';
import { randomUUID } from 'crypto';

async function testRecipeLoad() {
  console.log('🧪 Testing Recipe Load Command...\n');

  try {
    // Create params with dummy context/sessionId
    const params: RecipeLoadParams = {
      context: 'cli' as any,
      sessionId: randomUUID(),
      recipeId: 'general-chat'
    };

    console.log('📤 Calling Commands.execute("recipe/load", params)...');
    const result = await Commands.execute('recipe/load', params) as RecipeLoadResult;

    console.log('\n📥 Result:');
    console.log(JSON.stringify(result, null, 2));

    if (result.success) {
      console.log(`\n✅ SUCCESS: Loaded ${result.loaded.length} recipe(s)`);
      if (result.loaded.length > 0) {
        console.log(`\n📋 Recipe Details:`);
        console.log(`   Name: ${result.loaded[0].name}`);
        console.log(`   Display Name: ${result.loaded[0].displayName}`);
        console.log(`   Description: ${result.loaded[0].description}`);
        console.log(`   Pipeline Steps: ${result.loaded[0].pipeline.length}`);
      }
    } else {
      console.log(`\n❌ FAILED: ${result.errors?.[0]?.error || 'Unknown error'}`);
    }

  } catch (error) {
    console.error('\n❌ Test failed with exception:', error);
    if (error instanceof Error) {
      console.error('Stack:', error.stack);
    }
    process.exit(1);
  }
}

testRecipeLoad();
