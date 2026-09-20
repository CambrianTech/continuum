/**
 * Event Test Utilities - Shared testing infrastructure for event system tests
 * 
 * Provides common utilities, setup patterns, and verification methods
 * to reduce duplication across event and chat integration tests.
 */

import type { JTAGContext, JTAGMessage } from '../../system/core/types/JTAGTypes';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';
import { JTAGMessageFactory } from '../../system/core/types/JTAGTypes';

export interface EventTestResult {
  success: boolean;
  eventCount: number;
  proofExists: boolean;
  error?: string;
}

export interface MockEventSubscriber {
  receivedMessages: JTAGMessage[];
  uuid: string;
  handleMessage(message: JTAGMessage): Promise<any>;
}

/**
 * Create standardized test context
 */
export function createTestContext(environment: 'server' | 'browser' = 'server'): JTAGContext {
  return {
    uuid: generateUUID(),
    environment
  };
}

/**
 * Create a mock event subscriber for testing
 */
export function createMockEventSubscriber(subscriberId: string): MockEventSubscriber {
  return {
    receivedMessages: [],
    uuid: `mock-subscriber-${subscriberId}`,
    async handleMessage(message: JTAGMessage) {
      this.receivedMessages.push(message);
      return { success: true, message: `${subscriberId} received event` };
    }
  };
}

/**
 * Standard browser event listener setup code
 */
export function createBrowserEventListenerCode(eventName: string, elementId: string): string {
  return `
    // Create proof counter element
    let proofElement = document.createElement('div');
    proofElement.id = '${elementId}';
    proofElement.textContent = '${eventName} events: 0';
    proofElement.style.cssText = 'position: fixed; top: 10px; right: 10px; background: #ff6b6b; color: white; padding: 8px 12px; z-index: 9999; border-radius: 4px; font-family: monospace; font-size: 12px;';
    document.body.appendChild(proofElement);
    
    // Setup event counter
    let eventCount = 0;
    window.${elementId}Listener = function(event) {
      eventCount++;
      proofElement.textContent = \`${eventName} events: \${eventCount}\`;
      proofElement.style.background = eventCount > 0 ? '#4ecdc4' : '#ff6b6b';
      console.log('✅ BROWSER: ${eventName} event received!', event.detail || event);
    };
    
    // Setup event listener
    const jtagSystem = window.jtag;
    if (jtagSystem?.eventManager?.events) {
      jtagSystem.eventManager.events.on('${eventName}', window.${elementId}Listener);
      console.log('✅ BROWSER: Event listener registered for ${eventName}');
    } else {
      console.error('❌ BROWSER: JTAG event system not available');
    }
  `;
}

/**
 * Standard browser event proof check code
 */
export function createBrowserEventProofCode(elementId: string): string {
  return `
    const proofElement = document.getElementById('${elementId}');
    const eventCountText = proofElement?.textContent || '0';
    const eventCount = parseInt(eventCountText.match(/\\d+/)?.[0] || '0');
    
    console.log('🔍 BROWSER PROOF CHECK:');
    console.log('  - Proof element exists:', !!proofElement);
    console.log('  - Event count:', eventCount);
    console.log('  - Background color:', proofElement?.style.background);
    
    return {
      proofExists: !!proofElement,
      eventCount: eventCount,
      backgroundColor: proofElement?.style.background,
      success: eventCount > 0
    };
  `;
}

/**
 * Create standardized test event payload
 */
export function createTestEventPayload(eventName: string, roomId?: string, customData?: Record<string, any>) {
  return {
    eventName,
    data: {
      message: `Test ${eventName}`,
      roomId,
      timestamp: new Date().toISOString(),
      ...customData
    },
    scope: roomId ? { type: 'room' as const, id: roomId } : { type: 'system' as const },
    originSessionId: generateUUID(),
    originContextUUID: generateUUID(),
    timestamp: new Date().toISOString()
  };
}

/**
 * Wait for event propagation with configurable timeout
 */
export function waitForEventPropagation(timeoutMs: number = 1000): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, timeoutMs));
}

/**
 * Validate test result with descriptive error messages
 */
export function validateEventTestResult(
  testName: string, 
  result: EventTestResult, 
  expectedEventCount: number = 1
): void {
  if (!result.success || result.eventCount < expectedEventCount) {
    console.error(`❌ ${testName} FAILED:`);
    console.error(`   Expected events: >= ${expectedEventCount}`);
    console.error(`   Actual events: ${result.eventCount}`);
    console.error(`   Proof element: ${result.proofExists ? 'exists' : 'missing'}`);
    if (result.error) {
      console.error(`   Error: ${result.error}`);
    }
    throw new Error(`${testName} validation failed`);
  }
  
  console.log(`✅ ${testName} PASSED:`);
  console.log(`   Events received: ${result.eventCount}`);
  console.log(`   Proof verified: ${result.proofExists}`);
}

/**
 * Common test cleanup pattern
 */
export async function cleanupBrowserProofElements(client: any, elementIds: string[]): Promise<void> {
  await client.commands.exec({
    code: {
      type: 'inline',
      language: 'javascript',
      source: `
        ${elementIds.map(id => `
          const element = document.getElementById('${id}');
          if (element) element.remove();
        `).join('')}
        console.log('🧹 BROWSER: Cleanup completed for elements: ${elementIds.join(', ')}');
      `
    }
  });
}