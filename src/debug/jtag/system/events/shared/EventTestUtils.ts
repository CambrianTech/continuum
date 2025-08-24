/**
 * Event Test Utilities - Reusable test patterns for event system validation
 * 
 * Provides standardized testing patterns for DOM events, connection cleanup,
 * and cross-environment event validation.
 */

import type { ChatMessageEventData } from './EventSystemTypes';

/**
 * Standard DOM event listener setup for tests
 */
export class EventTestUtils {
  /**
   * Creates standardized DOM event listener with counter
   */
  static createDOMEventListener(eventName: string, windowProperty: string): string {
    return `
      return (async function() {
        console.log('🔧 Setting up DOM event listener for ${eventName}');
        
        window.${windowProperty} = { 
          domEventsReceived: 0,
          lastEvent: null,
          events: []
        };
        
        document.addEventListener('${eventName}', (event) => {
          window.${windowProperty}.domEventsReceived++;
          window.${windowProperty}.lastEvent = event.detail;
          window.${windowProperty}.events.push(event.detail);
          console.log('🎯 DOM EVENT RECEIVED!', {
            count: window.${windowProperty}.domEventsReceived,
            detail: event.detail
          });
        });
        
        console.log('✅ DOM event listener registered');
        
        return { success: true, listenerSetup: true };
      })();
    `;
  }

  /**
   * Creates event counter check script
   */
  static createEventCountCheck(windowProperty: string): string {
    return `
      return (async function() {
        return {
          success: window.${windowProperty}?.domEventsReceived >= 1,
          domEventsReceived: window.${windowProperty}?.domEventsReceived || 0,
          lastEvent: window.${windowProperty}?.lastEvent || null,
          allEvents: window.${windowProperty}?.events || []
        };
      })();
    `;
  }

  /**
   * Standard test message for cross-environment validation
   */
  static createTestMessage(roomId: string, testType: string): {
    roomId: string;
    message: string;
    metadata: Record<string, unknown>;
  } {
    return {
      roomId: roomId,
      message: `Test message for ${testType}`,
      metadata: { 
        test: testType,
        timestamp: new Date().toISOString(),
        automated: true
      }
    };
  }

  /**
   * Standard client cleanup pattern
   */
  static async cleanupClient(client: any): Promise<void> {
    if (client?.disconnect) {
      try {
        console.log('🔌 Cleaning up client connection...');
        await client.disconnect();
        console.log('✅ Client disconnected');
      } catch (disconnectError) {
        console.warn('⚠️ Disconnect warning:', disconnectError);
      }
    }
  }

  /**
   * Standard test wrapper with timeout and cleanup
   */
  static async runTestWithCleanup<T>(
    testName: string,
    testFn: () => Promise<T>,
    timeoutMs: number = 30000
  ): Promise<T> {
    console.log(`🧪 ${testName}...`);
    
    const timeoutPromise = new Promise<never>((_, reject) => {
      setTimeout(() => reject(new Error(`❌ TIMEOUT: Event test '${testName}' failed to complete within ${timeoutMs}ms - test cancelled`)), timeoutMs);
    });
    
    try {
      const result = await Promise.race([testFn(), timeoutPromise]);
      console.log(`✅ ${testName} PASSED`);
      return result;
    } catch (error) {
      console.error(`❌ ${testName} FAILED:`, error);
      throw error;
    }
  }

  /**
   * Validates event data structure
   */
  static validateChatEvent(eventData: unknown): eventData is ChatMessageEventData {
    return !!(
      eventData &&
      typeof eventData === 'object' &&
      eventData.messageId &&
      eventData.roomId &&
      eventData.message &&
      eventData.timestamp
    );
  }

  /**
   * Standard performance test pattern
   */
  static async performanceTest(
    client: any,
    roomId: string,
    messageCount: number,
    windowProperty: string
  ): Promise<{ success: boolean; eventsReceived: number; totalTimeMs: number }> {
    // Setup listener
    await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          return (async function() {
            window.${windowProperty} = { 
              domEventsReceived: 0,
              startTime: Date.now()
            };
            
            document.addEventListener('chat:message-received', (event) => {
              window.${windowProperty}.domEventsReceived++;
            });
            
            return { success: true };
          })();
        `
      }
    });
    
    // Send messages
    for (let i = 0; i < messageCount; i++) {
      await client.commands['chat/send-message']({
        roomId: `${roomId}-${i}`,
        message: `Performance test ${i + 1}`,
        metadata: { test: 'performance', index: i }
      });
    }
    
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Check results
    const result = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          return (async function() {
            const endTime = Date.now();
            const totalTime = endTime - window.${windowProperty}.startTime;
            
            return {
              success: window.${windowProperty}.domEventsReceived >= ${messageCount},
              domEventsReceived: window.${windowProperty}.domEventsReceived,
              expectedEvents: ${messageCount},
              totalTimeMs: totalTime
            };
          })();
        `
      }
    });
    
    return result.commandResult?.result || { success: false, eventsReceived: 0, totalTimeMs: 0 };
  }
}

/**
 * Event test patterns for common scenarios
 */
export const EventTestPatterns = {
  // Standard DOM event listener for chat messages
  CHAT_DOM_LISTENER: 'chat:message-received',
  
  // Standard room ID for tests
  TEST_ROOM_ID: 'test-room-events',
  
  // Standard timeout for event propagation
  EVENT_TIMEOUT_MS: 1000,
  
  // Standard timeout for tests
  TEST_TIMEOUT_MS: 30000,
  
  // Standard performance test message count
  PERF_MESSAGE_COUNT: 5
} as const;