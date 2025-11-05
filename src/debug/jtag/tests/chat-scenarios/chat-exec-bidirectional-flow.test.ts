#!/usr/bin/env tsx
/**
 * Chat Exec Command Bidirectional Flow Tests
 * 
 * Tests the complete browser ↔ server chat flow using exec commands to:
 * - Trigger browser-side widget methods
 * - Send server-side chat commands  
 * - Verify HTML content in widget shadow DOM
 * - Validate real-time event propagation
 * - Test cross-environment message delivery
 * 
 * This approach tests the ACTUAL user experience, not just command layer.
 */

import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';

console.log('🧪 EXEC COMMAND BIDIRECTIONAL CHAT FLOW TESTS');

interface FlowTestResult {
  readonly test: string;
  readonly success: boolean;
  readonly details: string;
  readonly executionTime?: number;
  readonly htmlContent?: string;
}

interface TestResults {
  browserToServer: Array<FlowTestResult>;
  serverToBrowser: Array<FlowTestResult>;
  bidirectionalFlow: Array<FlowTestResult>;
  widgetValidation: Array<FlowTestResult>;
  eventPropagation: Array<FlowTestResult>;
}

class ExecBidirectionalChatTest {
  private client: any;
  private testRoomId: string = 'exec-bidirectional-test';
  private testUserId: string = 'exec-test-user';
  private results: TestResults = {
    browserToServer: [],
    serverToBrowser: [],
    bidirectionalFlow: [],
    widgetValidation: [],
    eventPropagation: []
  };

  async initialize(): Promise<void> {
    console.log('🔗 Connecting to JTAG system for exec bidirectional chat testing...');
    
    try {
      const result = await JTAGClientServer.connect();
      this.client = result.client;
      
      if (!result.listResult.success) {
        throw new Error('Failed to connect to JTAG system');
      }
      
      console.log(`✅ Connected to JTAG system with ${result.listResult.commands.length} commands`);
      
    } catch (error) {
      console.error('❌ Failed to initialize exec bidirectional chat test:', error);
      throw error;
    }
  }

  /**
   * TEST 1: Browser → Server Flow using exec commands
   */
  async testBrowserToServerFlow(): Promise<void> {
    console.log('\n🌐➡️🖥️ TEST 1: Browser → Server Flow');
    
    try {
      const testMessage = `EXEC_TEST_B2S_${Date.now()}`;
      const startTime = Date.now();
      
      // Use exec to trigger browser-side widget message sending
      const browserSend = await this.client.executeCommand('exec', {
        code: `
          console.log('🔍 EXEC TEST: Looking for chat widget...');
          const chatWidget = document.querySelector('chat-widget');
          
          if (!chatWidget) {
            console.log('❌ EXEC TEST: No chat widget found');
            return { success: false, error: 'Chat widget not found' };
          }
          
          console.log('✅ EXEC TEST: Chat widget found');
          
          // Try to call widget method directly
          if (typeof chatWidget.sendMessage === 'function') {
            console.log('🚀 EXEC TEST: Calling widget.sendMessage()');
            const result = await chatWidget.sendMessage('${testMessage}');
            return { success: true, method: 'widget.sendMessage', result: result };
          }
          
          // Try to simulate user typing and clicking send
          const messageInput = chatWidget.shadowRoot?.querySelector('#messageInput') || 
                              chatWidget.querySelector('#messageInput');
          const sendButton = chatWidget.shadowRoot?.querySelector('#sendButton') || 
                            chatWidget.querySelector('#sendButton');
          
          if (messageInput && sendButton) {
            console.log('📝 EXEC TEST: Simulating user input');
            messageInput.value = '${testMessage}';
            messageInput.dispatchEvent(new Event('input', { bubbles: true }));
            
            console.log('🖱️ EXEC TEST: Clicking send button');
            sendButton.click();
            
            return { success: true, method: 'ui_simulation', message: '${testMessage}' };
          }
          
          return { success: false, error: 'No input/send elements found' };
        `,
        environment: 'browser'
      });
      
      const executionTime = Date.now() - startTime;
      
      this.results.browserToServer.push({
        test: 'Browser exec command execution',
        success: browserSend.success,
        details: browserSend.success ?
          `Exec command executed in ${executionTime}ms: ${JSON.stringify(browserSend.result)}` :
          `Failed: ${browserSend.error || 'Unknown error'}`,
        executionTime,
        htmlContent: browserSend.htmlContent
      });
      
      // Wait a moment for message processing
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      // Verify server received the message using data/list
      const serverCheck = await this.client.executeCommand('data/list', {
        collection: 'chat_messages',
        limit: 10
      });
      
      let serverReceivedMessage = false;
      if (serverCheck.success && serverCheck.items) {
        serverReceivedMessage = serverCheck.items.some((item: any) => 
          item.data && item.data.content && item.data.content.includes(testMessage)
        );
      }
      
      this.results.browserToServer.push({
        test: 'Server received browser message',
        success: serverReceivedMessage,
        details: serverReceivedMessage ?
          `Server successfully received message: ${testMessage}` :
          `Server did not receive message. Found ${serverCheck.items?.length || 0} messages total`,
        executionTime: Date.now() - startTime
      });
      
    } catch (error) {
      this.results.browserToServer.push({
        test: 'Browser to server flow',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 2: Server → Browser Flow using exec validation
   */
  async testServerToBrowserFlow(): Promise<void> {
    console.log('\n🖥️➡️🌐 TEST 2: Server → Browser Flow');
    
    try {
      const testMessage = `EXEC_TEST_S2B_${Date.now()}`;
      const startTime = Date.now();
      
      // Send message from server side
      const serverSend = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.testUserId,
        content: testMessage
      });
      
      this.results.serverToBrowser.push({
        test: 'Server send message command',
        success: serverSend.success,
        details: serverSend.success ?
          `Server message sent: ${serverSend.messageId}` :
          `Failed: ${serverSend.error || 'Unknown error'}`,
        executionTime: Date.now() - startTime
      });
      
      if (serverSend.success) {
        // Wait for message to propagate
        await new Promise(resolve => setTimeout(resolve, 1500));
        
        // Use exec to check if message appears in browser widget HTML
        const browserCheck = await this.client.executeCommand('exec', {
          code: `
            console.log('🔍 EXEC TEST: Checking browser for server message...');
            
            // Check main document for message
            const bodyHtml = document.body.innerHTML;
            const bodyHasMessage = bodyHtml.includes('${testMessage}');
            console.log('📄 EXEC TEST: Body HTML contains message:', bodyHasMessage);
            
            // Check chat widget specifically
            const chatWidget = document.querySelector('chat-widget');
            let widgetHtml = '';
            let widgetHasMessage = false;
            
            if (chatWidget) {
              console.log('✅ EXEC TEST: Chat widget found');
              
              // Check shadow DOM
              if (chatWidget.shadowRoot) {
                widgetHtml = chatWidget.shadowRoot.innerHTML;
                widgetHasMessage = widgetHtml.includes('${testMessage}');
                console.log('🌒 EXEC TEST: Shadow DOM contains message:', widgetHasMessage);
              }
              
              // Check regular DOM
              const regularHtml = chatWidget.innerHTML;
              const regularHasMessage = regularHtml.includes('${testMessage}');
              console.log('📝 EXEC TEST: Regular DOM contains message:', regularHasMessage);
              
              widgetHasMessage = widgetHasMessage || regularHasMessage;
            } else {
              console.log('❌ EXEC TEST: No chat widget found');
            }
            
            return {
              bodyHasMessage,
              widgetHasMessage,
              widgetHtml: widgetHtml.substring(0, 500), // Truncate for debugging
              testMessage: '${testMessage}',
              timestamp: Date.now()
            };
          `,
          environment: 'browser'
        });
        
        const executionTime = Date.now() - startTime;
        
        this.results.serverToBrowser.push({
          test: 'Browser HTML contains server message',
          success: browserCheck.success && (browserCheck.result?.bodyHasMessage || browserCheck.result?.widgetHasMessage),
          details: browserCheck.success ?
            `Body has message: ${browserCheck.result?.bodyHasMessage}, Widget has message: ${browserCheck.result?.widgetHasMessage}` :
            `Failed to check browser HTML: ${browserCheck.error || 'Unknown error'}`,
          executionTime,
          htmlContent: browserCheck.result?.widgetHtml
        });
        
        // Also check if widget methods can retrieve the message
        const widgetMethodCheck = await this.client.executeCommand('exec', {
          code: `
            const chatWidget = document.querySelector('chat-widget');
            if (chatWidget && typeof chatWidget.loadRoomHistory === 'function') {
              console.log('🔄 EXEC TEST: Calling widget.loadRoomHistory()');
              await chatWidget.loadRoomHistory();
              
              if (chatWidget.messages && Array.isArray(chatWidget.messages)) {
                const hasMessage = chatWidget.messages.some(msg => 
                  msg.content && msg.content.includes('${testMessage}')
                );
                return { 
                  success: true, 
                  messageCount: chatWidget.messages.length,
                  hasTestMessage: hasMessage,
                  messages: chatWidget.messages.map(m => ({ content: m.content, id: m.id }))
                };
              }
            }
            return { success: false, error: 'Widget method check failed' };
          `,
          environment: 'browser'
        });
        
        this.results.serverToBrowser.push({
          test: 'Widget methods can retrieve server message',
          success: widgetMethodCheck.success && widgetMethodCheck.result?.hasTestMessage,
          details: widgetMethodCheck.success ?
            `Widget has ${widgetMethodCheck.result?.messageCount || 0} messages, test message found: ${widgetMethodCheck.result?.hasTestMessage}` :
            `Failed: ${widgetMethodCheck.error || 'Unknown error'}`
        });
      }
      
    } catch (error) {
      this.results.serverToBrowser.push({
        test: 'Server to browser flow',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 3: Full Bidirectional Flow
   */
  async testBidirectionalFlow(): Promise<void> {
    console.log('\n🔄 TEST 3: Full Bidirectional Flow');
    
    try {
      const testId = Date.now();
      const browserMessage = `BIDIRECTIONAL_BROWSER_${testId}`;
      const serverMessage = `BIDIRECTIONAL_SERVER_${testId}`;
      const startTime = Date.now();
      
      // Step 1: Browser sends message
      const browserSend = await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          if (!chatWidget) return { success: false, error: 'No widget' };
          
          // Simulate user sending message
          const messageInput = chatWidget.shadowRoot?.querySelector('#messageInput') || 
                              chatWidget.querySelector('#messageInput');
          const sendButton = chatWidget.shadowRoot?.querySelector('#sendButton') || 
                            chatWidget.querySelector('#sendButton');
          
          if (messageInput && sendButton) {
            messageInput.value = '${browserMessage}';
            sendButton.click();
            return { success: true, sent: '${browserMessage}' };
          }
          
          return { success: false, error: 'No input elements' };
        `,
        environment: 'browser'
      });
      
      // Step 2: Server sends response message
      await new Promise(resolve => setTimeout(resolve, 500));
      const serverSend = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: 'server-responder',
        content: serverMessage
      });
      
      // Step 3: Verify both messages appear in browser
      await new Promise(resolve => setTimeout(resolve, 1000));
      const finalCheck = await this.client.executeCommand('exec', {
        code: `
          const chatWidget = document.querySelector('chat-widget');
          if (!chatWidget) return { success: false, error: 'No widget' };
          
          // Refresh widget data
          if (typeof chatWidget.loadRoomHistory === 'function') {
            await chatWidget.loadRoomHistory();
          }
          
          let allHtml = '';
          if (chatWidget.shadowRoot) {
            allHtml += chatWidget.shadowRoot.innerHTML;
          }
          allHtml += chatWidget.innerHTML + document.body.innerHTML;
          
          const hasBrowserMessage = allHtml.includes('${browserMessage}');
          const hasServerMessage = allHtml.includes('${serverMessage}');
          
          return {
            success: hasBrowserMessage && hasServerMessage,
            hasBrowserMessage,
            hasServerMessage,
            widgetMessages: chatWidget.messages ? chatWidget.messages.length : 0
          };
        `,
        environment: 'browser'
      });
      
      const executionTime = Date.now() - startTime;
      
      this.results.bidirectionalFlow.push({
        test: 'Browser message sent',
        success: browserSend.success,
        details: browserSend.success ? 
          `Browser message sent: ${browserMessage}` :
          `Failed: ${browserSend.error || 'Unknown error'}`
      });
      
      this.results.bidirectionalFlow.push({
        test: 'Server message sent',
        success: serverSend.success,
        details: serverSend.success ?
          `Server message sent: ${serverMessage}` :
          `Failed: ${serverSend.error || 'Unknown error'}`
      });
      
      this.results.bidirectionalFlow.push({
        test: 'Both messages visible in browser',
        success: finalCheck.success && finalCheck.result?.success,
        details: finalCheck.success ?
          `Browser msg: ${finalCheck.result?.hasBrowserMessage}, Server msg: ${finalCheck.result?.hasServerMessage}, Widget msgs: ${finalCheck.result?.widgetMessages}` :
          `Failed: ${finalCheck.error || 'Unknown error'}`,
        executionTime
      });
      
    } catch (error) {
      this.results.bidirectionalFlow.push({
        test: 'Bidirectional flow test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 4: Widget HTML Content Validation
   */
  async testWidgetValidation(): Promise<void> {
    console.log('\n🔍 TEST 4: Widget HTML Content Validation');
    
    try {
      // Send a styled message for validation
      const styledMessage = `STYLED_TEST_${Date.now()}`;
      await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: 'style-tester',
        content: styledMessage
      });
      
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      // Comprehensive widget validation
      const widgetAnalysis = await this.client.executeCommand('exec', {
        code: `
          console.log('🔍 WIDGET VALIDATION: Starting comprehensive analysis...');
          
          const analysis = {
            widgetExists: false,
            hasShadowRoot: false,
            hasInput: false,
            hasSendButton: false,
            hasMessages: false,
            hasTestMessage: false,
            widgetMethods: [],
            htmlStructure: '',
            cssStyles: '',
            eventListeners: false
          };
          
          const chatWidget = document.querySelector('chat-widget');
          if (chatWidget) {
            analysis.widgetExists = true;
            console.log('✅ WIDGET: Chat widget found');
            
            // Check shadow DOM
            if (chatWidget.shadowRoot) {
              analysis.hasShadowRoot = true;
              analysis.htmlStructure = chatWidget.shadowRoot.innerHTML;
              
              // Check for input elements
              const input = chatWidget.shadowRoot.querySelector('#messageInput') || 
                          chatWidget.shadowRoot.querySelector('input[type="text"]');
              analysis.hasInput = !!input;
              
              const button = chatWidget.shadowRoot.querySelector('#sendButton') || 
                           chatWidget.shadowRoot.querySelector('button');
              analysis.hasSendButton = !!button;
              
              // Check for messages
              const messageElements = chatWidget.shadowRoot.querySelectorAll('.message') || 
                                    chatWidget.shadowRoot.querySelectorAll('[class*="message"]');
              analysis.hasMessages = messageElements.length > 0;
              
              // Check for test message
              analysis.hasTestMessage = analysis.htmlStructure.includes('${styledMessage}');
              
              console.log('🌒 WIDGET: Shadow DOM analyzed');
            }
            
            // Check widget methods
            const methods = Object.getOwnPropertyNames(Object.getPrototypeOf(chatWidget))
              .filter(name => typeof chatWidget[name] === 'function');
            analysis.widgetMethods = methods;
            
            // Check for event listeners (basic check)
            analysis.eventListeners = methods.some(m => 
              m.includes('Event') || m.includes('Listener') || m.includes('Handler')
            );
          }
          
          return analysis;
        `,
        environment: 'browser'
      });
      
      this.results.widgetValidation.push({
        test: 'Widget exists and is functional',
        success: widgetAnalysis.success && widgetAnalysis.result?.widgetExists,
        details: widgetAnalysis.success ?
          `Widget found: ${widgetAnalysis.result?.widgetExists}` :
          `Failed: ${widgetAnalysis.error || 'Unknown error'}`
      });
      
      if (widgetAnalysis.success && widgetAnalysis.result) {
        const result = widgetAnalysis.result;
        
        this.results.widgetValidation.push({
          test: 'Widget has shadow DOM',
          success: result.hasShadowRoot,
          details: `Shadow DOM: ${result.hasShadowRoot}`
        });
        
        this.results.widgetValidation.push({
          test: 'Widget has input elements',
          success: result.hasInput && result.hasSendButton,
          details: `Input: ${result.hasInput}, Send button: ${result.hasSendButton}`
        });
        
        this.results.widgetValidation.push({
          test: 'Widget displays messages',
          success: result.hasMessages,
          details: `Has messages: ${result.hasMessages}`
        });
        
        this.results.widgetValidation.push({
          test: 'Widget shows test message',
          success: result.hasTestMessage,
          details: `Test message visible: ${result.hasTestMessage}`,
          htmlContent: result.htmlStructure?.substring(0, 500)
        });
        
        this.results.widgetValidation.push({
          test: 'Widget has proper methods',
          success: result.widgetMethods.length > 5,
          details: `Methods: ${result.widgetMethods.join(', ')}`
        });
      }
      
    } catch (error) {
      this.results.widgetValidation.push({
        test: 'Widget validation test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 5: Event Propagation Testing
   */
  async testEventPropagation(): Promise<void> {
    console.log('\n⚡ TEST 5: Event Propagation Testing');
    
    try {
      const eventMessage = `EVENT_TEST_${Date.now()}`;
      
      // Set up event listener in browser
      const setupListener = await this.client.executeCommand('exec', {
        code: `
          console.log('🎧 EVENT TEST: Setting up event listeners...');
          
          window.testEventResults = window.testEventResults || {};
          
          // Listen for custom chat events
          document.addEventListener('chat:message-received', (event) => {
            console.log('📨 EVENT: Received chat:message-received', event.detail);
            window.testEventResults.messageReceived = event.detail;
          });
          
          document.addEventListener('chat:message-sent', (event) => {
            console.log('📤 EVENT: Received chat:message-sent', event.detail);
            window.testEventResults.messageSent = event.detail;
          });
          
          // Check if widget has event subscription methods
          const chatWidget = document.querySelector('chat-widget');
          let hasEventMethods = false;
          if (chatWidget) {
            const methods = Object.getOwnPropertyNames(Object.getPrototypeOf(chatWidget));
            hasEventMethods = methods.some(m => 
              m.includes('Event') || m.includes('subscribe') || m.includes('listen')
            );
          }
          
          return { 
            success: true, 
            listenersSetup: true,
            widgetHasEventMethods: hasEventMethods,
            widgetMethods: chatWidget ? Object.getOwnPropertyNames(Object.getPrototypeOf(chatWidget)) : []
          };
        `,
        environment: 'browser'
      });
      
      this.results.eventPropagation.push({
        test: 'Event listeners setup',
        success: setupListener.success,
        details: setupListener.success ?
          `Listeners setup: ${setupListener.result?.listenersSetup}, Widget event methods: ${setupListener.result?.widgetHasEventMethods}` :
          `Failed: ${setupListener.error || 'Unknown error'}`
      });
      
      // Send message to trigger events
      const triggerEvent = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: 'event-trigger',
        content: eventMessage
      });
      
      this.results.eventPropagation.push({
        test: 'Message sent to trigger events',
        success: triggerEvent.success,
        details: triggerEvent.success ?
          `Event trigger message sent: ${eventMessage}` :
          `Failed: ${triggerEvent.error || 'Unknown error'}`
      });
      
      // Wait for event propagation
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      // Check if events were received
      const checkEvents = await this.client.executeCommand('exec', {
        code: `
          console.log('🔍 EVENT TEST: Checking received events...');
          
          const results = window.testEventResults || {};
          const hasMessageReceived = !!results.messageReceived;
          const hasMessageSent = !!results.messageSent;
          
          // Check if widget has updated in response to events
          const chatWidget = document.querySelector('chat-widget');
          let widgetUpdated = false;
          if (chatWidget) {
            const widgetHtml = chatWidget.shadowRoot ? 
              chatWidget.shadowRoot.innerHTML : chatWidget.innerHTML;
            widgetUpdated = widgetHtml.includes('${eventMessage}');
          }
          
          console.log('📊 EVENT RESULTS:', { hasMessageReceived, hasMessageSent, widgetUpdated });
          
          return {
            hasMessageReceived,
            hasMessageSent,
            widgetUpdated,
            eventDetails: results,
            testMessage: '${eventMessage}'
          };
        `,
        environment: 'browser'
      });
      
      this.results.eventPropagation.push({
        test: 'Events properly propagated',
        success: checkEvents.success && (checkEvents.result?.hasMessageReceived || checkEvents.result?.widgetUpdated),
        details: checkEvents.success ?
          `Msg received event: ${checkEvents.result?.hasMessageReceived}, Widget updated: ${checkEvents.result?.widgetUpdated}` :
          `Failed: ${checkEvents.error || 'Unknown error'}`
      });
      
      this.results.eventPropagation.push({
        test: 'Widget responds to events',
        success: checkEvents.success && checkEvents.result?.widgetUpdated,
        details: checkEvents.success ?
          `Widget updated in response to events: ${checkEvents.result?.widgetUpdated}` :
          `Failed: ${checkEvents.error || 'Unknown error'}`
      });
      
    } catch (error) {
      this.results.eventPropagation.push({
        test: 'Event propagation test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * Run all exec bidirectional tests
   */
  async runAllTests(): Promise<void> {
    await this.initialize();
    
    await this.testBrowserToServerFlow();
    await this.testServerToBrowserFlow();
    await this.testBidirectionalFlow();
    await this.testWidgetValidation();
    await this.testEventPropagation();
  }

  /**
   * Display comprehensive test results
   */
  displayResults(): void {
    console.log(`\n🎯 EXEC COMMAND BIDIRECTIONAL CHAT FLOW TEST RESULTS`);
    console.log('====================================================');
    
    const categories = [
      { name: 'Browser → Server', tests: this.results.browserToServer, icon: '🌐➡️🖥️' },
      { name: 'Server → Browser', tests: this.results.serverToBrowser, icon: '🖥️➡️🌐' },
      { name: 'Bidirectional Flow', tests: this.results.bidirectionalFlow, icon: '🔄' },
      { name: 'Widget Validation', tests: this.results.widgetValidation, icon: '🔍' },
      { name: 'Event Propagation', tests: this.results.eventPropagation, icon: '⚡' }
    ];
    
    let totalTests = 0;
    let totalPassed = 0;
    
    categories.forEach(category => {
      const passed = category.tests.filter(test => test.success).length;
      const total = category.tests.length;
      totalTests += total;
      totalPassed += passed;
      
      console.log(`\n${category.icon} ${category.name.toUpperCase()}: ${passed}/${total} tests passed`);
      category.tests.forEach(test => {
        const status = test.success ? '✅' : '❌';
        const timing = test.executionTime ? ` (${test.executionTime}ms)` : '';
        console.log(`  ${status} ${test.test}: ${test.details}${timing}`);
        
        // Show HTML content if available and test failed
        if (!test.success && test.htmlContent) {
          console.log(`    🔍 HTML: ${test.htmlContent.substring(0, 200)}...`);
        }
      });
    });
    
    console.log(`\n📊 OVERALL SUMMARY: ${totalPassed}/${totalTests} exec flow tests passed`);
    console.log(`Success Rate: ${((totalPassed / totalTests) * 100).toFixed(1)}%`);
    
    if (totalPassed === totalTests) {
      console.log('🎉 ✅ EXEC COMMAND BIDIRECTIONAL FLOW WORKING PERFECTLY!');
      console.log('🔄 Complete browser ↔ server integration validated');
      console.log('🌐 Widget HTML updates properly from both directions');
      console.log('⚡ Real-time events propagate correctly to UI');
      console.log('🚀 Ready for production Discord-scale chat system');
    } else {
      console.log('⚠️  Exec bidirectional flow has integration gaps');
      console.log('🔧 Focus on failing tests to achieve full integration');
      console.log('📋 These tests reveal actual user experience issues');
      console.log('🎯 Fix browser ↔ server integration for real chat functionality');
      
      // Show critical gaps
      const criticalGaps = [];
      if (this.results.browserToServer.every(test => !test.success)) {
        criticalGaps.push('Browser → Server flow completely broken');
      }
      if (this.results.serverToBrowser.every(test => !test.success)) {
        criticalGaps.push('Server → Browser flow completely broken');
      }
      if (this.results.widgetValidation.every(test => !test.success)) {
        criticalGaps.push('Widget HTML integration missing');
      }
      
      if (criticalGaps.length > 0) {
        console.log('\n🚨 CRITICAL INTEGRATION GAPS:');
        criticalGaps.forEach(gap => console.log(`   ❌ ${gap}`));
      }
    }
  }
}

// Main execution
async function runExecBidirectionalChatTests(): Promise<void> {
  const testRunner = new ExecBidirectionalChatTest();
  
  try {
    await testRunner.runAllTests();
    testRunner.displayResults();
    
  } catch (error) {
    console.error('💥 Exec bidirectional chat test execution failed:', error);
    process.exit(1);
  }
}

// Run tests if this file is executed directly
if (require.main === module) {
  runExecBidirectionalChatTests().catch(error => {
    console.error('💥 Test runner failed:', error);
    process.exit(1);
  });
}

export { runExecBidirectionalChatTests, ExecBidirectionalChatTest };