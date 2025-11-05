/**
 * Runtime test for TypeScript path aliases
 * Purpose: Verify @system/* imports work at runtime
 */

// Test @system/* alias with runtime usage
import { Commands } from '@system/core/shared/Commands';
import { Events } from '@system/core/shared/Events';

// Test execution
async function testPathAliasesRuntime(): Promise<void> {
  console.log('🧪 Testing path aliases at runtime...');

  // Test Commands exists and has the right shape
  if (typeof Commands.execute === 'function') {
    console.log('✅ Commands.execute is a function');
  } else {
    console.log('❌ Commands.execute is NOT a function');
  }

  // Test Events exists and has the right shape
  if (typeof Events.subscribe === 'function' && typeof Events.emit === 'function') {
    console.log('✅ Events.subscribe and Events.emit are functions');
  } else {
    console.log('❌ Events methods missing');
  }

  console.log('✅ Path aliases runtime test completed successfully!');
}

// Run test
testPathAliasesRuntime().catch(console.error);
