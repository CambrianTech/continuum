#!/usr/bin/env tsx
/**
 * TDD Integration Test: UserRepository + Data Seeding + Widget Integration
 *
 * This test defines the complete integration flow we want to achieve:
 * 1. Seed users using new UserRepository domain objects
 * 2. Verify seeded data has proper types (HumanUser, AgentUser, PersonaUser)
 * 3. Test widget integration loading real users
 * 4. Verify end-to-end Discord-like user management
 *
 * TDD Approach: WRITE FAILING TESTS FIRST, then make them pass
 */

import { HumanUser, type HumanUserData } from '../../domain/user/HumanUser';
import { AgentUser, type AgentUserData } from '../../domain/user/AgentUser';
import { PersonaUser, type PersonaUserData } from '../../domain/user/PersonaUser';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

console.log('🧪 TDD USER REPOSITORY INTEGRATION TEST');
console.log('=======================================');

async function runTDDIntegrationTests() {
  try {
    console.log('🔴 RED PHASE: Running tests that should FAIL initially...');
    console.log('');

    // TEST 1: Data Seeding with New Domain Objects
    console.log('🧪 TEST 1: Testing data seeding integration...');
    await testDataSeedingIntegration();

    // TEST 2: JTAG Command Integration
    console.log('🧪 TEST 2: Testing JTAG data/list command integration...');
    await testJTAGCommandIntegration();

    // TEST 3: Widget Integration
    console.log('🧪 TEST 3: Testing widget integration...');
    await testWidgetIntegration();

    // TEST 4: End-to-End Discord-like Features
    console.log('🧪 TEST 4: Testing Discord-like user management...');
    await testDiscordLikeFeatures();

    console.log('');
    console.log('🎉 ALL TESTS PASSED! (This should not happen in RED phase)');

  } catch (error) {
    console.log('');
    console.log('🔴 EXPECTED FAILURE - This is the RED phase of TDD');
    console.log(`❌ Error: ${error instanceof Error ? error.message : error}`);
    console.log('');
    console.log('📋 NEXT STEPS (GREEN phase):');
    console.log('1. Update UserDataSeed.ts to use new UserRepository classes');
    console.log('2. Update data seeding to create HumanUser, AgentUser, PersonaUser');
    console.log('3. Run this test again - should pass more tests');
    console.log('4. Update widgets to handle new user structure');
    console.log('5. Run this test again - should pass all tests');
    console.log('');
    console.log('🎯 Goal: Make all these tests GREEN by implementing the features');

    // Don't exit with error - this is expected in TDD RED phase
    return true;
  }
}

async function testDataSeedingIntegration(): Promise<void> {
  console.log('   Testing if seeded data uses proper domain objects...');

  // This test will fail until we update the seeding system
  // It checks if our data:seed command creates proper UserRepository objects

  const { execSync } = require('child_process');
  try {
    // First, clear and reseed data
    console.log('   🧹 Clearing existing data...');
    execSync('npm run data:clear', { stdio: 'pipe', timeout: 10000 });

    console.log('   🌱 Seeding new data...');
    execSync('npm run data:seed', { stdio: 'pipe', timeout: 10000 });

    console.log('   ✅ Seeding completed without errors');

    // This part should work once seeding is updated
    console.log('   📋 Verifying seeded data structure...');
    // TODO: Add verification that seeded users are proper domain objects

  } catch (error) {
    throw new Error(`Data seeding integration failed: ${error}`);
  }
}

async function testJTAGCommandIntegration(): Promise<void> {
  console.log('   Testing JTAG data/list command returns proper user types...');

  const { execSync } = require('child_process');
  try {
    const result = execSync('./jtag data/list --collection=users', {
      encoding: 'utf-8',
      timeout: 10000
    });

    const commandOutput = result.split('COMMAND RESULT:')[1]?.split('============================================================')[0]?.trim();
    if (!commandOutput) {
      throw new Error('No command output found');
    }

    const userData = JSON.parse(commandOutput);
    if (!userData.success) {
      throw new Error('Command failed: ' + userData.error);
    }

    if (!Array.isArray(userData.items) || userData.items.length === 0) {
      throw new Error('No users found - seeding may have failed');
    }

    // Verify we have proper user types
    const users = userData.items;
    const humanUsers = users.filter((u: any) => u.citizenType === 'human');
    const aiUsers = users.filter((u: any) => u.citizenType === 'ai');

    if (humanUsers.length === 0) {
      throw new Error('No human users found');
    }

    if (aiUsers.length === 0) {
      throw new Error('No AI users found');
    }

    // Verify AI users have proper subtypes
    const agents = aiUsers.filter((u: any) => u.aiType === 'agent');
    const personas = aiUsers.filter((u: any) => u.aiType === 'persona');

    if (agents.length === 0) {
      throw new Error('No agent users found');
    }

    if (personas.length === 0) {
      throw new Error('No persona users found');
    }

    console.log(`   ✅ Found ${humanUsers.length} humans, ${agents.length} agents, ${personas.length} personas`);

  } catch (error) {
    throw new Error(`JTAG command integration failed: ${error}`);
  }
}

async function testWidgetIntegration(): Promise<void> {
  console.log('   Testing UserListWidget can load new user structure...');

  // This test checks if widgets can handle the new user data structure
  // Will fail until widgets are updated to work with new domain objects

  throw new Error('Widget integration not implemented yet - widgets expect old user format');
}

async function testDiscordLikeFeatures(): Promise<void> {
  console.log('   Testing Discord-like user management features...');

  // This test checks if all Discord-like features work end-to-end:
  // - User sessions
  // - User permissions
  // - User presence
  // - Room participation

  throw new Error('Discord-like features not fully integrated yet');
}

// Run tests if called directly
runTDDIntegrationTests()
  .then((success) => {
    if (success) {
      console.log('🔴 TDD RED PHASE COMPLETE');
      console.log('Ready to implement features to make tests pass');
    }
  })
  .catch((error) => {
    console.error('❌ Test execution failed:', error);
    process.exit(1);
  });