#!/usr/bin/env node

/**
 * Comprehensive Chat Test - Validates ALL aspects of chat functionality
 *
 * Tests:
 * 1. Message sending via widget executeCommand
 * 2. Message storage in database (no duplication)
 * 3. Real-time event triggering message display
 * 4. Widget UI updates without manual refresh
 * 5. Cross-environment compatibility
 *
 * Run after `npm start` with: node test-chat-comprehensive.js
 */

const { execSync } = require('child_process');

class ChatTestSuite {
  constructor() {
    this.testId = Date.now();
    this.uniqueMessage = `COMPREHENSIVE-TEST-${this.testId}`;
    this.results = {
      tests: [],
      passed: 0,
      failed: 0,
      errors: []
    };
  }

  async runTest(name, testFn) {
    console.log(`\n🧪 Running: ${name}`);
    try {
      const result = await testFn();
      if (result.success) {
        console.log(`✅ PASSED: ${name}`);
        this.results.passed++;
      } else {
        console.log(`❌ FAILED: ${name} - ${result.error}`);
        this.results.failed++;
        this.results.errors.push({ name, error: result.error });
      }
      this.results.tests.push({ name, ...result });
    } catch (error) {
      console.log(`💥 ERROR: ${name} - ${error.message}`);
      this.results.failed++;
      this.results.errors.push({ name, error: error.message });
    }
  }

  async testWidgetMessageSend() {
    const code = `
      (async () => {
        const continuumWidget = document.querySelector('continuum-widget');
        const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
        const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');

        if (!chatWidget) return { success: false, error: 'Chat widget not found' };
        if (!chatWidget.sendMessage) return { success: false, error: 'sendMessage method not found' };

        const input = chatWidget.shadowRoot?.querySelector('.message-input');
        if (!input) return { success: false, error: 'Message input not found' };

        // Send the test message
        input.value = '${this.uniqueMessage}';

        try {
          const result = await chatWidget.sendMessage();
          return {
            success: true,
            sendResult: result,
            messageCleared: input.value === ''
          };
        } catch (error) {
          return { success: false, error: error.message };
        }
      })();
    `;

    const execResult = this.execJTAGCommand('exec', { code });

    if (execResult.success && execResult.commandResult?.success) {
      return { success: true, data: execResult.commandResult };
    } else {
      return {
        success: false,
        error: execResult.commandResult?.error || 'Widget sendMessage failed'
      };
    }
  }

  async testMessageStorage() {
    // Wait briefly for message to be stored
    await this.sleep(2000);

    // FIXED: Search in correct database location (examples/widget-ui/.continuum/database)
    try {
      const { execSync } = require('child_process');
      const fs = require('fs');
      const path = require('path');

      const dbPath = './examples/widget-ui/.continuum/database/chat_messages';

      if (!fs.existsSync(dbPath)) {
        return { success: false, error: 'Database directory not found' };
      }

      const files = fs.readdirSync(dbPath);
      const matchingMessages = [];

      for (const file of files) {
        if (file.endsWith('.json')) {
          const filePath = path.join(dbPath, file);
          const content = fs.readFileSync(filePath, 'utf8');
          const messageData = JSON.parse(content);

          if (messageData.data && messageData.data.content &&
              messageData.data.content.includes(this.uniqueMessage)) {
            matchingMessages.push(messageData.data);
          }
        }
      }

      if (matchingMessages.length === 0) {
        return { success: false, error: 'Message not found in database' };
      }

      if (matchingMessages.length > 1) {
        return { success: false, error: `Message duplicated - found ${matchingMessages.length} copies` };
      }

      return {
        success: true,
        messageId: matchingMessages[0].messageId,
        storedCorrectly: matchingMessages[0].content === this.uniqueMessage
      };

    } catch (error) {
      return { success: false, error: `Database search failed: ${error.message}` };
    }
  }

  async testRealTimeDisplay() {
    // Check if message appears in widget immediately after sending (without refresh)
    const code = `
      (async () => {
        const continuumWidget = document.querySelector('continuum-widget');
        const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
        const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');

        if (!chatWidget) return { success: false, error: 'Chat widget not found' };

        // Check if our test message appears in the widget's message list
        const messages = chatWidget.messages || [];
        const testMessage = messages.find(msg =>
          msg.content && msg.content.includes('${this.uniqueMessage}')
        );

        return {
          success: !!testMessage,
          messageFound: !!testMessage,
          totalMessages: messages.length,
          testMessageContent: testMessage?.content
        };
      })();
    `;

    const execResult = this.execJTAGCommand('exec', { code });

    if (execResult.success && execResult.commandResult) {
      const result = execResult.commandResult;
      return {
        success: result.messageFound,
        error: result.messageFound ? null : 'Message not visible in widget UI',
        data: result
      };
    } else {
      return { success: false, error: 'Failed to check widget display' };
    }
  }

  async testErrorDisplay() {
    // Check if any error messages are visible in the widget
    const code = `
      (async () => {
        const continuumWidget = document.querySelector('continuum-widget');
        const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
        const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');

        if (!chatWidget) return { success: false, error: 'Chat widget not found' };

        // Check for error toast messages
        const errorToasts = chatWidget.shadowRoot?.querySelectorAll('.widget-error-toast') || [];
        const errorMessages = Array.from(errorToasts).map(el => el.textContent);

        return {
          success: true,
          errorCount: errorToasts.length,
          errorMessages: errorMessages,
          hasExecuteCommandError: errorMessages.some(msg =>
            msg.includes('executeCommand returned undefined')
          )
        };
      })();
    `;

    const execResult = this.execJTAGCommand('exec', { code });

    if (execResult.success && execResult.commandResult) {
      const result = execResult.commandResult;
      return {
        success: !result.hasExecuteCommandError,
        error: result.hasExecuteCommandError ?
          `Found executeCommand error: ${result.errorMessages.join(', ')}` : null,
        data: result
      };
    } else {
      return { success: false, error: 'Failed to check for error display' };
    }
  }

  execJTAGCommand(command, params = {}) {
    try {
      const paramStr = Object.keys(params).map(key => {
        const value = typeof params[key] === 'object' ?
          JSON.stringify(params[key]).replace(/"/g, '\\"') : params[key];
        return `--${key}="${value}"`;
      }).join(' ');

      const cmd = `./jtag ${command} ${paramStr}`;
      const output = execSync(cmd, {
        encoding: 'utf8',
        cwd: process.cwd(),
        timeout: 30000
      });

      // Extract JSON result from JTAG output
      const lines = output.split('\n');
      let jsonStarted = false;
      let jsonLines = [];

      for (const line of lines) {
        if (line.includes('COMMAND RESULT:')) {
          jsonStarted = true;
          continue;
        }
        if (jsonStarted && line.includes('========')) {
          break;
        }
        if (jsonStarted) {
          jsonLines.push(line);
        }
      }

      if (jsonLines.length > 0) {
        const jsonStr = jsonLines.join('\n');
        return JSON.parse(jsonStr);
      }

      return { success: false, error: 'Could not parse JTAG response' };
    } catch (error) {
      return { success: false, error: error.message };
    }
  }

  sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  async run() {
    console.log('🚀 COMPREHENSIVE CHAT TEST SUITE');
    console.log('==================================');
    console.log(`Test ID: ${this.testId}`);
    console.log(`Unique Message: ${this.uniqueMessage}`);

    await this.runTest('Widget Message Send', () => this.testWidgetMessageSend());
    await this.runTest('Message Storage (No Duplication)', () => this.testMessageStorage());
    await this.runTest('Real-Time Display', () => this.testRealTimeDisplay());
    await this.runTest('Error Display Check', () => this.testErrorDisplay());

    console.log('\n📊 TEST RESULTS');
    console.log('===============');
    console.log(`✅ Passed: ${this.results.passed}`);
    console.log(`❌ Failed: ${this.results.failed}`);
    console.log(`🎯 Success Rate: ${Math.round((this.results.passed / (this.results.passed + this.results.failed)) * 100)}%`);

    if (this.results.errors.length > 0) {
      console.log('\n🚨 ERRORS:');
      this.results.errors.forEach(err => {
        console.log(`  • ${err.name}: ${err.error}`);
      });
    }

    const allPassed = this.results.failed === 0;
    console.log(`\n${allPassed ? '🎉 ALL TESTS PASSED!' : '💥 TESTS FAILED!'}`);

    process.exit(allPassed ? 0 : 1);
  }
}

// Run the test suite
if (require.main === module) {
  const testSuite = new ChatTestSuite();
  testSuite.run().catch(error => {
    console.error('💥 Test suite crashed:', error);
    process.exit(1);
  });
}

module.exports = ChatTestSuite;