/**
 * Data Marshal Command Unit Tests
 * 
 * Tests the universal data marshalling functionality for command chaining
 */

import assert from 'assert';
import { DataMarshalCommand } from '../../DataMarshalCommand';

async function runTests() {
  console.log('🔬 Testing Data Marshal Command functionality...');

  // Test 1: Basic command definition
  console.log('📋 Testing command definition...');
  const definition = DataMarshalCommand.getDefinition();
  assert.ok(definition.name === 'data-marshal', 'Command name should be data-marshal');
  assert.ok(definition.description.includes('marshalling'), 'Description should mention marshalling');
  assert.ok(definition.category === 'data-marshal', 'Category should be data-marshal');
  console.log('✅ Command definition valid');

  // Test 2: Encode operation
  console.log('🔢 Testing encode operation...');
  const encodeResult = await DataMarshalCommand.execute({
    operation: 'encode',
    data: { test: 'data', number: 42 },
    encoding: 'json',
    source: 'test'
  });
  assert.ok(encodeResult.success, 'Encode should succeed');
  assert.ok(encodeResult.data?.marshalled, 'Should have marshalled data');
  assert.ok(encodeResult.data.marshalled.encoding === 'json', 'Should use JSON encoding');
  console.log('✅ Encode operation working');

  // Test 3: Base64 encoding for binary data simulation
  console.log('🔐 Testing base64 encoding...');
  const base64Result = await DataMarshalCommand.execute({
    operation: 'encode',
    data: 'test binary data',
    encoding: 'base64',
    source: 'screenshot'
  });
  assert.ok(base64Result.success, 'Base64 encode should succeed');
  assert.ok(base64Result.data?.marshalled.encoding === 'base64', 'Should use base64 encoding');
  console.log('✅ Base64 encoding working');

  // Test 4: Decode operation
  console.log('🔓 Testing decode operation...');
  const decodeResult = await DataMarshalCommand.execute({
    operation: 'decode',
    data: encodeResult.data?.marshalled
  });
  assert.ok(decodeResult.success, 'Decode should succeed');
  assert.ok(decodeResult.data?.decoded, 'Should have decoded data');
  console.log('✅ Decode operation working');

  // Test 5: Chain operation for command composition
  console.log('🔗 Testing chain operation...');
  const chainResult = await DataMarshalCommand.execute({
    operation: 'chain',
    data: { widgets: [], screenshot: 'pending' },
    correlationId: 'test-chain-123',
    source: 'screenshot',
    destination: 'widget-inspect'
  });
  assert.ok(chainResult.success, 'Chain should succeed');
  assert.ok(chainResult.data?.chainable, 'Should have chainable data');
  assert.ok(chainResult.data.chainId === 'test-chain-123', 'Should preserve correlation ID');
  console.log('✅ Chain operation working');

  // Test 6: Extract operation for data access
  console.log('📤 Testing extract operation...');
  const extractResult = await DataMarshalCommand.execute({
    operation: 'extract',
    data: { widgets: [{ name: 'sidebar' }, { name: 'chat' }] },
    metadata: { path: 'widgets[0].name' }
  });
  assert.ok(extractResult.success, 'Extract should succeed');
  assert.ok(extractResult.data?.extracted === 'sidebar', 'Should extract correct value');
  console.log('✅ Extract operation working');

  // Test 7: Parameter parsing for command-line usage
  console.log('⚙️ Testing parameter parsing...');
  const parseResult = await DataMarshalCommand.execute({
    args: ['--operation=encode', '--data={"test":true}', '--encoding=json']
  });
  assert.ok(parseResult.success, 'Parameter parsing should succeed');
  console.log('✅ Parameter parsing working');

  console.log('🎉 All Data Marshal Command tests passed!');
  console.log('📊 Ready for screenshot integration testing');
}

runTests().catch(console.error);