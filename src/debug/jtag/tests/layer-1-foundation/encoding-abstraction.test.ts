#!/usr/bin/env tsx
/**
 * Encoding Abstraction Test
 * 
 * Tests the base64 encoding/decoding for transport abstraction
 */

import { JTAGPayload } from '@shared/JTAGTypes';
import { EventMessage } from '@systemEvents';

console.log('🧪 Encoding Abstraction Test Suite');

class TestPayload extends JTAGPayload {
  data: string;
  
  constructor(data: string) {
    super();
    this.data = data;
  }
}

function testBasicEncoding() {
  console.log('  📝 Testing basic payload encoding...');
  
  const payload = new TestPayload('test-data');
  const encoded = payload.encode();
  
  if (!encoded || encoded.length === 0) {
    throw new Error('Encoding failed - empty result');
  }
  
  console.log('  ✅ Basic encoding works');
}

function testEventMessageEncoding() {
  console.log('  📝 Testing EventMessage encoding...');
  
  const eventMsg = new EventMessage('system.ready', { timestamp: '2024-01-01' });
  const encoded = eventMsg.encode();
  
  if (!encoded || encoded.length === 0) {
    throw new Error('EventMessage encoding failed');
  }
  
  console.log('  ✅ EventMessage encoding works');
}

function testEncodingDecoding() {
  console.log('  📝 Testing encoding/decoding roundtrip...');
  
  const original = new TestPayload('roundtrip-test');
  const encoded = original.encode();
  const decoded = TestPayload.decode(encoded);
  
  if (decoded.data !== original.data) {
    throw new Error(`Roundtrip failed: ${decoded.data} !== ${original.data}`);
  }
  
  console.log('  ✅ Encoding/decoding roundtrip works');
}

// Run tests
try {
  testBasicEncoding();
  testEventMessageEncoding();
  testEncodingDecoding();
  
  console.log('✅ All encoding abstraction tests passed!');
  process.exit(0);
} catch (error) {
  console.error('❌ Encoding abstraction test failed:', error);
  process.exit(1);
}