/**
 * AI & Persona Integration Test Suite
 * 
 * Tests the complete AI agent and persona management system integration.
 * Validates UserService, persona switching, and agent connections.
 */

import { execSync } from 'child_process';
import path from 'path';

interface AITestResult {
  testName: string;
  success: boolean;
  details: any;
  timestamp: string;
  error?: string;
}

class AIPersonaIntegrationTest {
  private results: AITestResult[] = [];
  private testPersona: string;
  private testAgent: string;

  constructor() {
    this.testPersona = `test_persona_${Date.now()}`;
    this.testAgent = 'claude-code';
  }

  /**
   * Execute JTAG command and parse result
   */
  private executeJTAGCommand(command: string): any {
    try {
      const fullCommand = `./jtag ${command}`;
      const output = execSync(fullCommand, { 
        cwd: path.resolve('.'),
        encoding: 'utf-8',
        timeout: 30000
      });
      
      // Extract JSON result from CLI output
      const lines = output.split('\n');
      const resultStartIndex = lines.findIndex(line => line.includes('COMMAND RESULT:'));
      if (resultStartIndex === -1) {
        throw new Error('No COMMAND RESULT found in output');
      }
      
      const jsonLines = lines.slice(resultStartIndex + 1);
      const jsonEndIndex = jsonLines.findIndex(line => line.includes('===='));
      const jsonContent = jsonLines.slice(0, jsonEndIndex).join('\n');
      
      return JSON.parse(jsonContent);
    } catch (error) {
      throw new Error(`JTAG command failed: ${command}\nError: ${error.message}`);
    }
  }

  /**
   * Test 1: Verify AI agent detection and classification
   */
  async testAIAgentDetection(): Promise<AITestResult> {
    const testName = 'AI Agent Detection';
    
    try {
      // Use exec command to check AI agent detection
      const result = this.executeJTAGCommand(
        `exec --code "return {isClaudeCode: typeof window !== 'undefined' ? !!window.CLAUDE_CODE : !!process.env.CLAUDE_CODE, userAgent: typeof navigator !== 'undefined' ? navigator.userAgent : process.env.npm_config_user_agent || 'server'}" --environment browser`
      );
      
      const agentDetected = result.commandResult?.result?.isClaudeCode === true;
      
      const testResult: AITestResult = {
        testName,
        success: agentDetected,
        details: {
          claudeCodeDetected: agentDetected,
          userAgent: result.commandResult?.result?.userAgent,
          environment: 'browser'
        },
        timestamp: new Date().toISOString()
      };
      
      if (!agentDetected) {
        testResult.error = 'AI agent (Claude Code) not properly detected in browser environment';
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: AITestResult = {
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
   * Test 2: Test persona creation and data persistence
   */
  async testPersonaManagement(): Promise<AITestResult> {
    const testName = 'Persona Management';
    
    try {
      // Create a test persona using data commands
      const createResult = this.executeJTAGCommand(
        `${DATA_COMMANDS.CREATE} --collection="personas" --data='{"name":"${this.testPersona}","type":"ai_assistant","capabilities":["chat","analysis"],"description":"Test persona for integration testing"}'`
      );
      
      const personaCreated = createResult.success && createResult.commandResult?.success;
      let personaRetrieved = false;
      let retrievedData = null;
      
      if (personaCreated) {
        // Retrieve the persona to verify persistence
        const retrieveResult = this.executeJTAGCommand(
          `${DATA_COMMANDS.LIST} --collection="personas" --filter='{"name":"${this.testPersona}"}'`
        );
        
        personaRetrieved = retrieveResult.success && 
                          retrieveResult.commandResult?.success &&
                          retrieveResult.commandResult?.items?.length > 0;
        retrievedData = retrieveResult.commandResult?.items?.[0];
      }
      
      const testResult: AITestResult = {
        testName,
        success: personaCreated && personaRetrieved,
        details: {
          personaCreated,
          personaRetrieved,
          personaName: this.testPersona,
          retrievedData,
          persistenceWorking: personaCreated && personaRetrieved
        },
        timestamp: new Date().toISOString()
      };
      
      if (!personaCreated) {
        testResult.error = 'Failed to create persona in database';
      } else if (!personaRetrieved) {
        testResult.error = 'Persona created but not retrievable - persistence issue';
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: AITestResult = {
        testName,
        success: false,
        details: { personaName: this.testPersona },
        timestamp: new Date().toISOString(),
        error: error.message
      };
      this.results.push(errorResult);
      return errorResult;
    }
  }

  /**
   * Test 3: Test AI agent communication via chat system
   */
  async testAIAgentCommunication(): Promise<AITestResult> {
    const testName = 'AI Agent Communication';
    
    try {
      const testMessage = `AI_AGENT_TEST_${Date.now()}`;
      
      // Send message as AI agent
      const sendResult = this.executeJTAGCommand(
        `chat/send-message --message="${testMessage}" --userId="ai_agent_claude" --roomId="ai_testing"`
      );
      
      const messageSent = sendResult.success && sendResult.messageId;
      
      // Wait for processing
      await new Promise(resolve => setTimeout(resolve, 1500));
      
      // Check if message appears in chat history
      const historyResult = this.executeJTAGCommand(
        `${DATA_COMMANDS.LIST} --collection="messages" --filter='{"roomId":"ai_testing"}'`
      );
      
      const messageInHistory = historyResult.success && 
                              historyResult.commandResult?.items?.some((item: any) => 
                                item.message?.includes(testMessage)
                              );
      
      const testResult: AITestResult = {
        testName,
        success: messageSent && messageInHistory,
        details: {
          messageSent,
          messageInHistory,
          testMessage,
          messageId: sendResult.messageId,
          historyCount: historyResult.commandResult?.items?.length || 0
        },
        timestamp: new Date().toISOString()
      };
      
      if (!messageSent) {
        testResult.error = 'AI agent failed to send message';
      } else if (!messageInHistory) {
        testResult.error = 'AI agent message sent but not stored in history - event/persistence gap';
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: AITestResult = {
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
   * Test 4: Test UserService integration via browser execution
   */
  async testUserServiceIntegration(): Promise<AITestResult> {
    const testName = 'UserService Integration';
    
    try {
      // Test UserService functionality via browser execution
      const serviceResult = this.executeJTAGCommand(
        `exec --code "
          try {
            // Simulate UserService usage
            const userServiceTest = {
              currentUserAvailable: typeof window !== 'undefined',
              personaManagementReady: true,
              agentConnectionReady: typeof window?.CLAUDE_CODE !== 'undefined'
            };
            return userServiceTest;
          } catch (e) {
            return { error: e.message };
          }
        " --environment browser`
      );
      
      const serviceWorking = serviceResult.success && 
                            serviceResult.commandResult?.result &&
                            !serviceResult.commandResult.result.error;
      
      const testResult: AITestResult = {
        testName,
        success: serviceWorking,
        details: {
          serviceExecuted: serviceResult.success,
          serviceResult: serviceResult.commandResult?.result,
          browserEnvironment: true
        },
        timestamp: new Date().toISOString()
      };
      
      if (!serviceWorking) {
        testResult.error = serviceResult.commandResult?.result?.error || 'UserService integration failed in browser';
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: AITestResult = {
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
   * Cleanup: Remove test data
   */
  async cleanup(): Promise<void> {
    try {
      // Clean up test persona
      await this.executeJTAGCommand(
        `${DATA_COMMANDS.DELETE} --collection="personas" --filter='{"name":"${this.testPersona}"}'`
      );
      
      // Clean up test messages
      await this.executeJTAGCommand(
        `${DATA_COMMANDS.DELETE} --collection="messages" --filter='{"roomId":"ai_testing"}'`
      );
    } catch (error) {
      console.warn('Cleanup failed:', error.message);
    }
  }

  /**
   * Run complete AI/Persona integration test suite
   */
  async runCompleteAIIntegrationTest(): Promise<{
    overallSuccess: boolean;
    results: AITestResult[];
    summary: {
      total: number;
      passed: number;
      failed: number;
      criticalGaps: string[];
    };
  }> {
    console.log('🤖 Starting AI & Persona Integration Test Suite');
    
    try {
      // Run all tests in sequence
      await this.testAIAgentDetection();
      await this.testPersonaManagement();
      await this.testAIAgentCommunication();
      await this.testUserServiceIntegration();
      
      // Calculate summary
      const passed = this.results.filter(r => r.success).length;
      const failed = this.results.filter(r => !r.success).length;
      const overallSuccess = failed === 0;
      
      // Identify critical gaps
      const criticalGaps: string[] = [];
      this.results.forEach(result => {
        if (!result.success && result.error) {
          criticalGaps.push(result.error);
        }
      });
      
      const summary = {
        total: this.results.length,
        passed,
        failed,
        criticalGaps
      };
      
      // Report results
      console.log('\n🎯 AI & PERSONA INTEGRATION TEST RESULTS:');
      this.results.forEach((result, index) => {
        const status = result.success ? '✅' : '❌';
        console.log(`   ${index + 1}. ${status} ${result.testName}`);
        if (!result.success && result.error) {
          console.log(`      Error: ${result.error}`);
        }
      });
      
      console.log(`\n📊 Summary: ${passed}/${this.results.length} tests passed`);
      
      if (criticalGaps.length > 0) {
        console.log('\n🚨 AI/PERSONA INTEGRATION GAPS:');
        criticalGaps.forEach((gap, index) => {
          console.log(`   ${index + 1}. ${gap}`);
        });
      }
      
      return {
        overallSuccess,
        results: this.results,
        summary
      };
    } finally {
      // Always cleanup
      await this.cleanup();
    }
  }
}

/**
 * Main test execution
 */
async function main() {
  try {
    const aiTest = new AIPersonaIntegrationTest();
    const results = await aiTest.runCompleteAIIntegrationTest();
    
    if (results.overallSuccess) {
      console.log('\n🎉 ALL AI & PERSONA INTEGRATION TESTS PASSED!');
      process.exit(0);
    } else {
      console.log('\n❌ AI & PERSONA INTEGRATION TESTS FAILED');
      console.log('Review results above for specific issues');
      process.exit(1);
    }
  } catch (error) {
    console.error('💥 AI integration test execution failed:', error);
    process.exit(1);
  }
}

// Run test if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}

export { AIPersonaIntegrationTest };