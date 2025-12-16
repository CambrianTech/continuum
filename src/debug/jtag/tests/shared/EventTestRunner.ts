/**
 * Event Test Runner - Modular test execution framework for event system
 * 
 * Provides standardized test patterns with automatic cleanup and error handling.
 * Eliminates duplicate code across event tests.
 */

import { jtag } from '../../server-index';
import { EventTestUtils, EventTestPatterns } from '../../system/events/shared/EventTestUtils';

export interface EventTestCase {
  name: string;
  setup: string;        // JavaScript code to run in browser
  trigger: (client: any) => Promise<any>; // Function to trigger event with client
  validate: string;     // JavaScript code to validate result
  timeout?: number;
}

export class EventTestRunner {
  private client: any = null;

  async initialize(): Promise<void> {
    console.log('🔗 EventTestRunner: Connecting to JTAG system...');
    this.client = await jtag.connect({ targetEnvironment: 'server' });
    console.log('✅ EventTestRunner: Connected');
  }

  async runTest(testCase: EventTestCase): Promise<boolean> {
    return EventTestUtils.runTestWithCleanup(
      testCase.name,
      async () => {
        // Setup phase
        console.log(`📡 ${testCase.name}: Setting up...`);
        const setupResult = await this.client.commands.exec({
          code: {
            type: 'inline',
            language: 'javascript',
            source: testCase.setup
          }
        });

        if (!setupResult.commandResult?.result?.success) {
          throw new Error('Setup failed');
        }

        // Trigger phase  
        console.log(`📤 ${testCase.name}: Triggering event...`);
        await testCase.trigger(this.client);

        // Wait for propagation
        await new Promise(resolve => setTimeout(resolve, EventTestPatterns.EVENT_TIMEOUT_MS));

        // Validate phase
        console.log(`🔍 ${testCase.name}: Validating...`);
        const validateResult = await this.client.commands.exec({
          code: {
            type: 'inline',
            language: 'javascript', 
            source: testCase.validate
          }
        });

        const result = validateResult.commandResult?.result;
        if (!result?.success) {
          throw new Error(`Validation failed: ${JSON.stringify(result)}`);
        }

        return result;
      },
      testCase.timeout || EventTestPatterns.TEST_TIMEOUT_MS
    );
  }

  async runTestSuite(testCases: EventTestCase[]): Promise<void> {
    console.log(`🚀 EventTestRunner: Running ${testCases.length} tests...`);
    
    for (const testCase of testCases) {
      await this.runTest(testCase);
    }
    
    console.log('🎉 EventTestRunner: All tests passed!');
  }

  async cleanup(): Promise<void> {
    await EventTestUtils.cleanupClient(this.client);
  }
}

/**
 * Standard event test cases for reuse
 */
export const StandardEventTests = {
  basicDOMEvent: (): EventTestCase => ({
    name: 'Basic DOM Event Flow',
    setup: EventTestUtils.createDOMEventListener('chat:message-received', 'basicEventTest'),
    trigger: async () => {
      const runner = new EventTestRunner();
      await runner.initialize();
      await runner.client.commands['collaboration/chat/send'](
        EventTestUtils.createTestMessage(EventTestPatterns.TEST_ROOM_ID, 'basic')
      );
      await runner.cleanup();
    },
    validate: EventTestUtils.createEventCountCheck('basicEventTest')
  }),

  performanceTest: (messageCount: number = EventTestPatterns.PERF_MESSAGE_COUNT): EventTestCase => ({
    name: `Performance Test (${messageCount} messages)`,
    setup: `
      return (async function() {
        window.performanceTest = { 
          domEventsReceived: 0,
          startTime: Date.now()
        };
        
        document.addEventListener('chat:message-received', (event) => {
          window.performanceTest.domEventsReceived++;
        });
        
        return { success: true };
      })();
    `,
    trigger: async () => {
      const runner = new EventTestRunner();
      await runner.initialize();
      for (let i = 0; i < messageCount; i++) {
        await runner.client.commands['collaboration/chat/send'](
          EventTestUtils.createTestMessage(`perf-${i}`, 'performance')
        );
      }
      await runner.cleanup();
    },
    validate: `
      return (async function() {
        const endTime = Date.now();
        const totalTime = endTime - window.performanceTest.startTime;
        
        return {
          success: window.performanceTest.domEventsReceived >= ${messageCount},
          domEventsReceived: window.performanceTest.domEventsReceived,
          expectedEvents: ${messageCount},
          totalTimeMs: totalTime
        };
      })();
    `
  })
};