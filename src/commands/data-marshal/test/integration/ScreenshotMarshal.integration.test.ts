/**
 * Screenshot + Data Marshal Integration Test
 * 
 * Demonstrates how data marshalling enables screenshot workflows:
 * 1. Screenshot captures image as base64
 * 2. Data marshal encodes for safe transmission
 * 3. Data marshal chains to next command
 * 4. Data marshal extracts specific fields
 */

import assert from 'assert';
import { DataMarshalCommand } from '../../DataMarshalCommand';

async function testScreenshotMarshalWorkflow() {
  console.log('📸 Testing Screenshot + Data Marshal Integration...');

  // Simulate screenshot command output (what ScreenshotCommand.executeOnClient returns)
  const mockScreenshotResult = {
    success: true,
    data: {
      imageData: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==',
      selector: 'body',
      filename: 'screenshot-test.png',
      format: 'png',
      width: 1200,
      height: 800
    },
    clientMetadata: {
      userAgent: 'Test Browser',
      timestamp: Date.now(),
      executionTime: 245
    }
  };

  console.log('📋 Step 1: Encode screenshot data for transmission...');
  
  // Step 1: Marshal the screenshot result for transmission
  const marshalResult = await DataMarshalCommand.execute({
    operation: 'encode',
    data: mockScreenshotResult,
    encoding: 'json',
    source: 'screenshot',
    destination: 'analysis',
    correlationId: 'screenshot-workflow-123'
  });

  assert.ok(marshalResult.success, 'Screenshot marshalling should succeed');
  assert.ok(marshalResult.data?.marshalled, 'Should have marshalled data');
  assert.ok(marshalResult.data.marshalled.source === 'screenshot', 'Should preserve source');
  console.log(`✅ Screenshot marshalled: ${marshalResult.data.marshalId}`);

  console.log('🔗 Step 2: Create chainable workflow for command composition...');
  
  // Step 2: Create chainable data for multi-command workflow
  const chainResult = await DataMarshalCommand.execute({
    operation: 'chain',
    data: mockScreenshotResult,
    correlationId: 'screenshot-chain-456',
    source: 'screenshot',
    destination: 'widget-inspect'
  });

  assert.ok(chainResult.success, 'Screenshot chaining should succeed');
  assert.ok(chainResult.data?.chainable, 'Should have chainable data');
  assert.ok(chainResult.data.chainable.ready, 'Chain should be ready');
  console.log(`✅ Screenshot chain created: ${chainResult.data.chainId}`);

  console.log('📤 Step 3: Extract specific screenshot metadata...');
  
  // Step 3: Extract specific fields from screenshot data
  const extractResult = await DataMarshalCommand.execute({
    operation: 'extract',
    data: mockScreenshotResult,
    metadata: { path: 'data.imageData' }
  });

  assert.ok(extractResult.success, 'Screenshot extraction should succeed');
  assert.ok(extractResult.data?.extracted?.startsWith('data:image/png'), 'Should extract image data URL');
  console.log('✅ Screenshot image data extracted');

  console.log('🎯 Step 4: Extract screenshot dimensions for analysis...');
  
  // Step 4: Extract dimensions for AI analysis
  const dimensionsResult = await DataMarshalCommand.execute({
    operation: 'extract',
    data: mockScreenshotResult,
    metadata: { path: 'data.width' }
  });

  assert.ok(dimensionsResult.success, 'Dimension extraction should succeed');
  assert.ok(dimensionsResult.data?.extracted === 1200, 'Should extract correct width');
  console.log('✅ Screenshot dimensions extracted');

  console.log('🔐 Step 5: Base64 encode pure image data for WebSocket transmission...');
  
  // Step 5: Extract and re-encode just the base64 image data 
  const imageDataOnly = mockScreenshotResult.data.imageData.split(',')[1]; // Remove data:image/png;base64,
  const base64Result = await DataMarshalCommand.execute({
    operation: 'encode',
    data: imageDataOnly,
    encoding: 'base64',
    source: 'screenshot-pure',
    destination: 'ai-analysis',
    correlationId: 'image-only-789'
  });

  assert.ok(base64Result.success, 'Pure image encoding should succeed');
  assert.ok(base64Result.data?.marshalled.encoding === 'base64', 'Should use base64 encoding');
  console.log('✅ Pure image data encoded for transmission');

  console.log('🔄 Step 6: Round-trip decode to verify integrity...');
  
  // Step 6: Decode to verify data integrity
  const decodeResult = await DataMarshalCommand.execute({
    operation: 'decode',
    data: base64Result.data?.marshalled
  });

  assert.ok(decodeResult.success, 'Screenshot decode should succeed');
  assert.ok(decodeResult.data?.decoded === imageDataOnly, 'Decoded data should match original');
  console.log('✅ Screenshot data integrity verified');

  console.log('🎉 Screenshot + Data Marshal Integration Complete!');
  console.log('');
  console.log('📊 WORKFLOW SUMMARY:');
  console.log('1. ✅ Screenshot captured and marshalled for transmission');
  console.log('2. ✅ Chainable workflow created for command composition');
  console.log('3. ✅ Metadata extracted for AI analysis');
  console.log('4. ✅ Dimensions extracted for layout validation');
  console.log('5. ✅ Pure image data encoded for WebSocket safety');
  console.log('6. ✅ Data integrity verified through round-trip');
  console.log('');
  console.log('🚀 Ready for autonomous screenshot workflows!');
  
  return {
    marshalId: marshalResult.data?.marshalId,
    chainId: chainResult.data?.chainId,
    imageDataSize: imageDataOnly.length,
    screenshotDimensions: { 
      width: mockScreenshotResult.data.width, 
      height: mockScreenshotResult.data.height 
    }
  };
}

// Run the integration test
testScreenshotMarshalWorkflow()
  .then(result => {
    console.log('🎯 Integration test completed successfully!');
    console.log('📋 Result:', result);
  })
  .catch(error => {
    console.error('❌ Integration test failed:', error);
    process.exit(1);
  });