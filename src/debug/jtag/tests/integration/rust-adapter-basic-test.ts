/**
 * Basic RustAdapter Test - No System Required
 *
 * Tests JUST the adapter instantiation and communication
 * BEFORE integrating with the full system
 */

import { RustAdapter } from '../../daemons/data-daemon/server/RustAdapter';

async function main() {
  console.log('🧪 Testing RustAdapter (no system required)\n');

  // Test 1: Can we instantiate the adapter?
  console.log('1️⃣  Creating RustAdapter instance...');
  const adapter = new RustAdapter();
  console.log('   ✅ Adapter created\n');

  // Test 2: Can it initialize and connect to Rust worker?
  console.log('2️⃣  Initializing adapter (connecting to Rust worker)...');
  try {
    await adapter.initialize({
      type: 'rust',
      namespace: 'test-basic',
      options: {
        filename: '/tmp/test-basic.sqlite',
        storageType: 'auto-detect'
      }
    });
    console.log('   ✅ Adapter initialized and connected\n');
  } catch (error) {
    console.error('   ❌ Initialization failed:', error);
    console.log('\n💡 Make sure Rust worker is running:');
    console.log('   cd workers/data-daemon');
    console.log('   cargo run --bin data-worker-test\n');
    process.exit(1);
  }

  // Test 3: Can it execute a simple operation?
  console.log('3️⃣  Testing listCollections (should reach Rust worker)...');
  try {
    const result = await adapter.listCollections();
    if (result.success) {
      console.log('   ✅ Operation succeeded:', result.data);
    } else {
      console.log('   ⚠️  Operation returned error (expected if not implemented):', result.error);
    }
  } catch (error) {
    console.error('   ❌ Operation failed:', error);
  }

  console.log('\n4️⃣  Closing adapter...');
  await adapter.close();
  console.log('   ✅ Adapter closed\n');

  console.log('✅ BASIC TEST COMPLETE\n');
  console.log('Next: Test in running system via data/open command');
}

main();
