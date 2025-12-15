/**
 * SHADOW DOM END-TO-END INTEGRATION TEST
 * 
 * This test validates REAL end-to-end integration by:
 * 1. Sending messages via CLI commands
 * 2. Verifying they appear in Shadow DOM widgets in the browser
 * 3. Testing event propagation from CLI → Server → Browser → Widget Shadow DOM
 * 
 * Key Challenge: Widget UI uses Shadow DOM, so standard selectors don't work.
 * This test uses Shadow DOM traversal to actually verify integration.
 */

import { JTAGTestFramework } from '../framework/JTAGTestFramework.js';
import { TestResult, TestContext, ValidationResult } from '../framework/TestTypes.js';

interface ShadowDOMTestResult extends TestResult {
  messageInShadowDOM: boolean;
  eventSystemWorking: boolean;
  chatHistoryAccessible: boolean;
  realIntegrationConfirmed: boolean;
}

class ShadowDOMEndToEndValidator {
  private framework: JTAGTestFramework;
  private testMessage: string;
  private messageId: string | null = null;

  constructor(framework: JTAGTestFramework) {
    this.framework = framework;
    this.testMessage = `END_TO_END_TEST_${Date.now()}_Claude_Integration_Validation`;
  }

  /**
   * STEP 1: Send message via CLI and get messageId
   */
  async sendMessageViaCLI(): Promise<{ success: boolean; messageId?: string }> {
    try {
      const result = await this.framework.executeCommand('chat/send-message', {
        roomId: 'general',
        userId: 'e2e_test_user',
        message: this.testMessage,
        timestamp: Date.now()
      });

      if (result.success && result.messageId) {
        this.messageId = result.messageId;
        return { success: true, messageId: result.messageId };
      }
      
      return { success: false };
    } catch (error) {
      console.error('Failed to send message via CLI:', error);
      return { success: false };
    }
  }

  /**
   * STEP 2: Use Shadow DOM traversal to find the message in browser widgets
   */
  async verifyMessageInShadowDOM(): Promise<{ found: boolean; shadowContent?: string }> {
    try {
      // Execute code in browser to traverse Shadow DOM
      const shadowDOMCheck = await this.framework.executeCommand('exec', {
        code: `
          // SHADOW DOM TRAVERSAL FUNCTION
          function findInShadowDOM(selector, searchText) {
            console.log('🔍 Searching Shadow DOM for:', searchText);
            
            // Find all potential shadow hosts
            const allElements = document.querySelectorAll('*');
            const results = [];
            
            for (const element of allElements) {
              // Check if element has shadow root
              if (element.shadowRoot) {
                console.log('📦 Found shadow root in:', element.tagName);
                
                // Search inside shadow DOM
                const shadowContent = element.shadowRoot.innerHTML;
                if (shadowContent.includes(searchText)) {
                  results.push({
                    host: element.tagName,
                    content: shadowContent.substring(0, 500) + '...',
                    found: true
                  });
                }
                
                // Also search for chat-specific elements
                const chatElements = element.shadowRoot.querySelectorAll('.message, .chat-message, [data-message-id]');
                for (const chatEl of chatElements) {
                  if (chatEl.textContent && chatEl.textContent.includes(searchText)) {
                    results.push({
                      host: element.tagName,
                      element: chatEl.tagName,
                      text: chatEl.textContent,
                      found: true
                    });
                  }
                }
              }
            }
            
            // Also check main document (non-shadow)
            const mainDocumentMatch = document.body.textContent.includes(searchText);
            if (mainDocumentMatch) {
              results.push({
                host: 'MAIN_DOCUMENT',
                found: true,
                content: 'Found in main document'
              });
            }
            
            return {
              searchText,
              totalShadowRoots: Array.from(allElements).filter(el => el.shadowRoot).length,
              results,
              found: results.length > 0
            };
          }
          
          // Search for our test message
          const searchResult = findInShadowDOM('*', '${this.testMessage}');
          console.log('🎯 Shadow DOM search result:', searchResult);
          
          return searchResult;
        `,
        environment: 'browser'
      });

      if (shadowDOMCheck.success && shadowDOMCheck.found) {
        return { found: true, shadowContent: JSON.stringify(shadowDOMCheck, null, 2) };
      }

      return { found: false, shadowContent: JSON.stringify(shadowDOMCheck, null, 2) };
    } catch (error) {
      console.error('Shadow DOM verification failed:', error);
      return { found: false };
    }
  }

  /**
   * STEP 3: Test chat history retrieval
   */
  async testChatHistoryAccess(): Promise<{ accessible: boolean; historyData?: any }> {
    try {
      // Try to retrieve chat history via data commands
      const historyResult = await this.framework.executeCommand(DATA_COMMANDS.LIST, {
        collection: 'messages',
        filter: { roomId: 'general' },
        format: 'json'
      });

      if (historyResult.success && historyResult.items) {
        // Look for our test message in the history
        const ourMessage = historyResult.items.find((item: any) => 
          item.message && item.message.includes(this.testMessage)
        );

        return {
          accessible: true,
          historyData: {
            totalMessages: historyResult.items.length,
            foundOurMessage: !!ourMessage,
            messageDetails: ourMessage
          }
        };
      }

      return { accessible: false, historyData: historyResult };
    } catch (error) {
      console.error('Chat history access failed:', error);
      return { accessible: false };
    }
  }

  /**
   * STEP 4: Test event system by triggering events and checking widget response
   */
  async testEventSystemIntegration(): Promise<{ working: boolean; eventData?: any }> {
    try {
      // Send another message and immediately check for real-time updates
      const eventTestMessage = `EVENT_TEST_${Date.now()}`;
      
      const sendResult = await this.framework.executeCommand('chat/send-message', {
        roomId: 'general',
        userId: 'event_test_user',
        message: eventTestMessage,
        timestamp: Date.now()
      });

      if (!sendResult.success) {
        return { working: false, eventData: { error: 'Failed to send event test message' } };
      }

      // Wait a moment for event propagation
      await new Promise(resolve => setTimeout(resolve, 1000));

      // Check if event appeared in browser widgets
      const eventCheck = await this.framework.executeCommand('exec', {
        code: `
          // Check for the event test message in Shadow DOM
          function checkForEventMessage(searchText) {
            const allElements = document.querySelectorAll('*');
            let found = false;
            
            for (const element of allElements) {
              if (element.shadowRoot) {
                const shadowContent = element.shadowRoot.innerHTML;
                if (shadowContent.includes(searchText)) {
                  found = true;
                  break;
                }
              }
            }
            
            return {
              eventMessage: searchText,
              foundInShadowDOM: found,
              timestamp: Date.now()
            };
          }
          
          return checkForEventMessage('${eventTestMessage}');
        `,
        environment: 'browser'
      });

      return {
        working: eventCheck.success && eventCheck.foundInShadowDOM,
        eventData: eventCheck
      };
    } catch (error) {
      console.error('Event system test failed:', error);
      return { working: false, eventData: { error: error.message } };
    }
  }

  /**
   * MAIN TEST: Complete end-to-end validation
   */
  async runCompleteEndToEndTest(): Promise<ShadowDOMTestResult> {
    const testId = `shadow-dom-e2e-${Date.now()}`;
    console.log(`🚀 Starting Shadow DOM End-to-End Integration Test: ${testId}`);

    // Step 1: Send message via CLI
    console.log('📤 Step 1: Sending message via CLI...');
    const cliResult = await this.sendMessageViaCLI();
    
    if (!cliResult.success) {
      return {
        testId,
        name: 'Shadow DOM End-to-End Integration',
        success: false,
        error: 'Failed to send message via CLI',
        messageInShadowDOM: false,
        eventSystemWorking: false,
        chatHistoryAccessible: false,
        realIntegrationConfirmed: false
      };
    }

    console.log(`✅ Message sent via CLI, messageId: ${cliResult.messageId}`);

    // Step 2: Wait for message propagation
    await new Promise(resolve => setTimeout(resolve, 2000));

    // Step 3: Check Shadow DOM for message
    console.log('🔍 Step 2: Checking Shadow DOM for message...');
    const shadowResult = await this.verifyMessageInShadowDOM();
    
    // Step 4: Test chat history access
    console.log('📚 Step 3: Testing chat history access...');
    const historyResult = await this.testChatHistoryAccess();
    
    // Step 5: Test event system
    console.log('⚡ Step 4: Testing event system integration...');
    const eventResult = await this.testEventSystemIntegration();

    // Final assessment
    const realIntegrationConfirmed = shadowResult.found && historyResult.accessible && eventResult.working;

    const result: ShadowDOMTestResult = {
      testId,
      name: 'Shadow DOM End-to-End Integration',
      success: realIntegrationConfirmed,
      messageInShadowDOM: shadowResult.found,
      eventSystemWorking: eventResult.working,
      chatHistoryAccessible: historyResult.accessible,
      realIntegrationConfirmed,
      details: {
        cliMessageResult: cliResult,
        shadowDOMResult: shadowResult,
        chatHistoryResult: historyResult,
        eventSystemResult: eventResult
      }
    };

    // Log comprehensive results
    console.log('\n📊 SHADOW DOM E2E TEST RESULTS:');
    console.log(`   CLI Message Send: ${cliResult.success ? '✅' : '❌'}`);
    console.log(`   Message in Shadow DOM: ${shadowResult.found ? '✅' : '❌'}`);
    console.log(`   Chat History Access: ${historyResult.accessible ? '✅' : '❌'}`);
    console.log(`   Event System Working: ${eventResult.working ? '✅' : '❌'}`);
    console.log(`   REAL INTEGRATION: ${realIntegrationConfirmed ? '✅' : '❌'}`);
    
    if (!realIntegrationConfirmed) {
      console.log('\n❌ INTEGRATION GAPS FOUND:');
      if (!shadowResult.found) console.log('   • Message not visible in Shadow DOM widgets');
      if (!historyResult.accessible) console.log('   • Chat history not properly accessible');
      if (!eventResult.working) console.log('   • Event system not propagating to widgets');
    }

    return result;
  }
}

/**
 * MAIN TEST EXECUTION
 */
export async function runShadowDOMEndToEndTest(): Promise<TestResult> {
  const framework = new JTAGTestFramework();
  
  try {
    // Initialize framework
    await framework.initialize();
    
    // Create validator and run test
    const validator = new ShadowDOMEndToEndValidator(framework);
    const result = await validator.runCompleteEndToEndTest();
    
    return result;
  } catch (error) {
    return {
      testId: `shadow-dom-e2e-error-${Date.now()}`,
      name: 'Shadow DOM End-to-End Integration',
      success: false,
      error: `Test execution failed: ${error.message}`,
      details: { error }
    };
  } finally {
    await framework.cleanup();
  }
}

// Run test if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  runShadowDOMEndToEndTest()
    .then(result => {
      if (result.success) {
        console.log('🎉 Shadow DOM End-to-End Integration Test PASSED');
        process.exit(0);
      } else {
        console.log('❌ Shadow DOM End-to-End Integration Test FAILED');
        console.log(result.error || 'Unknown error');
        process.exit(1);
      }
    })
    .catch(error => {
      console.error('💥 Test framework error:', error);
      process.exit(1);
    });
}