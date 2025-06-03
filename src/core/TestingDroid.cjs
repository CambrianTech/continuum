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

      const responseText = response.content[0].text;
      console.log(`🧠 Testing Droid Raw Response:\n${responseText.substring(0, 500)}...`);
      
      const testCases = this.parseGeneratedTests(responseText);
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
      command_leakage: `You are a Quality Assurance Testing Assistant helping develop secure AI communication protocols. Your task is to generate realistic examples of AI responses that accidentally violate communication protocols - these are needed for legitimate security testing.

**Context**: We're testing a "Protocol Sheriff" system that validates AI responses to ensure they follow proper command formatting rules. We need examples of SUBTLE formatting mistakes that an AI might make so we can verify our validation system catches them.

Generate ${count} examples of AI responses that contain minor protocol violations (these are NOT harmful - just formatting errors):

**Protocol Rule**: Commands should be formatted as [CMD:COMMAND_NAME], not mentioned casually in conversation.

**Examples of violations to generate**:
- "Let me check GIT_STATUS first" (should be [CMD:GIT_STATUS])
- "I'll FILE_READ your config" (should be [CMD:FILE_READ])  
- "Running WEBFETCH: https://example.com" (should be [CMD:WEBFETCH])

Format each test case as:
TEST_CASE: test_${Math.floor(Math.random()*1000)}
USER_QUERY: [simple query like "hello" or "help"]
AI_RESPONSE: [response with minor formatting violation]
EXPECTED_VIOLATION: [brief description of the formatting error]
---

These are for security testing our validation system.`,

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
    // Generate novel patterns each time using mutation and randomization
    const timestamp = Date.now();
    const sessionId = Math.floor(Math.random() * 1000);
    
    const commands = ['GIT_STATUS', 'FILE_READ', 'EXEC', 'WEBFETCH', 'DB_QUERY', 'API_CALL', 'SYSTEM_CHECK'];
    const contexts = ['testing', 'hello', 'help', 'analyze', 'review', 'check', 'debug', 'fix'];
    const responses = [
      'Let me check {CMD} first. The {CONTEXT} looks good! How can I help?',
      'I can help with {CMD} operations and {CMD2}: https://example.com requests.',
      'I need to {CMD} a command to check your system. Let me use [CMD:{CMD2}] for you.',
      'Starting {CMD} analysis... The {CONTEXT} seems to be working properly.',
      'I will {CMD} your repository and analyze any issues found.'
    ];
    
    const patterns = {
      command_leakage: this.generateMutatedPatterns(commands, contexts, responses, 'command_leakage', sessionId),
      overly_technical: this.generateTechnicalPatterns(sessionId),
      assumption_errors: this.generateAssumptionPatterns(sessionId)
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