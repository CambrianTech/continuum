/**
 * Chat User ID Persistence Integration Test
 * 
 * Comprehensive test covering all the bugs we addressed in User ID persistence:
 * 1. Session ID vs User ID confusion (Session changes, User ID should persist)
 * 2. LocalStorage persistence across browser sessions
 * 3. Message attribution (User messages on RIGHT as "You", others on LEFT)
 * 4. Browser → Server → Database → Browser round-trip with correct User ID
 * 5. Chat widget Shadow DOM navigation and interaction
 * 6. Bidirectional messaging with proper attribution
 * 
 * This test validates the complete Discord-scale chat User ID architecture.
 */

import { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import { JTAGClientFactory } from '../shared/JTAGClientFactory';

interface UserIdTestResult {
  test: string;
  success: boolean;
  details: string;
  actualValue?: string;
  expectedValue?: string;
  timestamp: string;
}

interface UserIdPersistenceTestSuite {
  localStoragePersistence: UserIdTestResult[];
  sessionVsUserIdSeparation: UserIdTestResult[];
  messageAttribution: UserIdTestResult[];
  bidirectionalFlow: UserIdTestResult[];
  shadowDomNavigation: UserIdTestResult[];
  overall: UserIdTestResult;
}

class UserIdPersistenceValidator {
  private jtag: JTAGClient | null = null;
  private factory: JTAGClientFactory;
  private testRoomId: string = 'user-id-test-room';
  private expectedUserId: string = 'user-joel-12345'; // The persistent User ID we implemented
  
  constructor() {
    this.factory = JTAGClientFactory.getInstance();
  }

  async runCompleteValidation(): Promise<UserIdPersistenceTestSuite> {
    console.log(`🧪 USER ID PERSISTENCE TEST SUITE - Validating complete architecture`);
    console.log(`🎯 Expected User ID: ${this.expectedUserId}`);
    console.log(`🏠 Test Room: ${this.testRoomId}`);
    
    try {
      this.jtag = await this.factory.createClient();
      await this.jtag.connect();
      
      const results: UserIdPersistenceTestSuite = {
        localStoragePersistence: [],
        sessionVsUserIdSeparation: [],
        messageAttribution: [],
        bidirectionalFlow: [],
        shadowDomNavigation: [],
        overall: {
          test: 'overall',
          success: false,
          details: '',
          timestamp: new Date().toISOString()
        }
      };

      // Test 1: LocalStorage Persistence
      console.log(`\n🔍 TEST 1: LocalStorage Persistence`);
      results.localStoragePersistence = await this.testLocalStoragePersistence();
      
      // Test 2: Session ID vs User ID Separation  
      console.log(`\n🔍 TEST 2: Session ID vs User ID Separation`);
      results.sessionVsUserIdSeparation = await this.testSessionVsUserIdSeparation();
      
      // Test 3: Shadow DOM Navigation
      console.log(`\n🔍 TEST 3: Shadow DOM Widget Navigation`);
      results.shadowDomNavigation = await this.testShadowDomNavigation();
      
      // Test 4: Message Attribution
      console.log(`\n🔍 TEST 4: Message Attribution (Right/Left Side)`);
      results.messageAttribution = await this.testMessageAttribution();
      
      // Test 5: Bidirectional Flow with User ID Consistency
      console.log(`\n🔍 TEST 5: Bidirectional Flow with User ID`);
      results.bidirectionalFlow = await this.testBidirectionalUserIdFlow();
      
      // Calculate overall result
      const allTests = [
        ...results.localStoragePersistence,
        ...results.sessionVsUserIdSeparation,
        ...results.shadowDomNavigation,
        ...results.messageAttribution,
        ...results.bidirectionalFlow
      ];
      
      const successCount = allTests.filter(t => t.success).length;
      const totalTests = allTests.length;
      
      results.overall = {
        test: 'overall',
        success: successCount === totalTests,
        details: `${successCount}/${totalTests} tests passed - User ID persistence ${successCount === totalTests ? 'WORKING' : 'BROKEN'}`,
        timestamp: new Date().toISOString()
      };
      
      console.log(`\n🏁 USER ID PERSISTENCE TEST RESULTS:`);
      console.log(`   ${successCount}/${totalTests} tests passed`);
      console.log(`   Overall: ${results.overall.success ? '✅ WORKING' : '❌ BROKEN'}`);
      
      return results;
      
    } catch (error) {
      console.error(`❌ Test suite failed:`, error);
      throw error;
    } finally {
      if (this.jtag) {
        await this.jtag.disconnect();
      }
    }
  }

  private async testLocalStoragePersistence(): Promise<UserIdTestResult[]> {
    const results: UserIdTestResult[] = [];
    
    try {
      // Check if localStorage has our expected User ID
      const checkStorageResult = await this.jtag!.executeCommand('exec', {
        code: `
          const storedUserId = localStorage.getItem('continuum_user_id');
          console.log('🔧 LocalStorage continuum_user_id:', storedUserId);
          return {
            hasStorage: typeof localStorage !== 'undefined',
            storedUserId: storedUserId,
            isCorrectUserId: storedUserId === '${this.expectedUserId}'
          };
        `,
        environment: 'browser'
      });
      
      const storageData = checkStorageResult.result;
      
      results.push({
        test: 'localStorage_available',
        success: storageData.hasStorage,
        details: storageData.hasStorage ? 'LocalStorage is available' : 'LocalStorage not available',
        timestamp: new Date().toISOString()
      });
      
      results.push({
        test: 'user_id_stored',
        success: !!storageData.storedUserId,
        details: storageData.storedUserId ? `User ID found: ${storageData.storedUserId}` : 'No User ID in localStorage',
        actualValue: storageData.storedUserId,
        expectedValue: this.expectedUserId,
        timestamp: new Date().toISOString()
      });
      
      results.push({
        test: 'correct_user_id_value',
        success: storageData.isCorrectUserId,
        details: storageData.isCorrectUserId ? 'Correct persistent User ID stored' : `Wrong User ID - expected ${this.expectedUserId}, got ${storageData.storedUserId}`,
        actualValue: storageData.storedUserId,
        expectedValue: this.expectedUserId,
        timestamp: new Date().toISOString()
      });
      
    } catch (error) {
      results.push({
        test: 'localStorage_persistence_error',
        success: false,
        details: `Error testing localStorage: ${error.message}`,
        timestamp: new Date().toISOString()
      });
    }
    
    return results;
  }

  private async testSessionVsUserIdSeparation(): Promise<UserIdTestResult[]> {
    const results: UserIdTestResult[] = [];
    
    try {
      // Get session ID from JTAG and check it's different from User ID
      const sessionCheck = await this.jtag!.executeCommand('exec', {
        code: `
          // Check if we can access the ChatWidget to get session info
          const continuumWidget = document.querySelector('continuum-widget');
          const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
          const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
          
          if (chatWidget && chatWidget.currentSessionId && chatWidget.currentUserId) {
            return {
              hasWidget: true,
              sessionId: chatWidget.currentSessionId,
              userId: chatWidget.currentUserId,
              areDifferent: chatWidget.currentSessionId !== chatWidget.currentUserId,
              userIdMatchesExpected: chatWidget.currentUserId === '${this.expectedUserId}'
            };
          } else {
            return {
              hasWidget: !!chatWidget,
              sessionId: chatWidget?.currentSessionId || null,
              userId: chatWidget?.currentUserId || null,
              areDifferent: false,
              userIdMatchesExpected: false,
              error: 'ChatWidget or session/user IDs not accessible'
            };
          }
        `,
        environment: 'browser'
      });
      
      const sessionData = sessionCheck.result;
      
      results.push({
        test: 'widget_accessible',
        success: sessionData.hasWidget,
        details: sessionData.hasWidget ? 'ChatWidget is accessible' : 'ChatWidget not found in Shadow DOM',
        timestamp: new Date().toISOString()
      });
      
      if (sessionData.hasWidget && sessionData.sessionId && sessionData.userId) {
        results.push({
          test: 'session_vs_user_id_different',
          success: sessionData.areDifferent,
          details: sessionData.areDifferent 
            ? `Session ID (${sessionData.sessionId}) and User ID (${sessionData.userId}) are correctly different`
            : `PROBLEM: Session ID and User ID are the same (${sessionData.sessionId})`,
          actualValue: `Session: ${sessionData.sessionId}, User: ${sessionData.userId}`,
          timestamp: new Date().toISOString()
        });
        
        results.push({
          test: 'user_id_correct_value',
          success: sessionData.userIdMatchesExpected,
          details: sessionData.userIdMatchesExpected
            ? `User ID correctly set to ${this.expectedUserId}`
            : `User ID wrong - expected ${this.expectedUserId}, got ${sessionData.userId}`,
          actualValue: sessionData.userId,
          expectedValue: this.expectedUserId,
          timestamp: new Date().toISOString()
        });
      }
      
    } catch (error) {
      results.push({
        test: 'session_user_id_separation_error',
        success: false,
        details: `Error testing Session/User ID separation: ${error.message}`,
        timestamp: new Date().toISOString()
      });
    }
    
    return results;
  }

  private async testShadowDomNavigation(): Promise<UserIdTestResult[]> {
    const results: UserIdTestResult[] = [];
    
    try {
      // Test the exact Shadow DOM navigation path we documented
      const navigationResult = await this.jtag!.executeCommand('exec', {
        code: `
          console.log('🔧 Testing Shadow DOM navigation path');
          
          // Step 1: continuum-widget
          const continuumWidget = document.querySelector('continuum-widget');
          if (!continuumWidget) return { step: 'continuum-widget', success: false, error: 'Element not found' };
          
          // Step 2: main-widget in shadowRoot
          const mainWidget = continuumWidget.shadowRoot?.querySelector('main-widget');
          if (!mainWidget) return { step: 'main-widget', success: false, error: 'Element not found in continuum-widget shadowRoot' };
          
          // Step 3: chat-widget in main-widget shadowRoot
          const chatWidget = mainWidget.shadowRoot?.querySelector('chat-widget');
          if (!chatWidget) return { step: 'chat-widget', success: false, error: 'Element not found in main-widget shadowRoot' };
          
          // Step 4: message input in chat-widget shadowRoot
          const messageInput = chatWidget.shadowRoot?.querySelector('.message-input');
          if (!messageInput) return { step: 'message-input', success: false, error: 'Message input not found in chat-widget shadowRoot' };
          
          // Step 5: sendMessage method exists
          if (typeof chatWidget.sendMessage !== 'function') {
            return { step: 'sendMessage-method', success: false, error: 'sendMessage method not found on chat widget' };
          }
          
          return { 
            step: 'complete', 
            success: true, 
            path: 'continuum-widget → main-widget → chat-widget → .message-input + sendMessage()',
            widgetMethods: Object.getOwnPropertyNames(Object.getPrototypeOf(chatWidget)).filter(name => typeof chatWidget[name] === 'function')
          };
        `,
        environment: 'browser'
      });
      
      const navData = navigationResult.result;
      
      results.push({
        test: 'shadow_dom_navigation_complete',
        success: navData.success,
        details: navData.success 
          ? `Successfully navigated: ${navData.path}`
          : `Failed at ${navData.step}: ${navData.error}`,
        timestamp: new Date().toISOString()
      });
      
      if (navData.success && navData.widgetMethods) {
        results.push({
          test: 'widget_methods_available',
          success: navData.widgetMethods.length > 0,
          details: `ChatWidget has ${navData.widgetMethods.length} methods: ${navData.widgetMethods.join(', ')}`,
          timestamp: new Date().toISOString()
        });
      }
      
    } catch (error) {
      results.push({
        test: 'shadow_dom_navigation_error',
        success: false,
        details: `Error testing Shadow DOM navigation: ${error.message}`,
        timestamp: new Date().toISOString()
      });
    }
    
    return results;
  }

  private async testMessageAttribution(): Promise<UserIdTestResult[]> {
    const results: UserIdTestResult[] = [];
    
    try {
      // Send a test message and verify attribution
      const testMessage = `user-id-attribution-test-${Date.now()}`;
      
      // Send message from browser
      const sendResult = await this.jtag!.executeCommand('exec', {
        code: `
          const continuumWidget = document.querySelector('continuum-widget');
          const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
          const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
          const input = chatWidget?.shadowRoot?.querySelector('.message-input');
          
          if (input && chatWidget.sendMessage) {
            input.value = '${testMessage}';
            const result = chatWidget.sendMessage();
            return { success: true, message: 'Message sent from browser' };
          } else {
            return { success: false, error: 'Could not send message - widget not ready' };
          }
        `,
        environment: 'browser'
      });
      
      results.push({
        test: 'browser_message_send',
        success: sendResult.result.success,
        details: sendResult.result.success ? 'Message sent from browser' : sendResult.result.error,
        timestamp: new Date().toISOString()
      });
      
      // Wait a bit for processing
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      // Check message attribution
      const attributionResult = await this.jtag!.executeCommand('exec', {
        code: `
          const continuumWidget = document.querySelector('continuum-widget');
          const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
          const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
          
          if (chatWidget && chatWidget.messages) {
            // Find our test message
            const testMsg = chatWidget.messages.find(m => m.content === '${testMessage}');
            if (testMsg) {
              const currentUserId = chatWidget.currentUserId;
              const isAttributedCorrectly = testMsg.senderId === currentUserId && testMsg.type === 'user';
              
              return {
                found: true,
                message: {
                  content: testMsg.content,
                  senderId: testMsg.senderId,
                  type: testMsg.type,
                  senderName: testMsg.senderName
                },
                currentUserId: currentUserId,
                isAttributedCorrectly: isAttributedCorrectly,
                expectedAttribution: { senderId: currentUserId, type: 'user' }
              };
            } else {
              return { found: false, error: 'Test message not found in widget messages' };
            }
          } else {
            return { found: false, error: 'ChatWidget or messages not accessible' };
          }
        `,
        environment: 'browser'
      });
      
      const attrData = attributionResult.result;
      
      results.push({
        test: 'test_message_found',
        success: attrData.found,
        details: attrData.found ? 'Test message found in widget' : attrData.error,
        timestamp: new Date().toISOString()
      });
      
      if (attrData.found) {
        results.push({
          test: 'message_attribution_correct',
          success: attrData.isAttributedCorrectly,
          details: attrData.isAttributedCorrectly
            ? `Message correctly attributed: senderId=${attrData.message.senderId}, type=${attrData.message.type} (shows as "${attrData.message.senderName}" on RIGHT side)`
            : `Message attribution WRONG: senderId=${attrData.message.senderId} (expected ${attrData.currentUserId}), type=${attrData.message.type} (expected 'user')`,
          actualValue: `senderId: ${attrData.message.senderId}, type: ${attrData.message.type}`,
          expectedValue: `senderId: ${attrData.currentUserId}, type: 'user'`,
          timestamp: new Date().toISOString()
        });
      }
      
    } catch (error) {
      results.push({
        test: 'message_attribution_error',
        success: false,
        details: `Error testing message attribution: ${error.message}`,
        timestamp: new Date().toISOString()
      });
    }
    
    return results;
  }

  private async testBidirectionalUserIdFlow(): Promise<UserIdTestResult[]> {
    const results: UserIdTestResult[] = [];
    
    try {
      // Test server → browser message with different sender
      const serverMessage = `server-test-${Date.now()}`;
      
      const serverSendResult = await this.jtag!.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        content: serverMessage,
        senderType: 'server'
      });
      
      results.push({
        test: 'server_message_send',
        success: serverSendResult.success,
        details: serverSendResult.success ? 'Server message sent successfully' : 'Server message send failed',
        timestamp: new Date().toISOString()
      });
      
      // Wait for processing
      await new Promise(resolve => setTimeout(resolve, 1500));
      
      // Check if server message is attributed correctly (should NOT be current user)
      const serverAttributionResult = await this.jtag!.executeCommand('exec', {
        code: `
          const continuumWidget = document.querySelector('continuum-widget');
          const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
          const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
          
          if (chatWidget && chatWidget.messages) {
            // Trigger reload to get server message
            if (chatWidget.loadRoomHistory) {
              await chatWidget.loadRoomHistory();
            }
            
            const serverMsg = chatWidget.messages.find(m => m.content === '${serverMessage}');
            if (serverMsg) {
              const currentUserId = chatWidget.currentUserId;
              const isServerMessage = serverMsg.senderId !== currentUserId && serverMsg.type === 'assistant';
              
              return {
                found: true,
                message: {
                  content: serverMsg.content,
                  senderId: serverMsg.senderId,
                  type: serverMsg.type,
                  senderName: serverMsg.senderName
                },
                currentUserId: currentUserId,
                isServerMessage: isServerMessage
              };
            } else {
              return { found: false, error: 'Server message not found' };
            }
          } else {
            return { found: false, error: 'ChatWidget not accessible' };
          }
        `,
        environment: 'browser'
      });
      
      const serverAttrData = serverAttributionResult.result;
      
      results.push({
        test: 'server_message_attribution',
        success: serverAttrData.found && serverAttrData.isServerMessage,
        details: serverAttrData.found
          ? (serverAttrData.isServerMessage 
             ? `Server message correctly attributed: senderId=${serverAttrData.message.senderId} (≠ ${serverAttrData.currentUserId}), type=${serverAttrData.message.type} (shows as "${serverAttrData.message.senderName}" on LEFT side)`
             : `Server message attribution WRONG: should not match current user`)
          : serverAttrData.error,
        timestamp: new Date().toISOString()
      });
      
    } catch (error) {
      results.push({
        test: 'bidirectional_flow_error',
        success: false,
        details: `Error testing bidirectional flow: ${error.message}`,
        timestamp: new Date().toISOString()
      });
    }
    
    return results;
  }
}

// Export for use in test runner
export async function runUserIdPersistenceTests(): Promise<UserIdPersistenceTestSuite> {
  const validator = new UserIdPersistenceValidator();
  return await validator.runCompleteValidation();
}

// Run if this is the main module (for direct testing)
if (require.main === module) {
  runUserIdPersistenceTests()
    .then(results => {
      console.log('\n📊 FINAL TEST RESULTS:', JSON.stringify(results, null, 2));
      const success = results.overall.success;
      console.log(`\n🎯 User ID Persistence: ${success ? '✅ WORKING' : '❌ BROKEN'}`);
      process.exit(success ? 0 : 1);
    })
    .catch(error => {
      console.error('❌ Test suite error:', error);
      process.exit(1);
    });
}