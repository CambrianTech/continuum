#!/usr/bin/env tsx
/**
 * JTAG Message Types Unit Tests
 * 
 * Tests the type-safe message system with Request/Event/Response patterns
 * and ensures proper TypeScript type checking and runtime validation.
 */

import { 
  JTAGMessageFactory, 
  JTAGMessageTypes, 
  JTAGEventMessage, 
  JTAGRequestMessage, 
  JTAGResponseMessage,
  JTAGMessage,
  JTAGPayload,
  JTAGContext
} from '../../shared/JTAGTypes';
import { JTAG_ENDPOINTS } from '../../shared/JTAGEndpoints';

console.log('🧪 JTAG Message Types Test Suite');

// Test payload types
class TestCommandPayload extends JTAGPayload {
  command: string;
  filename: string;

  constructor(command: string, filename: string) {
    super();
    this.command = command;
    this.filename = filename;
  }
}

class TestResultPayload extends JTAGPayload {
  success: boolean;
  result: string;

  constructor(success: boolean, result: string) {
    super();
    this.success = success;
    this.result = result;
  }
}

const testContext: JTAGContext = {
  uuid: 'test-context-uuid',
  environment: 'server'
};

function testEventMessageCreation() {
  console.log('  📝 Testing event message creation...');
  
  const testPayload = new TestCommandPayload('screenshot', 'test.png');
  const eventMessage = JTAGMessageFactory.createEvent(
    testContext,
    'server/test',
    JTAG_ENDPOINTS.CONSOLE.BROWSER,
    testPayload
  );

  if (eventMessage.messageType !== 'event') {
    throw new Error('Event message type incorrect');
  }
  if (eventMessage.context !== testContext) {
    throw new Error('Event context mismatch');
  }
  if (eventMessage.origin !== 'server/test') {
    throw new Error('Event origin mismatch');
  }
  if (eventMessage.endpoint !== JTAG_ENDPOINTS.CONSOLE.BROWSER) {
    throw new Error('Event endpoint mismatch');
  }
  if (eventMessage.payload !== testPayload) {
    throw new Error('Event payload mismatch');
  }
  if ('correlationId' in eventMessage) {
    throw new Error('Event message should not have correlationId');
  }
  
  console.log('  ✅ Event message creation works');
}

function testRequestMessageCreation() {
  console.log('  📝 Testing request message creation...');
  
  const testPayload = new TestCommandPayload('screenshot', 'test.png');
  const correlationId = 'test-correlation-id';
  const requestMessage = JTAGMessageFactory.createRequest(
    testContext,
    JTAG_ENDPOINTS.COMMANDS.SERVER,
    'browser/commands/screenshot',
    testPayload,
    correlationId
  );

  if (requestMessage.messageType !== 'request') {
    throw new Error('Request message type incorrect');
  }
  if (requestMessage.context !== testContext) {
    throw new Error('Request context mismatch');
  }
  if (requestMessage.correlationId !== correlationId) {
    throw new Error('Request correlation ID mismatch');
  }
  
  console.log('  ✅ Request message creation works');
}

function testResponseMessageCreation() {
  console.log('  📝 Testing response message creation...');
  
  const testResult = new TestResultPayload(true, 'screenshot-taken');
  const correlationId = 'matching-correlation-id';
  const responseMessage = JTAGMessageFactory.createResponse(
    testContext,
    'browser/commands/screenshot',
    JTAG_ENDPOINTS.COMMANDS.SERVER,
    testResult,
    correlationId
  );

  if (responseMessage.messageType !== 'response') {
    throw new Error('Response message type incorrect');
  }
  if (responseMessage.correlationId !== correlationId) {
    throw new Error('Response correlation ID mismatch');
  }
  
  console.log('  ✅ Response message creation works');
}

function testMessageTypeGuards() {
  console.log('  📝 Testing message type guards...');
  
  const testPayload = new TestCommandPayload('screenshot', 'test.png');
  const testResult = new TestResultPayload(true, 'screenshot-taken');
  
  const eventMessage = JTAGMessageFactory.createEvent(testContext, 'origin', 'endpoint', testPayload);
  const requestMessage = JTAGMessageFactory.createRequest(testContext, 'origin', 'endpoint', testPayload, 'corr-1');
  const responseMessage = JTAGMessageFactory.createResponse(testContext, 'origin', 'endpoint', testResult, 'corr-1');

  // Test event type guard
  if (!JTAGMessageTypes.isEvent(eventMessage)) {
    throw new Error('Event type guard failed');
  }
  if (JTAGMessageTypes.isRequest(eventMessage) || JTAGMessageTypes.isResponse(eventMessage)) {
    throw new Error('Event type guard false positive');
  }

  // Test request type guard
  if (!JTAGMessageTypes.isRequest(requestMessage)) {
    throw new Error('Request type guard failed');
  }
  if (JTAGMessageTypes.isEvent(requestMessage) || JTAGMessageTypes.isResponse(requestMessage)) {
    throw new Error('Request type guard false positive');
  }

  // Test response type guard
  if (!JTAGMessageTypes.isResponse(responseMessage)) {
    throw new Error('Response type guard failed');
  }
  if (JTAGMessageTypes.isEvent(responseMessage) || JTAGMessageTypes.isRequest(responseMessage)) {
    throw new Error('Response type guard false positive');
  }
  
  console.log('  ✅ Message type guards work');
}

function testCorrelationIdGeneration() {
  console.log('  📝 Testing correlation ID generation...');
  
  const id1 = JTAGMessageFactory.generateCorrelationId();
  const id2 = JTAGMessageFactory.generateCorrelationId();
  
  if (!id1 || !id2) {
    throw new Error('Correlation ID generation failed');
  }
  if (id1 === id2) {
    throw new Error('Correlation IDs should be unique');
  }
  if (!id1.startsWith('corr_')) {
    throw new Error('Correlation ID format incorrect');
  }
  
  console.log('  ✅ Correlation ID generation works');
}

function testMessageHashing() {
  console.log('  📝 Testing message hashing...');
  
  const testPayload = new TestCommandPayload('screenshot', 'test.png');
  const message1 = JTAGMessageFactory.createEvent(testContext, 'origin', 'endpoint', testPayload);
  const message2 = JTAGMessageFactory.createEvent(testContext, 'origin', 'endpoint', testPayload);
  
  const hash1 = message1.hashCode();
  const hash2 = message2.hashCode();
  
  if (!hash1 || !hash2) {
    throw new Error('Message hashing failed');
  }
  if (hash1 !== hash2) {
    throw new Error('Identical messages should have same hash');
  }
  
  console.log('  ✅ Message hashing works');
}

// Run all tests
try {
  testEventMessageCreation();
  testRequestMessageCreation();
  testResponseMessageCreation();
  testMessageTypeGuards();
  testCorrelationIdGeneration();
  testMessageHashing();
  
  console.log('✅ All JTAG Message Types tests passed!');
  process.exit(0);
} catch (error) {
  console.error('❌ JTAG Message Types test failed:', error);
  process.exit(1);
}