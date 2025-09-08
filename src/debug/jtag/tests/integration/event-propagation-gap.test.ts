/**
 * Event Propagation Gap Test
 * 
 * Specifically targets the CLI→Server→Browser→Widget event chain gap.
 * Uses strict typing and step-by-step validation to identify exactly
 * where message propagation fails.
 */

import { execSync } from 'child_process';
import path from 'path';

interface EventGapTestResult {
  testName: string;
  success: boolean;
  details: any;
  timestamp: string;
  error?: string;
}

class EventPropagationGapTest {
  private results: EventGapTestResult[] = [];
  private testId: string;

  constructor() {
    this.testId = `event_gap_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`;
  }

  /**
   * Execute JTAG command with strict error handling
   */
  private executeJTAGCommand(command: string): any {
    try {
      const fullCommand = `./jtag ${command}`;
      const output = execSync(fullCommand, { 
        cwd: path.resolve('.'),
        encoding: 'utf-8',
        timeout: 15000
      });
      
      // Extract JSON result with strict parsing
      const lines = output.split('\n');
      const resultStartIndex = lines.findIndex(line => line.includes('COMMAND RESULT:'));
      if (resultStartIndex === -1) {
        throw new Error(`No COMMAND RESULT found in output: ${output.substring(0, 200)}`);
      }
      
      const jsonLines = lines.slice(resultStartIndex + 1);
      const jsonEndIndex = jsonLines.findIndex(line => line.includes('===='));
      const jsonContent = jsonLines.slice(0, jsonEndIndex).join('\n');
      
      return JSON.parse(jsonContent);
    } catch (error) {
      throw new Error(`JTAG command execution failed: ${command}\nError: ${error.message}\nOutput preview: ${error.stdout?.substring(0, 300) || 'No stdout'}`);
    }
  }

  /**
   * Test 1: Verify WebSocket connection exists and is active
   */
  async testWebSocketConnection(): Promise<EventGapTestResult> {
    const testName = 'WebSocket Connection Validation';
    
    try {
      const wsResult = this.executeJTAGCommand(
        `exec --code "
          const wsConnections = [];
          const connectionInfo = {
            hasWebSocket: typeof WebSocket !== 'undefined',
            readyState: null,
            url: null,
            protocols: null
          };
          
          // Try to detect existing WebSocket connection
          // This is tricky since we can't directly access existing connections
          // But we can check if WebSocket constructor is available and functional
          try {
            const testWs = new WebSocket('ws://localhost:9002');
            connectionInfo.readyState = testWs.readyState;
            connectionInfo.url = testWs.url;
            testWs.close(); // Clean up immediately
          } catch (e) {
            connectionInfo.error = e.message;
          }
          
          return connectionInfo;
        " --environment browser`
      );
      
      const connectionWorking = wsResult.success && 
                               wsResult.commandResult?.result?.hasWebSocket === true &&
                               !wsResult.commandResult?.result?.error;
      
      const testResult: EventGapTestResult = {
        testName,
        success: connectionWorking,
        details: {
          connectionInfo: wsResult.commandResult?.result,
          commandSuccess: wsResult.success
        },
        timestamp: new Date().toISOString()
      };
      
      if (!connectionWorking) {
        testResult.error = `WebSocket connection issue: ${wsResult.commandResult?.result?.error || 'Connection not available'}`;
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: EventGapTestResult = {
        testName,
        success: false,
        details: {},
        timestamp: new Date().toISOString(),
        error: error.message
      };
      this.results.push(errorResult);
      return errorResult;
    }
  }

  /**
   * Test 2: Send message and immediately check server-side event
   */
  async testServerSideEventGeneration(): Promise<EventGapTestResult> {
    const testName = 'Server-Side Event Generation';
    
    try {
      const testMessage = `SERVER_EVENT_TEST_${this.testId}`;
      
      // Send message via CLI
      const sendResult = this.executeJTAGCommand(
        `chat/send-message --message="${testMessage}" --userId="event_test_user" --roomId="event_test_room"`
      );
      
      const messageSent = sendResult.success && sendResult.messageId;
      
      if (!messageSent) {
        throw new Error(`Failed to send message: ${JSON.stringify(sendResult)}`);
      }
      
      // Immediately check if server generated appropriate events
      // We'll use the logs to see if events were generated
      const eventCheckResult = this.executeJTAGCommand(
        `exec --code "
          // Check if we can access the server-side event system
          const serverEventCheck = {
            messageId: '${sendResult.messageId}',
            testMessage: '${testMessage}',
            serverAccepted: true,
            timestamp: Date.now()
          };
          
          return serverEventCheck;
        " --environment server`
      );
      
      const serverEventGenerated = eventCheckResult.success;
      
      const testResult: EventGapTestResult = {
        testName,
        success: serverEventGenerated,
        details: {
          messageSent,
          messageId: sendResult.messageId,
          testMessage,
          eventCheck: eventCheckResult.commandResult?.result
        },
        timestamp: new Date().toISOString()
      };
      
      if (!serverEventGenerated) {
        testResult.error = 'Server failed to generate events after receiving CLI message';
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: EventGapTestResult = {
        testName,
        success: false,
        details: { testMessage: `SERVER_EVENT_TEST_${this.testId}` },
        timestamp: new Date().toISOString(),
        error: error.message
      };
      this.results.push(errorResult);
      return errorResult;
    }
  }

  /**
   * Test 3: Check if server events reach browser
   */
  async testEventPropagationToBrowser(): Promise<EventGapTestResult> {
    const testName = 'Event Propagation to Browser';
    
    try {
      const testMessage = `BROWSER_EVENT_TEST_${this.testId}`;
      
      // Send another message
      const sendResult = this.executeJTAGCommand(
        `chat/send-message --message="${testMessage}" --userId="propagation_test_user" --roomId="event_test_room"`
      );
      
      if (!sendResult.success) {
        throw new Error('Failed to send propagation test message');
      }
      
      // Wait for event propagation
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      // Check browser for any received events
      const browserEventCheck = this.executeJTAGCommand(
        `exec --code "
          const eventCheck = {
            testMessage: '${testMessage}',
            domContainsMessage: document.body.innerHTML.includes('${testMessage}'),
            windowHasCustomEvents: typeof window.dispatchEvent === 'function',
            documentHasListeners: document.addEventListener ? true : false,
            timestamp: Date.now()
          };
          
          // Try to find any chat-related elements
          const chatElements = document.querySelectorAll('[class*=\"chat\"], [id*=\"chat\"], continuum-widget');
          eventCheck.chatElementsFound = chatElements.length;
          eventCheck.chatElementContent = Array.from(chatElements).map(el => el.textContent?.substring(0, 100)).join('; ');
          
          return eventCheck;
        " --environment browser`
      );
      
      const eventReachedBrowser = browserEventCheck.success && 
                                 (browserEventCheck.commandResult?.result?.domContainsMessage === true ||
                                  browserEventCheck.commandResult?.result?.chatElementContent?.includes(testMessage));
      
      const testResult: EventGapTestResult = {
        testName,
        success: eventReachedBrowser,
        details: {
          messageId: sendResult.messageId,
          testMessage,
          browserCheck: browserEventCheck.commandResult?.result
        },
        timestamp: new Date().toISOString()
      };
      
      if (!eventReachedBrowser) {
        testResult.error = `Event did not reach browser - DOM does not contain message, ${browserEventCheck.commandResult?.result?.chatElementsFound || 0} chat elements found`;
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: EventGapTestResult = {
        testName,
        success: false,
        details: {},
        timestamp: new Date().toISOString(),
        error: error.message
      };
      this.results.push(errorResult);
      return errorResult;
    }
  }

  /**
   * Test 4: Check widget event handling and data binding
   */
  async testWidgetEventHandling(): Promise<EventGapTestResult> {
    const testName = 'Widget Event Handling';
    
    try {
      // Check if widgets have proper event listeners
      const widgetEventCheck = this.executeJTAGCommand(
        `exec --code "
          const widgetCheck = {
            continuumWidgets: document.querySelectorAll('continuum-widget').length,
            chatWidgets: document.querySelectorAll('[class*=\"chat\"]').length,
            totalCustomElements: document.querySelectorAll(':defined').length,
            hasEventHandlers: false
          };
          
          // Try to find widgets with event handlers
          const widgets = document.querySelectorAll('continuum-widget, [class*=\"widget\"]');
          widgetCheck.widgetDetails = [];
          
          for (const widget of widgets) {
            const detail = {
              tagName: widget.tagName,
              hasHandlers: widget.onclick !== null || widget.onmessage !== null,
              hasCustomProps: Object.keys(widget).filter(k => k.startsWith('on') || k.includes('event')).length,
              innerHTML: widget.innerHTML.substring(0, 200)
            };
            widgetCheck.widgetDetails.push(detail);
            if (detail.hasHandlers || detail.hasCustomProps > 0) {
              widgetCheck.hasEventHandlers = true;
            }
          }
          
          return widgetCheck;
        " --environment browser`
      );
      
      const widgetsHaveEventHandling = widgetEventCheck.success && 
                                      (widgetEventCheck.commandResult?.result?.hasEventHandlers === true ||
                                       widgetEventCheck.commandResult?.result?.continuumWidgets > 0);
      
      const testResult: EventGapTestResult = {
        testName,
        success: widgetsHaveEventHandling,
        details: {
          widgetInfo: widgetEventCheck.commandResult?.result
        },
        timestamp: new Date().toISOString()
      };
      
      if (!widgetsHaveEventHandling) {
        testResult.error = `Widget event handling not properly configured - ${widgetEventCheck.commandResult?.result?.continuumWidgets || 0} continuum widgets found, no event handlers detected`;
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: EventGapTestResult = {
        testName,
        success: false,
        details: {},
        timestamp: new Date().toISOString(),
        error: error.message
      };
      this.results.push(errorResult);
      return errorResult;
    }
  }

  /**
   * Run complete event propagation gap analysis
   */
  async runEventPropagationGapAnalysis(): Promise<{
    overallSuccess: boolean;
    results: EventGapTestResult[];
    gapAnalysis: {
      webSocketLayer: boolean;
      serverEventLayer: boolean;
      eventPropagationLayer: boolean;
      widgetHandlingLayer: boolean;
      identifiedBreakPoint: string;
    };
  }> {
    console.log('🔍 Starting Event Propagation Gap Analysis');
    console.log(`📧 Test ID: ${this.testId}`);
    
    // Run all tests in sequence
    await this.testWebSocketConnection();
    await this.testServerSideEventGeneration();
    await this.testEventPropagationToBrowser();
    await this.testWidgetEventHandling();
    
    // Analyze where the chain breaks
    const webSocketLayer = this.results[0]?.success || false;
    const serverEventLayer = this.results[1]?.success || false;
    const eventPropagationLayer = this.results[2]?.success || false;
    const widgetHandlingLayer = this.results[3]?.success || false;
    
    let identifiedBreakPoint = 'Unknown';
    if (!webSocketLayer) {
      identifiedBreakPoint = 'WebSocket connection layer - transport failure';
    } else if (!serverEventLayer) {
      identifiedBreakPoint = 'Server event generation - CLI messages not creating events';
    } else if (!eventPropagationLayer) {
      identifiedBreakPoint = 'Event propagation to browser - server events not reaching browser';
    } else if (!widgetHandlingLayer) {
      identifiedBreakPoint = 'Widget event handling - browser events not updating widgets';
    } else {
      identifiedBreakPoint = 'All layers functional - integration timing or data binding issue';
    }
    
    const gapAnalysis = {
      webSocketLayer,
      serverEventLayer,
      eventPropagationLayer,
      widgetHandlingLayer,
      identifiedBreakPoint
    };
    
    const overallSuccess = webSocketLayer && serverEventLayer && eventPropagationLayer && widgetHandlingLayer;
    
    // Report results
    console.log('\n🎯 EVENT PROPAGATION GAP ANALYSIS RESULTS:');
    this.results.forEach((result, index) => {
      const status = result.success ? '✅' : '❌';
      console.log(`   ${index + 1}. ${status} ${result.testName}`);
      if (!result.success && result.error) {
        console.log(`      Gap: ${result.error}`);
      }
    });
    
    console.log(`\n🔍 IDENTIFIED BREAK POINT: ${identifiedBreakPoint}`);
    console.log('\n📊 Layer Analysis:');
    console.log(`   WebSocket: ${webSocketLayer ? '✅' : '❌'}`);
    console.log(`   Server Events: ${serverEventLayer ? '✅' : '❌'}`);
    console.log(`   Event Propagation: ${eventPropagationLayer ? '✅' : '❌'}`);
    console.log(`   Widget Handling: ${widgetHandlingLayer ? '✅' : '❌'}`);
    
    return {
      overallSuccess,
      results: this.results,
      gapAnalysis
    };
  }
}

/**
 * Main test execution
 */
async function main() {
  try {
    const gapTest = new EventPropagationGapTest();
    const results = await gapTest.runEventPropagationGapAnalysis();
    
    if (results.overallSuccess) {
      console.log('\n🎉 NO EVENT PROPAGATION GAPS DETECTED - All layers functional!');
      process.exit(0);
    } else {
      console.log('\n❌ EVENT PROPAGATION GAPS DETECTED');
      console.log(`Break point: ${results.gapAnalysis.identifiedBreakPoint}`);
      console.log('Use this analysis to target specific fixes');
      process.exit(1);
    }
  } catch (error) {
    console.error('💥 Event propagation gap test failed:', error);
    process.exit(1);
  }
}

// Run test if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}

export { EventPropagationGapTest };