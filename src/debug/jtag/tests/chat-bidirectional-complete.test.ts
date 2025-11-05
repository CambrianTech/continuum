/**
 * AUTOMATED BIDIRECTIONAL MESSAGING TEST
 * 
 * Comprehensive test that validates complete chat functionality:
 * - Server-originated messages → Browser shadow DOM display
 * - Browser-side message sending → Server receipt & storage
 * - Message storage/retrieval through data/list commands
 * - Widget method deployment and functionality verification
 * - Runs continuously until complete bidirectional functionality proven
 * 
 * Based on widget testing methodology documented in CLAUDE.md
 */

import { JTAGClient } from '../system/core/client/shared/JTAGClient';
import { JTAGClientFactory } from '../tests/shared/JTAGClientFactory';
import { UUID } from '../domain/chat/ChatMessage';

interface TestResult {
  test: string;
  success: boolean;
  details: string;
  timestamp: string;
}

interface BidirectionalTestSuite {
  serverToBrowser: TestResult[];
  browserToServer: TestResult[];
  storageRetrieval: TestResult[];
  widgetMethods: TestResult[];
  overall: TestResult;
}

class ChatBidirectionalValidator {
  private jtag: JTAGClient | null = null;
  private testRoomId: string = 'test-room-bidirectional';
  private testSessionId: string;
  private factory: JTAGClientFactory;

  constructor() {
    this.factory = JTAGClientFactory.getInstance();
    this.testSessionId = `test-${Date.now()}`;
  }

  async runCompleteValidation(): Promise<BidirectionalTestSuite> {
    console.log(`🧪 AUTOMATED BIDIRECTIONAL MESSAGING TEST - Starting complete validation`);
    console.log(`🎯 Test Room: ${this.testRoomId}`);
    console.log(`📋 Session ID: ${this.testSessionId}`);

    const results: BidirectionalTestSuite = {
      serverToBrowser: [],
      browserToServer: [],
      storageRetrieval: [],
      widgetMethods: [],
      overall: { test: 'complete', success: false, details: '', timestamp: new Date().toISOString() }
    };

    try {
      // Connect to JTAG system using factory
      console.log(`🔌 Connecting to JTAG system...`);
      const connectionResult = await this.factory.createClient({
        timeout: 15000,
        validateConnection: true
      });
      this.jtag = connectionResult.client;
      this.testSessionId = connectionResult.sessionId;
      console.log(`✅ Connected to JTAG system (session: ${connectionResult.sessionId})`);

      // Phase 1: Widget Method Verification
      console.log(`\n📋 PHASE 1: Widget Method Verification`);
      results.widgetMethods = await this.validateWidgetMethods();

      // Phase 2: Server-to-Browser Message Flow
      console.log(`\n📋 PHASE 2: Server-to-Browser Message Flow`);
      results.serverToBrowser = await this.validateServerToBrowser();

      // Phase 3: Browser-to-Server Message Flow  
      console.log(`\n📋 PHASE 3: Browser-to-Server Message Flow`);
      results.browserToServer = await this.validateBrowserToServer();

      // Phase 4: Storage & Retrieval Validation
      console.log(`\n📋 PHASE 4: Storage & Retrieval Validation`);
      results.storageRetrieval = await this.validateStorageRetrieval();

      // Overall Assessment
      const allTests = [
        ...results.widgetMethods,
        ...results.serverToBrowser, 
        ...results.browserToServer,
        ...results.storageRetrieval
      ];

      const successCount = allTests.filter(t => t.success).length;
      const totalCount = allTests.length;
      const overallSuccess = successCount === totalCount;

      results.overall = {
        test: 'complete_bidirectional_validation',
        success: overallSuccess,
        details: `${successCount}/${totalCount} tests passed - ${overallSuccess ? 'COMPLETE SUCCESS' : 'PARTIAL FAILURE'}`,
        timestamp: new Date().toISOString()
      };

      this.printResults(results);
      return results;

    } catch (error) {
      results.overall = {
        test: 'complete_bidirectional_validation',
        success: false,
        details: `Critical error: ${error}`,
        timestamp: new Date().toISOString()
      };
      console.error(`❌ Critical test error:`, error);
      return results;
    }
  }

  /**
   * Phase 1: Validate widget methods exist and function properly
   */
  private async validateWidgetMethods(): Promise<TestResult[]> {
    const results: TestResult[] = [];

    // Test 1: Widget exists in DOM
    const widgetExistsTest = await this.testWidgetExists();
    results.push(widgetExistsTest);

    // Test 2: Shadow DOM accessible
    const shadowDOMTest = await this.testShadowDOMAccess();
    results.push(shadowDOMTest);

    // Test 3: Widget methods deployed
    const methodsTest = await this.testWidgetMethodsDeployed();
    results.push(methodsTest);

    return results;
  }

  private async testWidgetExists(): Promise<TestResult> {
    try {
      if (!this.jtag) throw new Error('JTAG client not connected');
      const widgetCheck = await this.jtag.commands.exec({
        code: `
          function queryShadowDOM(selector) {
            const elements = document.querySelectorAll('*');
            for (let element of elements) {
              if (element.shadowRoot) {
                const found = element.shadowRoot.querySelector(selector);
                if (found) return found;
              }
            }
            return null;
          }
          const chatWidget = queryShadowDOM('chat-widget');
          return chatWidget ? 'WIDGET_EXISTS' : 'NO_WIDGET';
        `,
        environment: 'browser'
      });

      const success = widgetCheck.result === 'WIDGET_EXISTS';
      return {
        test: 'widget_exists_in_dom',
        success,
        details: success ? 'Chat widget found in shadow DOM' : 'Chat widget NOT found in shadow DOM',
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        test: 'widget_exists_in_dom',
        success: false,
        details: `Error checking widget existence: ${error}`,
        timestamp: new Date().toISOString()
      };
    }
  }

  private async testShadowDOMAccess(): Promise<TestResult> {
    try {
      if (!this.jtag) throw new Error('JTAG client not connected');
      const shadowCheck = await this.jtag.commands.exec({
        code: `
          function queryShadowDOM(selector) {
            const elements = document.querySelectorAll('*');
            for (let element of elements) {
              if (element.shadowRoot) {
                const found = element.shadowRoot.querySelector(selector);
                if (found) return found;
              }
            }
            return null;
          }
          const widget = queryShadowDOM('chat-widget');
          if (!widget) return 'NO_WIDGET';
          if (!widget.shadowRoot) return 'NO_SHADOW_ROOT';
          const messageContainer = widget.shadowRoot.querySelector('.messages-container') || widget.shadowRoot.querySelector('.message');
          return messageContainer ? 'SHADOW_DOM_ACCESSIBLE' : 'NO_MESSAGE_CONTAINER';
        `,
        environment: 'browser'
      });

      const success = shadowCheck.result === 'SHADOW_DOM_ACCESSIBLE';
      return {
        test: 'shadow_dom_access',
        success,
        details: success ? 'Shadow DOM accessible with message container' : `Shadow DOM issue: ${shadowCheck.result}`,
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        test: 'shadow_dom_access',
        success: false,
        details: `Error accessing shadow DOM: ${error}`,
        timestamp: new Date().toISOString()
      };
    }
  }

  private async testWidgetMethodsDeployed(): Promise<TestResult> {
    try {
      if (!this.jtag) throw new Error('JTAG client not connected');
      const methodCheck = await this.jtag.commands.exec({
        code: `
          const widget = document.querySelector('chat-widget');
          if (!widget) return 'NO_WIDGET';
          
          const methods = {
            loadRoomHistory: typeof widget.loadRoomHistory === 'function',
            sendMessage: typeof widget.sendMessage === 'function',
            renderWidget: typeof widget.renderWidget === 'function'
          };
          
          return JSON.stringify(methods);
        `,
        environment: 'browser'
      });

      const methods = JSON.parse(methodCheck.result);
      const allMethodsExist = methods.loadRoomHistory && methods.sendMessage && methods.renderWidget;

      return {
        test: 'widget_methods_deployed',
        success: allMethodsExist,
        details: allMethodsExist ? 'All widget methods deployed' : `Missing methods: ${JSON.stringify(methods)}`,
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        test: 'widget_methods_deployed', 
        success: false,
        details: `Error checking widget methods: ${error}`,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Phase 2: Test server-originated messages appearing in browser shadow DOM
   */
  private async validateServerToBrowser(): Promise<TestResult[]> {
    const results: TestResult[] = [];

    // Test 1: Send message via server data/create command
    const serverSendTest = await this.testServerMessageSend();
    results.push(serverSendTest);

    // Test 2: Verify message appears in browser shadow DOM
    const browserReceiveTest = await this.testBrowserMessageReceive();
    results.push(browserReceiveTest);

    return results;
  }

  private async testServerMessageSend(): Promise<TestResult> {
    try {
      if (!this.jtag) throw new Error('JTAG client not connected');
      const messageId = `server-msg-${Date.now()}`;
      const messageContent = `Server-originated test message: ${messageId}`;

      const sendResult = await this.jtag.commands['data/create']({
        collection: 'chat_messages',
        data: {
          messageId,
          content: messageContent,
          roomId: this.testRoomId,
          senderId: 'server-test',
          senderName: 'Test Server',
          timestamp: new Date().toISOString()
        },
        sessionId: this.testSessionId
      });

      const success = sendResult.success;
      return {
        test: 'server_message_send',
        success,
        details: success ? `Server message sent: ${messageId}` : `Failed to send: ${sendResult.error}`,
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        test: 'server_message_send',
        success: false,
        details: `Error sending server message: ${error}`,
        timestamp: new Date().toISOString()
      };
    }
  }

  private async testBrowserMessageReceive(): Promise<TestResult> {
    try {
      if (!this.jtag) throw new Error('JTAG client not connected');
      // Force widget to reload messages
      await this.jtag.commands.exec({
        code: `
          (async () => {
            const widget = document.querySelector('chat-widget');
            if (widget && typeof widget.loadRoomHistory === 'function') {
              await widget.loadRoomHistory();
              await widget.renderWidget();
            }
          })();
        `,
        environment: 'browser'
      });

      // Check if server message appears in shadow DOM
      const messageCheck = await this.jtag.commands.exec({
        code: `
          const widget = document.querySelector('chat-widget');
          if (!widget || !widget.shadowRoot) return 'NO_WIDGET_OR_SHADOW';
          
          const messages = widget.shadowRoot.querySelectorAll('.message');
          const serverMessages = Array.from(messages).filter(msg => 
            msg.textContent && msg.textContent.includes('Server-originated test message')
          );
          
          return serverMessages.length > 0 ? 'SERVER_MESSAGE_DISPLAYED' : 'NO_SERVER_MESSAGE';
        `,
        environment: 'browser'
      });

      const success = messageCheck.result === 'SERVER_MESSAGE_DISPLAYED';
      return {
        test: 'browser_message_receive',
        success,
        details: success ? 'Server message displayed in browser shadow DOM' : `Message display issue: ${messageCheck.result}`,
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        test: 'browser_message_receive',
        success: false,
        details: `Error checking browser message display: ${error}`,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Phase 3: Test browser-side message sending and server receipt
   */
  private async validateBrowserToServer(): Promise<TestResult[]> {
    const results: TestResult[] = [];

    // Test 1: Send message via browser widget
    const browserSendTest = await this.testBrowserMessageSend();
    results.push(browserSendTest);

    // Test 2: Verify server receives and stores message
    const serverReceiveTest = await this.testServerMessageReceive();
    results.push(serverReceiveTest);

    return results;
  }

  private async testBrowserMessageSend(): Promise<TestResult> {
    try {
      const messageContent = `Browser-originated test message: ${Date.now()}`;

      const sendResult = await this.jtag.commands.exec({
        code: `
          (async () => {
            const widget = document.querySelector('chat-widget');
            if (!widget || !widget.shadowRoot) return 'NO_WIDGET_OR_SHADOW';
            
            const messageInput = widget.shadowRoot.querySelector('#messageInput');
            if (!messageInput) return 'NO_MESSAGE_INPUT';
            
            messageInput.value = '${messageContent}';
            
            // Trigger send message
            if (typeof widget.sendMessage === 'function') {
              await widget.sendMessage();
              return 'BROWSER_MESSAGE_SENT';
            } else {
              return 'NO_SEND_METHOD';
            }
          })();
        `,
        environment: 'browser'
      });

      const success = sendResult.result === 'BROWSER_MESSAGE_SENT';
      return {
        test: 'browser_message_send',
        success,
        details: success ? `Browser message sent: ${messageContent}` : `Send failed: ${sendResult.result}`,
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        test: 'browser_message_send',
        success: false,
        details: `Error sending browser message: ${error}`,
        timestamp: new Date().toISOString()
      };
    }
  }

  private async testServerMessageReceive(): Promise<TestResult> {
    try {
      // Wait a moment for message to be processed and stored
      await new Promise(resolve => setTimeout(resolve, 1000));

      // Check server logs or storage for the browser message
      const serverCheckResult = await this.jtag.commands['data/list']({
        collection: 'chat_messages',
        sessionId: this.testSessionId,
        limit: 10
      });

      if (!serverCheckResult.success) {
        return {
          test: 'server_message_receive',
          success: false,
          details: `Failed to retrieve messages: ${serverCheckResult.error}`,
          timestamp: new Date().toISOString()
        };
      }

      const browserMessages = serverCheckResult.items.filter((item: any) => 
        item.data && item.data.content && item.data.content.includes('Browser-originated test message')
      );

      const success = browserMessages.length > 0;
      return {
        test: 'server_message_receive',
        success,
        details: success ? 'Server received and stored browser message' : 'Browser message not found in server storage',
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        test: 'server_message_receive',
        success: false,
        details: `Error checking server message receipt: ${error}`,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Phase 4: Validate message storage and retrieval through data/list commands
   */
  private async validateStorageRetrieval(): Promise<TestResult[]> {
    const results: TestResult[] = [];

    // Test 1: Storage path consistency
    const storageTest = await this.testStoragePathConsistency();
    results.push(storageTest);

    // Test 2: Data retrieval completeness
    const retrievalTest = await this.testDataRetrievalCompleteness();
    results.push(retrievalTest);

    return results;
  }

  private async testStoragePathConsistency(): Promise<TestResult> {
    try {
      // Create a test message via data/create
      const testMessageId = `storage-test-${Date.now()}`;
      const createResult = await this.jtag.commands['data/create']({
        collection: 'chat_messages',
        data: {
          messageId: testMessageId,
          content: 'Storage path consistency test',
          roomId: this.testRoomId,
          senderId: 'test-system',
          senderName: 'Test System',
          timestamp: new Date().toISOString()
        },
        sessionId: this.testSessionId
      });

      if (!createResult.success) {
        return {
          test: 'storage_path_consistency',
          success: false,
          details: `Failed to create test message: ${createResult.error}`,
          timestamp: new Date().toISOString()
        };
      }

      // Immediately try to retrieve it via data/list  
      const listResult = await this.jtag.commands['data/list']({
        collection: 'chat_messages',
        sessionId: this.testSessionId,
        limit: 20
      });

      if (!listResult.success) {
        return {
          test: 'storage_path_consistency',
          success: false,
          details: `Failed to list messages: ${listResult.error}`,
          timestamp: new Date().toISOString()
        };
      }

      const foundMessage = listResult.items.find((item: any) => 
        item.data && item.data.messageId === testMessageId
      );

      const success = !!foundMessage;
      return {
        test: 'storage_path_consistency',
        success,
        details: success ? 'Storage and retrieval paths consistent' : 'Message created but not retrievable - path mismatch',
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        test: 'storage_path_consistency',
        success: false,
        details: `Error testing storage consistency: ${error}`,
        timestamp: new Date().toISOString()
      };
    }
  }

  private async testDataRetrievalCompleteness(): Promise<TestResult> {
    try {
      // Get all messages for test room
      const listResult = await this.jtag.commands['data/list']({
        collection: 'chat_messages',
        sessionId: this.testSessionId,
        limit: 50
      });

      if (!listResult.success) {
        return {
          test: 'data_retrieval_completeness',
          success: false,
          details: `Failed to retrieve messages: ${listResult.error}`,
          timestamp: new Date().toISOString()
        };
      }

      const roomMessages = listResult.items.filter((item: any) =>
        item.data && item.data.roomId === this.testRoomId
      );

      const messageTypes = roomMessages.map((item: any) => item.data.senderId);
      const hasServerMessage = messageTypes.includes('server-test');
      const hasBrowserMessage = messageTypes.some(id => id === 'current_user');
      const hasTestMessage = messageTypes.includes('test-system');

      const completeness = hasServerMessage && hasBrowserMessage && hasTestMessage;
      return {
        test: 'data_retrieval_completeness',
        success: completeness,
        details: completeness 
          ? `Complete message retrieval: ${roomMessages.length} messages from all sources`
          : `Incomplete retrieval - Server: ${hasServerMessage}, Browser: ${hasBrowserMessage}, Test: ${hasTestMessage}`,
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        test: 'data_retrieval_completeness',
        success: false,
        details: `Error testing retrieval completeness: ${error}`,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Print comprehensive test results
   */
  private printResults(results: BidirectionalTestSuite): void {
    console.log(`\n🎯 AUTOMATED BIDIRECTIONAL MESSAGING TEST RESULTS`);
    console.log(`=${'='.repeat(60)}`);

    console.log(`\n📋 PHASE 1: Widget Method Verification (${results.widgetMethods.length} tests)`);
    results.widgetMethods.forEach(test => {
      console.log(`${test.success ? '✅' : '❌'} ${test.test}: ${test.details}`);
    });

    console.log(`\n📋 PHASE 2: Server-to-Browser Flow (${results.serverToBrowser.length} tests)`);
    results.serverToBrowser.forEach(test => {
      console.log(`${test.success ? '✅' : '❌'} ${test.test}: ${test.details}`);
    });

    console.log(`\n📋 PHASE 3: Browser-to-Server Flow (${results.browserToServer.length} tests)`);
    results.browserToServer.forEach(test => {
      console.log(`${test.success ? '✅' : '❌'} ${test.test}: ${test.details}`);
    });

    console.log(`\n📋 PHASE 4: Storage & Retrieval (${results.storageRetrieval.length} tests)`);
    results.storageRetrieval.forEach(test => {
      console.log(`${test.success ? '✅' : '❌'} ${test.test}: ${test.details}`);
    });

    console.log(`\n🎯 OVERALL RESULT:`);
    console.log(`${results.overall.success ? '🎉' : '❌'} ${results.overall.details}`);
    
    if (results.overall.success) {
      console.log(`\n🚀 BIDIRECTIONAL MESSAGING COMPLETE! All functionality validated.`);
    } else {
      console.log(`\n🔧 BIDIRECTIONAL MESSAGING INCOMPLETE - Fix failing tests and rerun.`);
    }
  }
}

/**
 * Continuous test runner - runs until complete bidirectional functionality proven
 */
async function runContinuousValidation(): Promise<void> {
  console.log(`🔄 Starting continuous bidirectional messaging validation...`);
  
  let attempt = 1;
  let maxAttempts = 5; // Prevent infinite loops
  
  while (attempt <= maxAttempts) {
    console.log(`\n🔄 ATTEMPT ${attempt}/${maxAttempts}`);
    
    const validator = new ChatBidirectionalValidator();
    const results = await validator.runCompleteValidation();
    
    if (results.overall.success) {
      console.log(`\n🎉 SUCCESS! Complete bidirectional functionality proven on attempt ${attempt}`);
      break;
    }
    
    console.log(`\n⚠️ Attempt ${attempt} failed - ${results.overall.details}`);
    
    if (attempt === maxAttempts) {
      console.log(`\n❌ FAILED: Could not prove complete functionality after ${maxAttempts} attempts`);
      console.log(`📋 Failing tests need manual investigation and fixes`);
    } else {
      console.log(`🔄 Waiting 5 seconds before retry...`);
      await new Promise(resolve => setTimeout(resolve, 5000));
    }
    
    attempt++;
  }
}

// Run the continuous validation when script is executed directly
if (require.main === module) {
  runContinuousValidation().catch(error => {
    console.error(`💥 Critical test runner error:`, error);
    process.exit(1);
  });
}

export { ChatBidirectionalValidator, runContinuousValidation };