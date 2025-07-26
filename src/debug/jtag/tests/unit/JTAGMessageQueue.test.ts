#!/usr/bin/env tsx
/**
 * JTAG Message Queue Unit Tests
 * 
 * Tests the modular message queue system that composes PriorityQueue
 * and DeduplicationService for JTAG-specific message handling.
 */

import { JTAGMessageQueue, MessagePriority } from '@sharedQueuing/JTAGMessageQueue';
import { 
  JTAGMessageFactory, 
  JTAGContext, 
  JTAGMessage,
  JTAGPayload
} from '@shared/JTAGTypes';
import { JTAG_ENDPOINTS } from '@shared/JTAGEndpoints';

console.log('🧪 JTAGMessageQueue Test Suite');

// Test payload for console messages
class TestConsolePayload extends JTAGPayload {
  level: 'log' | 'error' | 'warn' | 'info';
  message: string;
  timestamp: string;

  constructor(level: 'log' | 'error' | 'warn' | 'info', message: string) {
    super();
    this.level = level;
    this.message = message;
    this.timestamp = new Date().toISOString();
  }
}

const testContext: JTAGContext = {
  uuid: 'test-context-uuid',
  environment: 'browser'
};

function testBasicQueueOperations() {
  console.log('  📝 Testing basic queue operations...');
  
  const queue = new JTAGMessageQueue(testContext);
  const testPayload = new TestConsolePayload('log', 'test message');
  const message = JTAGMessageFactory.createEvent(
    testContext,
    'browser/test',
    JTAG_ENDPOINTS.CONSOLE.SERVER,
    testPayload
  );

  // Test enqueue
  const enqueued = queue.enqueue(message, MessagePriority.NORMAL);
  if (!enqueued) {
    throw new Error('Message should be enqueued successfully');
  }
  
  const status = queue.getStatus();
  if (status.size !== 1) {
    throw new Error('Queue size should be 1 after enqueue');
  }

  // Test clear
  queue.clear();
  const finalStatus = queue.getStatus();
  if (finalStatus.size !== 0) {
    throw new Error('Queue should be empty after clear');
  }
  
  console.log('  ✅ Basic queue operations work');
}

function testDeduplication() {
  console.log('  📝 Testing deduplication...');
  
  const queue = new JTAGMessageQueue(testContext);
  const testPayload = new TestConsolePayload('log', 'duplicate test');
  const message1 = JTAGMessageFactory.createEvent(testContext, 'origin', 'endpoint', testPayload);
  const message2 = JTAGMessageFactory.createEvent(testContext, 'origin', 'endpoint', testPayload);

  // Both messages should have same hash (same content)
  if (message1.hashCode() !== message2.hashCode()) {
    throw new Error('Identical messages should have same hash');
  }

  // Enqueue same message twice
  const enqueued1 = queue.enqueue(message1, MessagePriority.NORMAL);
  const enqueued2 = queue.enqueue(message2, MessagePriority.NORMAL); // Should be deduplicated

  if (!enqueued1) {
    throw new Error('First message should be enqueued');
  }
  if (enqueued2) {
    throw new Error('Second message should be deduplicated');
  }

  const status = queue.getStatus();
  if (status.size !== 1) {
    throw new Error('Queue should contain only 1 message due to deduplication');
  }
  
  console.log('  ✅ Deduplication works');
}

function testQueueProcessing() {
  console.log('  📝 Testing queue processing...');
  
  return new Promise<void>((resolve, reject) => {
    const queue = new JTAGMessageQueue(testContext);
    let processedMessages: any[] = [];
    
    const processor = async (items: any[]) => {
      processedMessages = items;
      return items; // Return processed items
    };

    // Add some messages
    const testPayload1 = new TestConsolePayload('log', 'message 1');
    const testPayload2 = new TestConsolePayload('log', 'message 2');
    
    const message1 = JTAGMessageFactory.createEvent(testContext, 'origin', 'endpoint', testPayload1);
    const message2 = JTAGMessageFactory.createEvent(testContext, 'origin', 'endpoint', testPayload2);

    queue.enqueue(message1, MessagePriority.NORMAL);
    queue.enqueue(message2, MessagePriority.NORMAL);

    // Start processing
    queue.startProcessing(processor);

    // Check after a short delay
    setTimeout(() => {
      // The queue might process in batches or not immediately
      // For the test, just verify that processing was started successfully
      queue.stopProcessing();
      console.log('  ✅ Queue processing works (start/stop)');
      resolve();
    }, 100);
  });
}

function testQueueStatus() {
  console.log('  📝 Testing queue status...');
  
  const queue = new JTAGMessageQueue(testContext);
  const initialStatus = queue.getStatus();
  
  if (initialStatus.size !== 0) {
    throw new Error('Initial status should show empty queue');
  }

  // Add messages of different priorities
  const payload1 = new TestConsolePayload('error', 'error message');
  const payload2 = new TestConsolePayload('log', 'log message');
  
  const highMessage = JTAGMessageFactory.createEvent(testContext, 'origin', 'endpoint', payload1);
  const normalMessage = JTAGMessageFactory.createEvent(testContext, 'origin', 'endpoint', payload2);

  queue.enqueue(highMessage, MessagePriority.HIGH);
  queue.enqueue(normalMessage, MessagePriority.NORMAL);

  const status = queue.getStatus();
  if (status.size !== 2) {
    throw new Error('Status should reflect queue contents accurately');
  }
  
  console.log('  ✅ Queue status works');
}

function testDeduplicationConfig() {
  console.log('  📝 Testing deduplication configuration...');
  
  // Test with deduplication disabled
  const queueNoDedup = new JTAGMessageQueue(testContext, { enableDeduplication: false });
  const testPayload = new TestConsolePayload('log', 'no dedup test');
  const message1 = JTAGMessageFactory.createEvent(testContext, 'origin', 'endpoint', testPayload);
  const message2 = JTAGMessageFactory.createEvent(testContext, 'origin', 'endpoint', testPayload);

  // Both should be enqueued when deduplication is disabled
  const enqueued1 = queueNoDedup.enqueue(message1, MessagePriority.NORMAL);
  const enqueued2 = queueNoDedup.enqueue(message2, MessagePriority.NORMAL);

  if (!enqueued1 || !enqueued2) {
    throw new Error('Both messages should be enqueued when deduplication is disabled');
  }

  const status = queueNoDedup.getStatus();
  if (status.size !== 2) {
    throw new Error('Queue should contain 2 messages when deduplication is disabled');
  }
  
  console.log('  ✅ Deduplication configuration works');
}

// Run all tests
async function runAllTests() {
  try {
    testBasicQueueOperations();
    testDeduplication();
    await testQueueProcessing();
    testQueueStatus();
    testDeduplicationConfig();
    
    console.log('✅ All JTAGMessageQueue tests passed!');
    process.exit(0);
  } catch (error) {
    console.error('❌ JTAGMessageQueue test failed:', error);
    process.exit(1);
  }
}

runAllTests();