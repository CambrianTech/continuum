/**
 * Event Validation Patterns - Standard patterns for testing events
 * 
 * Eliminates duplication across event tests by providing reusable JavaScript snippets
 * for setup, validation, and cleanup.
 */

/**
 * Standard DOM event listener patterns
 */
export const DOMEventListenerPatterns = {
  /**
   * Basic chat message listener
   */
  CHAT_MESSAGE_LISTENER: (windowProperty: string) => `
    window.${windowProperty} = { domEventsReceived: 0, lastEvent: null, events: [] };
    
    document.addEventListener('chat:message-received', (event) => {
      window.${windowProperty}.domEventsReceived++;
      window.${windowProperty}.lastEvent = event.detail;
      window.${windowProperty}.events.push(event.detail);
      console.log('🎯 DOM EVENT:', window.${windowProperty}.domEventsReceived, event.detail);
    });
    
    return { success: true, listenerSetup: true };
  `,

  /**
   * Performance test listener
   */
  PERFORMANCE_LISTENER: (windowProperty: string) => `
    window.${windowProperty} = { 
      domEventsReceived: 0,
      startTime: Date.now()
    };
    
    document.addEventListener('chat:message-received', (event) => {
      window.${windowProperty}.domEventsReceived++;
    });
    
    return { success: true, perfSetup: true };
  `,

  /**
   * Validation listener with type checking
   */
  VALIDATION_LISTENER: (windowProperty: string) => `
    window.${windowProperty} = { validEvents: 0, invalidEvents: 0, events: [] };
    
    document.addEventListener('chat:message-received', (event) => {
      const data = event.detail;
      if (data?.messageId && data?.roomId && data?.message) {
        window.${windowProperty}.validEvents++;
        console.log('✅ Valid event:', data);
      } else {
        window.${windowProperty}.invalidEvents++;
        console.log('❌ Invalid event:', data);
      }
      window.${windowProperty}.events.push(data);
    });
    
    return { success: true, validationSetup: true };
  `
} as const;

/**
 * Standard validation checks
 */
export const ValidationChecks = {
  /**
   * Basic success check
   */
  BASIC_SUCCESS: (windowProperty: string) => `
    return {
      success: window.${windowProperty}?.domEventsReceived >= 1,
      domEventsReceived: window.${windowProperty}?.domEventsReceived || 0,
      lastEvent: window.${windowProperty}?.lastEvent || null
    };
  `,

  /**
   * Performance check
   */
  PERFORMANCE_CHECK: (windowProperty: string, expectedCount: number) => `
    const endTime = Date.now();
    const totalTime = endTime - window.${windowProperty}.startTime;
    
    return {
      success: window.${windowProperty}.domEventsReceived >= ${expectedCount},
      domEventsReceived: window.${windowProperty}.domEventsReceived,
      expectedEvents: ${expectedCount},
      totalTimeMs: totalTime
    };
  `,

  /**
   * Validation check
   */
  VALIDATION_CHECK: (windowProperty: string) => `
    return {
      success: window.${windowProperty}.validEvents >= 1 && window.${windowProperty}.invalidEvents === 0,
      validEvents: window.${windowProperty}.validEvents,
      invalidEvents: window.${windowProperty}.invalidEvents,
      totalEvents: window.${windowProperty}.events.length
    };
  `
} as const;

/**
 * Standard test messages
 */
export const StandardTestMessages = {
  BASIC: { roomId: 'test-basic', message: 'Basic test message', metadata: { test: 'basic', automated: true } },
  PERFORMANCE: (index: number) => ({ 
    roomId: `perf-${index}`, 
    message: `Performance test ${index + 1}`, 
    metadata: { test: 'performance', index, automated: true } 
  }),
  VALIDATION: { roomId: 'test-validation', message: 'Validation test', metadata: { test: 'validation', automated: true } },
  DEDUPLICATION: { roomId: 'test-dedup', message: 'Deduplication test', metadata: { test: 'dedup', automated: true } }
} as const;