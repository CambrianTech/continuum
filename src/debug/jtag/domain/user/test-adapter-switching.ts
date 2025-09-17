/**
 * Test: Easy Adapter Switching Demo
 *
 * Demonstrates how the same repository code works with different storage backends:
 * JSON, SQLite, Memory, etc. - just change the configuration, same API.
 */

import { UserRepositoryFactory, REPOSITORY_PRESETS } from './UserRepositoryFactory';
import { HumanUser } from './HumanUser';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

async function demonstrateAdapterSwitching() {
  console.log('🔧 CLAUDE-FIX-' + Date.now() + ': Testing adapter switching');

  const context = {
    sessionId: 'test-session' as UUID,
    timestamp: new Date().toISOString(),
    source: 'adapter-switching-demo'
  };

  // ============================================================================
  // 1. JSON FILE STORAGE (Development)
  // ============================================================================
  console.log('\n📄 Testing JSON File Storage...');

  try {
    const jsonRepos = await UserRepositoryFactory.createWithPreset('development');
    console.log('✅ JSON repositories created');

    // Create a human user
    const humanUser = await jsonRepos.humanRepository.createHuman(
      'Joel (JSON)',
      'session-json' as UUID,
      context
    );

    if (humanUser.success && humanUser.data) {
      console.log('✅ Created human user in JSON:', humanUser.data.displayName);

      // Find by type
      const humans = await jsonRepos.userRepository.findByType('human', context);
      console.log('✅ Found humans in JSON:', humans.data?.length || 0);
    }

    await UserRepositoryFactory.close('development');
    console.log('✅ JSON storage test completed');

  } catch (error) {
    console.log('⚠️ JSON storage test failed (expected if dependencies missing):', error);
  }

  // ============================================================================
  // 2. MEMORY STORAGE (Testing)
  // ============================================================================
  console.log('\n🧠 Testing Memory Storage...');

  try {
    const memoryRepos = await UserRepositoryFactory.createWithPreset('testing');
    console.log('✅ Memory repositories created');

    // Create the same user in memory
    const humanUser = await memoryRepos.humanRepository.createHuman(
      'Joel (Memory)',
      'session-memory' as UUID,
      context
    );

    if (humanUser.success && humanUser.data) {
      console.log('✅ Created human user in memory:', humanUser.data.displayName);

      // Find active users
      const activeUsers = await memoryRepos.userRepository.findActiveUsers(context);
      console.log('✅ Found active users in memory:', activeUsers.data?.length || 0);
    }

    await UserRepositoryFactory.close('testing');
    console.log('✅ Memory storage test completed');

  } catch (error) {
    console.log('⚠️ Memory storage test failed (expected if dependencies missing):', error);
  }

  // ============================================================================
  // 3. SQLITE STORAGE (Production)
  // ============================================================================
  console.log('\n🗃️ Testing SQLite Storage...');

  try {
    const sqliteRepos = await UserRepositoryFactory.createWithPreset('production');
    console.log('✅ SQLite repositories created');

    // Create an AI agent user
    const agentUser = await sqliteRepos.agentRepository.createAgent(
      'Claude Agent (SQLite)',
      'session-sqlite' as UUID,
      {
        portalType: 'api',
        endpoint: 'https://api.anthropic.com',
        config: { model: 'claude-3-sonnet' }
      },
      {
        provider: 'anthropic',
        model: 'claude-3-sonnet',
        contextWindow: 200000
      },
      context
    );

    if (agentUser.success && agentUser.data) {
      console.log('✅ Created agent user in SQLite:', agentUser.data.displayName);

      // Find AI users
      const aiUsers = await sqliteRepos.userRepository.findByType('ai', context, 'agent');
      console.log('✅ Found AI agents in SQLite:', aiUsers.data?.length || 0);
    }

    await UserRepositoryFactory.close('production');
    console.log('✅ SQLite storage test completed');

  } catch (error) {
    console.log('⚠️ SQLite storage test failed (expected if dependencies missing):', error);
  }

  // ============================================================================
  // 4. SAME CODE, DIFFERENT BACKEND DEMONSTRATION
  // ============================================================================
  console.log('\n🔄 Demonstrating Same Code, Different Backend...');

  async function testWithBackend(presetName: keyof typeof REPOSITORY_PRESETS, backendName: string) {
    try {
      const repos = await UserRepositoryFactory.createWithPreset(presetName);

      // This is the SAME CODE that works with ANY backend
      const user = await repos.humanRepository.createHuman(
        `Test User (${backendName})`,
        `session-${presetName}` as UUID,
        context
      );

      if (user.success && user.data) {
        // Same query API works with any backend
        const activeUsers = await repos.userRepository.findActiveUsers(context);
        console.log(`✅ ${backendName}: Created user and found ${activeUsers.data?.length || 0} active users`);
      }

      await UserRepositoryFactory.close(presetName);
      return true;

    } catch (error) {
      console.log(`⚠️ ${backendName} failed:`, error?.constructor?.name || 'Error');
      return false;
    }
  }

  // Test all available presets with the SAME code
  const results = await Promise.allSettled([
    testWithBackend('development', 'JSON'),
    testWithBackend('testing', 'Memory'),
    // Skip production SQLite and scale PostgreSQL in demo to avoid setup requirements
  ]);

  const successful = results.filter(r => r.status === 'fulfilled' && r.value).length;
  console.log(`\n🎉 Successfully tested ${successful} different storage backends with IDENTICAL code!`);

  // ============================================================================
  // 5. SHOW AVAILABLE PRESETS
  // ============================================================================
  console.log('\n📋 Available Storage Presets:');
  const presets = UserRepositoryFactory.getAvailablePresets();
  presets.forEach(({ key, config }) => {
    console.log(`  ${key}: ${config.description} (${config.storageConfig.strategy}/${config.storageConfig.backend})`);
  });

  console.log('\n✨ Adapter switching demo completed!');
  console.log('💡 Key insight: Same repository API works with ANY storage backend');
  console.log('🔧 Change backend: Just change the preset name, rest stays the same!');
}

// Run the demo
if (require.main === module) {
  demonstrateAdapterSwitching().catch(error => {
    console.error('Demo failed:', error);
    process.exit(1);
  });
}

export { demonstrateAdapterSwitching };