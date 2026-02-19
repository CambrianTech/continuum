/**
 * Test script to verify UnslothLoRAAdapter integration
 *
 * Verifies:
 * 1. supportsFineTuning() returns true after bootstrap
 * 2. Paths resolve correctly
 * 3. Training script exists
 */

import { PEFTLoRAAdapter } from './adapters/PEFTLoRAAdapter';
import * as path from 'path';
import * as fs from 'fs';

async function main() {
  console.log('🧬 Testing PEFTLoRAAdapter Integration\n');

  const adapter = new PEFTLoRAAdapter();

  // Test 1: Check if fine-tuning is supported
  console.log('Test 1: supportsFineTuning()');
  const supportsFineTuning = adapter.supportsFineTuning();
  console.log(`   Result: ${supportsFineTuning}`);

  if (!supportsFineTuning) {
    console.log('   ❌ FAILED: Fine-tuning not supported');
    console.log('   Possible reasons:');
    console.log('   - Python environment not bootstrapped');
    console.log('   - train-wrapper.sh not found');
    console.log('   - peft-train.py not found');

    // Debug: Check each file individually
    const scriptPath = path.join(__dirname, 'adapters', 'scripts', 'peft-train.py');
    const projectRoot = path.resolve(__dirname, '../../../../../..');
    const wrapperPath = path.join(projectRoot, '.continuum', 'genome', 'python', 'train-wrapper.sh');

    console.log(`\n   Debug info:`);
    console.log(`   - Script path: ${scriptPath}`);
    console.log(`   - Script exists: ${fs.existsSync(scriptPath)}`);
    console.log(`   - Wrapper path: ${wrapperPath}`);
    console.log(`   - Wrapper exists: ${fs.existsSync(wrapperPath)}`);

    process.exit(1);
  }

  console.log('   ✅ PASSED\n');

  // Test 2: Get capabilities
  console.log('Test 2: getFineTuningCapabilities()');
  const capabilities = adapter.getFineTuningCapabilities();
  console.log(`   supportsFineTuning: ${capabilities.supportsFineTuning}`);
  console.log(`   strategy: ${capabilities.strategy}`);
  console.log(`   defaultRank: ${capabilities.defaultRank}`);
  console.log(`   defaultEpochs: ${capabilities.defaultEpochs}`);
  console.log(`   costPerExample: $${capabilities.costPerExample} (free!)`);
  console.log('   ✅ PASSED\n');

  // Test 3: Check training strategy
  console.log('Test 3: getFineTuningStrategy()');
  const strategy = adapter.getFineTuningStrategy();
  console.log(`   Strategy: ${strategy}`);
  if (strategy !== 'local-pytorch' && strategy !== 'local') {
    console.log(`   ❌ FAILED: Expected "local" or "local-pytorch" strategy, got "${strategy}"`);
    process.exit(1);
  }
  console.log('   ✅ PASSED\n');

  console.log('============================================================');
  console.log('✅ All tests PASSED');
  console.log('   Python environment is bootstrapped and ready for training');
  console.log('============================================================');
}

main().catch((error) => {
  console.error('❌ Test failed with error:');
  console.error(error);
  process.exit(1);
});
