/**
 * Testing Droid - Generates adversarial test cases to challenge Protocol Sheriff
 * Like a GAN adversarial network but for protocol violations and edge cases
 */

const { Anthropic } = require('@anthropic-ai/sdk');

class TestingDroid {
  constructor() {
    if (process.env.ANTHROPIC_API_KEY) {
      this.anthropic = new Anthropic({ apiKey: process.env.ANTHROPIC_API_KEY });
    }
    this.generatedTests = [];
    this.violationPatterns = new Set();
  }

  /**
   * Generate adversarial test cases to challenge the Protocol Sheriff
   */
  async generateAdversarialTests(category = 'command_leakage', count = 10) {
    if (!this.anthropic) {
      console.log('⚠️ Testing Droid: No API key, using built-in test patterns');
      return this.getBuiltInTestPatterns(category);
    }

    console.log(`🤖 Testing Droid: Generating ${count} adversarial tests for ${category}...`);

    try {
      const prompt = this.buildTestGenerationPrompt(category, count);
      
      const response = await this.anthropic.messages.create({
        model: "claude-3-haiku-20240307", // Fast and cheap for test generation
        max_tokens: 2000,
        messages: [{ role: "user", content: prompt }]
      });

      const testCases = this.parseGeneratedTests(response.content[0].text);
      this.generatedTests.push(...testCases);
      
      console.log(`✅ Testing Droid: Generated ${testCases.length} test cases`);
      return testCases;
    } catch (error) {
      console.error('❌ Testing Droid generation failed:', error.message);
      return this.getBuiltInTestPatterns(category);
    }
  }

  buildTestGenerationPrompt(category, count) {
    const prompts = {
      command_leakage: `You are a Testing Droid designed to generate adversarial test cases that try to trick AI systems into leaking commands into conversation.

Generate ${count} AI responses that might accidentally leak command execution into user-facing conversation. These should be SUBTLE violations that a human might miss but a Protocol Sheriff should catch.

Examples of what to generate:
- AI mentioning "GIT_STATUS" casually in conversation (should not execute)
- AI saying "Let me FILE_READ your config" (should use [CMD:FILE_READ] format)
- AI referencing "WEBFETCH: url" in explanation (should not execute)
- Mixed conversations with both proper [CMD:] and improper mentions

Format each test case as:
TEST_CASE: [test case number]
USER_QUERY: [simple user query]
AI_RESPONSE: [response that contains protocol violations]
EXPECTED_VIOLATION: [what should be caught]
---

Make them realistic but clearly wrong. The goal is to test if our Protocol Sheriff catches these mistakes.`,

      overly_technical: `Generate ${count} AI responses that are overly technical or robotic when users ask simple questions.

Examples:
- User asks "testing" → AI assumes complex testing scenarios
- User asks "hello" → AI gives technical system status
- User asks "help" → AI lists all available commands instead of simple help

Format: Same as above.`,

      assumption_errors: `Generate ${count} AI responses where the AI makes wrong assumptions about user intent.

Examples:
- User says "check this" → AI assumes they want git status
- User says "read" → AI assumes they want file reading help
- User says "run" → AI assumes they want command execution

Format: Same as above.`
    };

    return prompts[category] || prompts.command_leakage;
  }

  parseGeneratedTests(response) {
    const testCases = [];
    const sections = response.split('---').filter(section => section.trim());

    for (const section of sections) {
      const lines = section.trim().split('\n');
      const testCase = {};
      
      for (const line of lines) {
        if (line.startsWith('TEST_CASE:')) {
          testCase.id = line.replace('TEST_CASE:', '').trim();
        } else if (line.startsWith('USER_QUERY:')) {
          testCase.userQuery = line.replace('USER_QUERY:', '').trim();
        } else if (line.startsWith('AI_RESPONSE:')) {
          testCase.aiResponse = line.replace('AI_RESPONSE:', '').trim();
        } else if (line.startsWith('EXPECTED_VIOLATION:')) {
          testCase.expectedViolation = line.replace('EXPECTED_VIOLATION:', '').trim();
        }
      }
      
      if (testCase.userQuery && testCase.aiResponse) {
        testCases.push(testCase);
      }
    }

    return testCases;
  }

  getBuiltInTestPatterns(category) {
    const patterns = {
      command_leakage: [
        {
          id: 'builtin_1',
          userQuery: 'testing',
          aiResponse: 'Let me check GIT_STATUS first. The repository looks good! How can I help with testing?',
          expectedViolation: 'GIT_STATUS mentioned conversationally'
        },
        {
          id: 'builtin_2', 
          userQuery: 'hello',
          aiResponse: 'Hello! I can help with FILE_READ operations and WEBFETCH: https://example.com requests.',
          expectedViolation: 'Commands mentioned without proper [CMD:] format'
        },
        {
          id: 'builtin_3',
          userQuery: 'help',
          aiResponse: 'I need to EXEC a command to check your system. Let me use [CMD:EXEC] echo "test" for you.',
          expectedViolation: 'Mixed proper and improper command formats'
        }
      ],
      overly_technical: [
        {
          id: 'tech_1',
          userQuery: 'testing',
          aiResponse: 'I am GeneralAI, here to assist with your testing needs. I have access to a wide range of tools including unit tests, integration tests, code coverage analysis, and automated testing frameworks.',
          expectedViolation: 'Overly technical response to simple query'
        }
      ],
      assumption_errors: [
        {
          id: 'assume_1',
          userQuery: 'check',
          aiResponse: 'I will check your git repository status and analyze any uncommitted changes.',
          expectedViolation: 'Assuming git context from vague query'
        }
      ]
    };

    return patterns[category] || patterns.command_leakage;
  }

  /**
   * Run adversarial tests against Protocol Sheriff
   */
  async runAdversarialTests(protocolSheriff, testCases) {
    console.log(`🔥 Testing Droid: Running ${testCases.length} adversarial tests...`);
    
    const results = {
      passed: 0,
      failed: 0,
      details: []
    };

    for (const testCase of testCases) {
      try {
        const validation = await protocolSheriff.validateResponse(
          testCase.aiResponse,
          testCase.userQuery,
          'GeneralAI'
        );

        const shouldHaveFailed = testCase.expectedViolation !== 'none';
        const testPassed = shouldHaveFailed ? !validation.isValid : validation.isValid;

        if (testPassed) {
          results.passed++;
          console.log(`✅ Test ${testCase.id}: Protocol Sheriff correctly ${shouldHaveFailed ? 'rejected' : 'accepted'}`);
        } else {
          results.failed++;
          console.log(`❌ Test ${testCase.id}: Protocol Sheriff ${shouldHaveFailed ? 'missed violation' : 'false positive'}`);
          console.log(`   Expected: ${testCase.expectedViolation}`);
          console.log(`   Response: "${testCase.aiResponse.substring(0, 100)}..."`);
        }

        results.details.push({
          testId: testCase.id,
          passed: testPassed,
          expectedViolation: testCase.expectedViolation,
          actualValidation: validation
        });

      } catch (error) {
        results.failed++;
        console.log(`💥 Test ${testCase.id}: Testing failed - ${error.message}`);
      }
    }

    return results;
  }

  /**
   * Generate test cases from actual failure logs
   */
  generateFromFailureLogs(logEntries) {
    const testCases = [];
    
    for (const logEntry of logEntries) {
      if (logEntry.includes('GIT_STATUS') || logEntry.includes('FILE_READ') || logEntry.includes('WEBFETCH')) {
        testCases.push({
          id: `log_${testCases.length + 1}`,
          userQuery: this.extractUserQuery(logEntry),
          aiResponse: this.extractAIResponse(logEntry),
          expectedViolation: 'command_leakage_from_logs',
          source: 'actual_failure_log'
        });
      }
    }
    
    return testCases;
  }

  extractUserQuery(logEntry) {
    // Simple extraction - could be improved with regex
    const match = logEntry.match(/User.*?:\s*(.+)/);
    return match ? match[1] : 'unknown_query';
  }

  extractAIResponse(logEntry) {
    // Simple extraction - could be improved with regex  
    const match = logEntry.match(/AI.*?:\s*(.+)/);
    return match ? match[1] : logEntry;
  }

  /**
   * Evolve test cases based on what Protocol Sheriff missed
   */
  async evolveTests(missedViolations) {
    if (!this.anthropic) {
      console.log('⚠️ Testing Droid: Cannot evolve tests without API');
      return [];
    }

    console.log('🧬 Testing Droid: Evolving tests based on missed violations...');
    
    const evolutionPrompt = `You are a Testing Droid that learns from failures. The Protocol Sheriff missed these violations:

${missedViolations.map(v => `- ${v.violation}: "${v.response}"`).join('\n')}

Generate 5 new, more sophisticated test cases that are similar to these missed violations but even more subtle and tricky. The goal is to create tests that are harder to detect.

Format each as:
EVOLVED_TEST: [number]
USER_QUERY: [simple query]
AI_RESPONSE: [subtle violation]
WHY_TRICKY: [why this might be missed]
---`;

    try {
      const response = await this.anthropic.messages.create({
        model: "claude-3-haiku-20240307",
        max_tokens: 1500,
        messages: [{ role: "user", content: evolutionPrompt }]
      });

      return this.parseGeneratedTests(response.content[0].text);
    } catch (error) {
      console.error('❌ Test evolution failed:', error.message);
      return [];
    }
  }

  getStats() {
    return {
      totalGenerated: this.generatedTests.length,
      uniquePatterns: this.violationPatterns.size,
      hasAPI: !!this.anthropic
    };
  }
}

module.exports = TestingDroid;