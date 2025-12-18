#!/usr/bin/env tsx
/**
 * Chat ME vs OTHER Positioning Test
 *
 * Tests the fundamental semantic positioning logic:
 * - Messages from current user appear as "ME" (right-aligned)
 * - Messages from other users appear as "OTHER" (left-aligned)
 * - Regardless of message entity type (human, AI, agent, persona)
 *
 * This validates the fix for the bug where CSS classes incorrectly used
 * message.type instead of senderId === currentUserId comparison.
 */

import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';
import { TestUserManager } from '../shared/TestUserManager';
import { BrowserTestSession } from '../shared/BrowserTestSession';

console.log('🧪 CHAT ME vs OTHER POSITIONING TEST');

interface PositioningTestResult {
  readonly test: string;
  readonly success: boolean;
  readonly details: string;
  readonly currentUserId?: string;
  readonly messageUserId?: string;
  readonly cssClasses?: string;
  readonly alignment?: string;
  readonly screenshot?: string;
}

class ChatMeOtherPositioningTest {
  private userManager: TestUserManager;
  private browserSession: BrowserTestSession | null = null;
  private testRoomId: string = 'positioning-test-room';
  private results: PositioningTestResult[] = [];

  constructor() {
    this.userManager = new TestUserManager();
  }

  async initialize(): Promise<void> {
    console.log('🔗 Initializing ME/OTHER positioning test...');

    try {
      // Connect multiple user types for positioning test
      await this.userManager.connectStandardUsers();

      const controlUser = this.userManager.getUser('Human');
      if (!controlUser) {
        throw new Error('Control user not found');
      }

      // Setup browser test session for visual verification
      this.browserSession = new BrowserTestSession({
        controllingClient: controlUser.client,
        sessionName: 'chat-me-other-positioning',
        screenshotPrefix: 'positioning'
      });

      console.log('✅ ME/OTHER positioning test initialized');

    } catch (error) {
      console.error('❌ Failed to initialize positioning test:', error);
      throw error;
    }
  }

  /**
   * TEST 1: Same Entity Type, Different Users
   * Test that multiple humans show correct ME vs OTHER positioning
   */
  async testSameEntityTypeDifferentUsers(): Promise<void> {
    console.log('\n👥 TEST 1: Same Entity Type (Human), Different Users');

    try {
      // Clear any existing messages
      await this.clearTestRoom();

      // Create two human users with different IDs
      const human1 = this.userManager.getUser('Human');
      const human2 = await this.createTestUser('Human2', 'human');

      if (!human1 || !human2 || !this.browserSession) {
        throw new Error('Test users or browser session not available');
      }

      console.log(`👤 Current user (should show as ME): ${human1.userId}`);
      console.log(`👤 Other user (should show as OTHER): ${human2.userId}`);

      // Human1 sends message (should appear as ME)
      await this.userManager.sendMessage('Human', this.testRoomId, 'Message from Human1 - should show as ME');
      await this.browserSession.screenshot('chat-widget', 'human1-me-message');

      // Human2 sends message (should appear as OTHER)
      await human2.client.executeCommand('collaboration/chat/send', {
        roomId: this.testRoomId,
        content: 'Message from Human2 - should show as OTHER',
        senderName: human2.name
      });
      await this.browserSession.screenshot('chat-widget', 'human2-other-message');

      // Verify positioning in DOM
      const positioningResult = await this.browserSession.executeScript(`
        console.log('🔍 POSITIONING TEST: Analyzing message positioning...');

        const chatWidget = document.querySelector('chat-widget');
        if (!chatWidget) {
          return { success: false, error: 'Chat widget not found' };
        }

        // Find all message elements
        const messageElements = chatWidget.shadowRoot?.querySelectorAll('.message-row, .message') ||
                               chatWidget.querySelectorAll('.message-row, .message') ||
                               [];

        const analysisResults = [];
        messageElements.forEach((element, index) => {
          const messageContent = element.textContent || '';
          const classes = element.className;
          const alignment = classes.includes('right') ? 'right' :
                           classes.includes('left') ? 'left' : 'unknown';
          const userType = classes.includes('current-user') ? 'current-user' :
                          classes.includes('other-user') ? 'other-user' :
                          classes.includes('me') ? 'me' :
                          classes.includes('other') ? 'other' : 'unknown';

          analysisResults.push({
            index,
            content: messageContent.substring(0, 50) + '...',
            classes,
            alignment,
            userType,
            isHuman1: messageContent.includes('Human1'),
            isHuman2: messageContent.includes('Human2')
          });
        });

        return {
          success: true,
          messageCount: messageElements.length,
          messages: analysisResults
        };
      `, 'Message positioning analysis');

      if (positioningResult.success && positioningResult.messages) {
        const human1Message = positioningResult.messages.find((m: any) => m.isHuman1);
        const human2Message = positioningResult.messages.find((m: any) => m.isHuman2);

        const human1CorrectlyPositioned = human1Message &&
          (human1Message.alignment === 'right' || human1Message.userType === 'current-user');
        const human2CorrectlyPositioned = human2Message &&
          (human2Message.alignment === 'left' || human2Message.userType === 'other-user');

        this.results.push({
          test: 'Human1 message shows as ME (right-aligned)',
          success: !!human1CorrectlyPositioned,
          details: human1CorrectlyPositioned ?
            `Correctly positioned as ME: ${human1Message?.alignment}/${human1Message?.userType}` :
            `Incorrectly positioned: ${human1Message?.alignment || 'not found'}`,
          currentUserId: human1.userId,
          messageUserId: human1.userId,
          cssClasses: human1Message?.classes || 'not found',
          alignment: human1Message?.alignment || 'not found',
          screenshot: 'human1-me-message'
        });

        this.results.push({
          test: 'Human2 message shows as OTHER (left-aligned)',
          success: !!human2CorrectlyPositioned,
          details: human2CorrectlyPositioned ?
            `Correctly positioned as OTHER: ${human2Message?.alignment}/${human2Message?.userType}` :
            `Incorrectly positioned: ${human2Message?.alignment || 'not found'}`,
          currentUserId: human1.userId,
          messageUserId: human2.userId,
          cssClasses: human2Message?.classes || 'not found',
          alignment: human2Message?.alignment || 'not found',
          screenshot: 'human2-other-message'
        });
      } else {
        this.results.push({
          test: 'Same entity type positioning analysis',
          success: false,
          details: `Failed to analyze positioning: ${positioningResult.error || 'Unknown error'}`
        });
      }

    } catch (error) {
      this.results.push({
        test: 'Same entity type different users test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 2: Different Entity Types, Current User
   * Test that AI messages from current user show as ME
   */
  async testDifferentEntityTypeCurrentUser(): Promise<void> {
    console.log('\n🤖 TEST 2: AI Message from Current User (should show as ME)');

    try {
      if (!this.browserSession) {
        throw new Error('Browser session not available');
      }

      // Simulate current user sending different message types
      const testMessages = [
        { type: 'user', content: 'Human message from current user' },
        { type: 'assistant', content: 'AI assistant response from current user' },
        { type: 'system', content: 'System message from current user' }
      ];

      const currentUser = this.userManager.getUser('Human');
      if (!currentUser) {
        throw new Error('Current user not found');
      }

      for (const msgData of testMessages) {
        // Send message with different entity type but same user ID
        await currentUser.client.executeCommand('collaboration/chat/send', {
          roomId: this.testRoomId,
          content: msgData.content,
          senderName: currentUser.name,
          messageType: msgData.type
        });

        await this.browserSession.screenshot('chat-widget', `current-user-${msgData.type}-message`);
      }

      // Analyze positioning for all message types from current user
      const entityTypeResult = await this.browserSession.executeScript(`
        console.log('🔍 ENTITY TYPE TEST: Analyzing current user messages with different types...');

        const chatWidget = document.querySelector('chat-widget');
        if (!chatWidget) {
          return { success: false, error: 'Chat widget not found' };
        }

        const messageElements = chatWidget.shadowRoot?.querySelectorAll('.message-row, .message') ||
                               chatWidget.querySelectorAll('.message-row, .message') ||
                               [];

        const currentUserMessages = [];
        messageElements.forEach((element, index) => {
          const messageContent = element.textContent || '';
          const classes = element.className;

          // Check if this message is from current user (regardless of type)
          if (messageContent.includes('current user')) {
            const alignment = classes.includes('right') ? 'right' :
                             classes.includes('left') ? 'left' : 'unknown';
            const userType = classes.includes('current-user') ? 'current-user' :
                            classes.includes('other-user') ? 'other-user' : 'unknown';

            let entityType = 'unknown';
            if (messageContent.includes('Human message')) entityType = 'human';
            if (messageContent.includes('AI assistant')) entityType = 'assistant';
            if (messageContent.includes('System message')) entityType = 'system';

            currentUserMessages.push({
              index,
              content: messageContent.substring(0, 40) + '...',
              classes,
              alignment,
              userType,
              entityType,
              correctlyPositioned: alignment === 'right' || userType === 'current-user'
            });
          }
        });

        return {
          success: true,
          currentUserMessages
        };
      `, 'Current user entity type analysis');

      if (entityTypeResult.success && entityTypeResult.currentUserMessages) {
        entityTypeResult.currentUserMessages.forEach((msg: any) => {
          this.results.push({
            test: `Current user ${msg.entityType} message shows as ME`,
            success: msg.correctlyPositioned,
            details: msg.correctlyPositioned ?
              `Correctly positioned as ME: ${msg.alignment}/${msg.userType}` :
              `Incorrectly positioned: ${msg.alignment}/${msg.userType}`,
            currentUserId: currentUser.userId,
            messageUserId: currentUser.userId,
            cssClasses: msg.classes,
            alignment: msg.alignment,
            screenshot: `current-user-${msg.entityType}-message`
          });
        });
      } else {
        this.results.push({
          test: 'Current user entity type analysis',
          success: false,
          details: `Failed to analyze entity types: ${entityTypeResult.error || 'Unknown error'}`
        });
      }

    } catch (error) {
      this.results.push({
        test: 'Different entity type current user test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 3: CSS Class Validation
   * Verify semantic CSS classes are used instead of entity type classes
   */
  async testCSSClassValidation(): Promise<void> {
    console.log('\n🎨 TEST 3: CSS Class Validation');

    try {
      if (!this.browserSession) {
        throw new Error('Browser session not available');
      }

      // Check CSS rules and classes
      const cssValidationResult = await this.browserSession.executeScript(`
        console.log('🎨 CSS VALIDATION: Checking for semantic vs entity-type classes...');

        const chatWidget = document.querySelector('chat-widget');
        if (!chatWidget) {
          return { success: false, error: 'Chat widget not found' };
        }

        // Get all stylesheets
        const stylesheets = document.styleSheets;
        const shadowStylesheets = chatWidget.shadowRoot ?
          Array.from(chatWidget.shadowRoot.styleSheets) : [];

        const cssRules = [];

        // Check for old wrong classes (entity-type based)
        const wrongClasses = ['.message.user', '.message.assistant', '.message.system'];
        const correctClasses = ['.message.current-user', '.message.other-user',
                               '.message-bubble.current-user', '.message-bubble.other-user'];

        let hasWrongClasses = false;
        let hasCorrectClasses = false;

        // Check actual DOM elements for classes
        const messageElements = chatWidget.shadowRoot?.querySelectorAll('.message-row, .message') ||
                               chatWidget.querySelectorAll('.message-row, .message') ||
                               [];

        const classesFound = new Set();
        messageElements.forEach(element => {
          element.className.split(' ').forEach(cls => {
            if (cls.trim()) classesFound.add('.' + cls.trim());
          });
        });

        // Check for wrong patterns
        wrongClasses.forEach(wrongClass => {
          if (Array.from(classesFound).some(found => found === wrongClass)) {
            hasWrongClasses = true;
          }
        });

        // Check for correct patterns
        correctClasses.forEach(correctClass => {
          if (Array.from(classesFound).some(found => found === correctClass)) {
            hasCorrectClasses = true;
          }
        });

        return {
          success: true,
          classesFound: Array.from(classesFound),
          hasWrongClasses,
          hasCorrectClasses,
          wrongPatterns: wrongClasses,
          correctPatterns: correctClasses
        };
      `, 'CSS class validation');

      if (cssValidationResult.success) {
        this.results.push({
          test: 'No entity-type CSS classes used (.message.user, .message.assistant)',
          success: !cssValidationResult.hasWrongClasses,
          details: cssValidationResult.hasWrongClasses ?
            'Found wrong entity-type classes - still using .message.user/.assistant' :
            'Correctly avoiding entity-type classes',
          cssClasses: cssValidationResult.classesFound?.join(', ') || 'none found'
        });

        this.results.push({
          test: 'Semantic CSS classes used (.current-user, .other-user)',
          success: cssValidationResult.hasCorrectClasses,
          details: cssValidationResult.hasCorrectClasses ?
            'Found correct semantic classes - using .current-user/.other-user' :
            'Missing semantic classes - positioning may not work',
          cssClasses: cssValidationResult.classesFound?.join(', ') || 'none found'
        });
      } else {
        this.results.push({
          test: 'CSS class validation',
          success: false,
          details: `Failed to validate CSS: ${cssValidationResult.error || 'Unknown error'}`
        });
      }

    } catch (error) {
      this.results.push({
        test: 'CSS class validation test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * Helper: Create test user with specific type
   */
  private async createTestUser(name: string, userType: 'human' | 'ai' | 'agent' | 'persona') {
    const result = await JTAGClientServer.connect();
    const client = result.client;

    // Create user with specific type
    await client.executeCommand(DATA_COMMANDS.CREATE, {
      collection: 'users',
      data: {
        name,
        type: userType,
        id: `test-${name.toLowerCase()}-${Date.now()}`,
        createdAt: new Date().toISOString()
      }
    });

    return {
      client,
      name,
      userId: `test-${name.toLowerCase()}-${Date.now()}`,
      type: userType
    };
  }

  /**
   * Helper: Clear test room
   */
  private async clearTestRoom(): Promise<void> {
    const controlUser = this.userManager.getUser('Human');
    if (controlUser) {
      await controlUser.client.executeCommand(DATA_COMMANDS.CLEAR, {
        collection: 'chat_messages',
        filter: { roomId: this.testRoomId }
      });
    }
  }

  /**
   * Run all positioning tests
   */
  async runAllTests(): Promise<void> {
    await this.initialize();

    await this.testSameEntityTypeDifferentUsers();
    await this.testDifferentEntityTypeCurrentUser();
    await this.testCSSClassValidation();
  }

  /**
   * Display comprehensive test results
   */
  displayResults(): void {
    console.log(`\n🎯 CHAT ME vs OTHER POSITIONING TEST RESULTS`);
    console.log('===========================================');

    const passed = this.results.filter(test => test.success).length;
    const total = this.results.length;

    this.results.forEach(result => {
      const status = result.success ? '✅' : '❌';
      const screenshot = result.screenshot ? ` (📸 ${result.screenshot})` : '';
      console.log(`  ${status} ${result.test}: ${result.details}${screenshot}`);

      if (result.currentUserId && result.messageUserId) {
        const relationship = result.currentUserId === result.messageUserId ? 'SAME USER' : 'DIFFERENT USER';
        console.log(`      User relationship: ${relationship} (current: ${result.currentUserId}, message: ${result.messageUserId})`);
      }

      if (result.cssClasses) {
        console.log(`      CSS classes: ${result.cssClasses}`);
      }
    });

    console.log(`\n📊 OVERALL SUMMARY: ${passed}/${total} positioning tests passed`);
    console.log(`Success Rate: ${((passed / total) * 100).toFixed(1)}%`);

    if (passed === total) {
      console.log('🎉 ✅ CHAT ME vs OTHER POSITIONING WORKING PERFECTLY!');
      console.log('👥 Multiple users with same entity type positioned correctly');
      console.log('🤖 Different entity types from current user show as ME');
      console.log('🎨 Semantic CSS classes used instead of entity-type classes');
      console.log('🚀 Ready for multi-user chat with proper visual distinction');
    } else {
      console.log('⚠️  ME vs OTHER positioning has issues');
      console.log('🔧 Fix failing tests for proper user experience');

      // Identify critical failures
      const criticalIssues = [];

      const sameUserIssues = this.results.filter(r =>
        r.currentUserId === r.messageUserId && !r.success
      );
      if (sameUserIssues.length > 0) {
        criticalIssues.push('Current user messages not showing as ME');
      }

      const differentUserIssues = this.results.filter(r =>
        r.currentUserId !== r.messageUserId && !r.success
      );
      if (differentUserIssues.length > 0) {
        criticalIssues.push('Other user messages not showing as OTHER');
      }

      const cssIssues = this.results.filter(r =>
        r.test.includes('CSS') && !r.success
      );
      if (cssIssues.length > 0) {
        criticalIssues.push('Wrong CSS classes used for positioning');
      }

      if (criticalIssues.length > 0) {
        console.log('\n🚨 CRITICAL POSITIONING FAILURES:');
        criticalIssues.forEach(issue => console.log(`   ❌ ${issue}`));
        console.log('\n💡 Users cannot distinguish their messages from others!');
      }
    }

    console.log(`\n📸 Visual documentation: ${this.browserSession?.getSessionSummary() || 'Not available'}`);
  }

  /**
   * Cleanup
   */
  async cleanup(): Promise<void> {
    await this.userManager.disconnectAll();
  }
}

// Main execution
async function runChatMeOtherPositioningTest(): Promise<void> {
  const testRunner = new ChatMeOtherPositioningTest();

  try {
    await testRunner.runAllTests();
    testRunner.displayResults();

  } catch (error) {
    console.error('💥 Chat ME/OTHER positioning test execution failed:', error);
    process.exit(1);
  } finally {
    await testRunner.cleanup();
  }
}

// Run test if this file is executed directly
if (require.main === module) {
  runChatMeOtherPositioningTest().catch(error => {
    console.error('💥 Test runner failed:', error);
    process.exit(1);
  });
}

export { runChatMeOtherPositioningTest, ChatMeOtherPositioningTest };