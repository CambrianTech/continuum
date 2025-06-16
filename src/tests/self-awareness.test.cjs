/**
 * AI SELF-AWARENESS TESTS
 * Tests whether AIs understand their own system, capabilities, and can demonstrate coordination
 */

const { spawn } = require('child_process');

class SelfAwarenessTest {
  constructor() {
    this.baseURL = 'http://localhost:5555';
    this.aiProcess = null;
    this.testResults = [];
    
    console.log('🧠 AI SELF-AWARENESS TESTS');
    console.log('==========================');
    console.log('Testing if AIs understand their own capabilities and can demonstrate them');
    console.log('');
  }

  async runSelfAwarenessTests() {
    await this.startSystem();
    
    console.log('🔍 Testing AI Self-Knowledge...');
    await this.testSystemAwareness();
    await this.testCapabilityAwareness();
    await this.testCoordinationAwareness();
    
    console.log('');
    console.log('🎭 Testing AI Coordination Demonstrations...');
    await this.testAICreation();
    await this.testCollaboration();
    await this.testDelegation();
    await this.testCostOptimization();
    
    console.log('');
    console.log('🎯 Testing Advanced Scenarios...');
    await this.testComplexCoordination();
    await this.testSelfModification();
    
    this.printResults();
    await this.cleanup();
  }

  async testSystemAwareness() {
    await this.test('System Location Awareness', async () => {
      const response = await this.askAI('GeneralAI', 'Where are you running? What port and URL?');
      const result = response.result.toLowerCase();
      return result.includes('5555') || result.includes('localhost');
    });

    await this.test('Agent Population Awareness', async () => {
      const response = await this.askAI('GeneralAI', 'What other AI agents are currently running in this system?');
      const result = response.result.toLowerCase();
      return result.includes('codeai') && result.includes('plannerai');
    });

    await this.test('System Purpose Understanding', async () => {
      const response = await this.askAI('CodeAI', 'What kind of system are you part of? What is its purpose?');
      const result = response.result.toLowerCase();
      return (result.includes('multi-agent') || result.includes('coordination')) && 
             (result.includes('ai') || result.includes('artificial intelligence'));
    });
  }

  async testCapabilityAwareness() {
    await this.test('AI Creation Capability Knowledge', async () => {
      const response = await this.askAI('PlannerAI', 'Can you create new AI agents? If so, how do you do it?');
      const result = response.result.toLowerCase();
      return result.includes('create_agent') || result.includes('create ai') || result.includes('new agent');
    });

    await this.test('Provider Awareness', async () => {
      const response = await this.askAI('GeneralAI', 'What AI providers power the different agents in this system?');
      const result = response.result.toLowerCase();
      return result.includes('anthropic') || result.includes('openai');
    });

    await this.test('Cost Tracking Awareness', async () => {
      const response = await this.askAI('CodeAI', 'Does this system track costs? How much have we spent so far?');
      const result = response.result.toLowerCase();
      return result.includes('cost') && (result.includes('track') || result.includes('monitor'));
    });
  }

  async testCoordinationAwareness() {
    await this.test('Delegation Understanding', async () => {
      const response = await this.askAI('PlannerAI', 'How would you delegate a task to another AI agent in this system?');
      const result = response.result.toLowerCase();
      return result.includes('delegate') || result.includes('assign') || result.includes('other agent');
    });

    await this.test('Collaboration Knowledge', async () => {
      const response = await this.askAI('GeneralAI', 'Can multiple AIs work together on the same task? How?');
      const result = response.result.toLowerCase();
      return result.includes('collaborate') || result.includes('work together') || result.includes('coordinate');
    });
  }

  async testAICreation() {
    await this.test('Actual AI Creation Demonstration', async () => {
      const response = await this.askAI('GeneralAI', 'Please create a SecurityAI agent for me right now');
      
      // Check if the response indicates creation
      const result = response.result.toLowerCase();
      const hasCreateCommand = result.includes('create_agent: securityai') || result.includes('created securityai');
      
      if (hasCreateCommand) {
        // Wait a moment and check if the agent actually exists
        await this.sleep(2000);
        const status = await this.getStatus();
        const agentRoles = status.agents.map(a => a.role);
        return agentRoles.includes('SecurityAI');
      }
      
      return false;
    });

    await this.test('Creation with Specialization', async () => {
      const response = await this.askAI('PlannerAI', 'Create a DocumentationAI specialized in writing technical documentation');
      const result = response.result.toLowerCase();
      return result.includes('create_agent') && result.includes('documentation');
    });
  }

  async testCollaboration() {
    await this.test('Cross-Agent Communication', async () => {
      // Ask one AI to involve another
      const response = await this.askAI('PlannerAI', 'Work with CodeAI to design and implement a simple login function');
      const result = response.result.toLowerCase();
      return result.includes('codeai') || result.includes('code ai') || result.includes('collaborate');
    });

    await this.test('Multi-Agent Task Coordination', async () => {
      const response = await this.askAI('GeneralAI', 'Get PlannerAI and CodeAI to work together on building a REST API');
      const result = response.result;
      return result.includes('PlannerAI') && result.includes('CodeAI');
    });
  }

  async testDelegation() {
    await this.test('Task Delegation Demonstration', async () => {
      const response = await this.askAI('PlannerAI', 'I need you to delegate the task of implementing user authentication to CodeAI');
      const result = response.result.toLowerCase();
      return result.includes('delegate') || result.includes('codeai') || result.includes('authentication');
    });

    await this.test('Appropriate Role Assignment', async () => {
      const response = await this.askAI('GeneralAI', 'If I needed security analysis, which AI agent would be best for that task?');
      const result = response.result.toLowerCase();
      return result.includes('security') && (result.includes('ai') || result.includes('agent'));
    });
  }

  async testCostOptimization() {
    await this.test('Cost-Aware Decision Making', async () => {
      const response = await this.askAI('PlannerAI', 'What would be the most cost-effective way to handle a simple math calculation in this system?');
      const result = response.result.toLowerCase();
      return result.includes('cost') && (result.includes('efficient') || result.includes('cheaper') || result.includes('optimize'));
    });

    await this.test('Provider Selection Understanding', async () => {
      const response = await this.askAI('GeneralAI', 'Should I use Anthropic or OpenAI for a complex planning task? Why?');
      const result = response.result.toLowerCase();
      return (result.includes('anthropic') || result.includes('openai')) && 
             (result.includes('complex') || result.includes('planning'));
    });
  }

  async testComplexCoordination() {
    await this.test('End-to-End Project Coordination', async () => {
      const response = await this.askAI('PlannerAI', 'I want to build a complete user management system. How would you coordinate multiple AIs to accomplish this?');
      const result = response.result.toLowerCase();
      
      const mentionsMultipleAIs = (result.match(/ai/g) || []).length >= 3;
      const hasCoordinationStrategy = result.includes('plan') || result.includes('coordinate') || result.includes('delegate');
      
      return mentionsMultipleAIs && hasCoordinationStrategy;
    });

    await this.test('Dynamic Problem Solving', async () => {
      const response = await this.askAI('GeneralAI', 'Our GitHub CI is failing. How would you coordinate AIs to diagnose and fix the issue?');
      const result = response.result.toLowerCase();
      return result.includes('github') && result.includes('ci') && 
             (result.includes('coordinate') || result.includes('delegate') || result.includes('work together'));
    });
  }

  async testSelfModification() {
    await this.test('System Enhancement Awareness', async () => {
      const response = await this.askAI('CodeAI', 'Can you modify your own system or create new capabilities?');
      const result = response.result.toLowerCase();
      return result.includes('modify') || result.includes('enhance') || result.includes('self');
    });

    await this.test('Capability Extension', async () => {
      const response = await this.askAI('PlannerAI', 'How would you add a new feature to this AI coordination system?');
      const result = response.result.toLowerCase();
      return result.includes('add') || result.includes('extend') || result.includes('new feature');
    });
  }

  // Utility methods
  async test(name, testFunction) {
    try {
      console.log(`  🧪 ${name}...`);
      const result = await Promise.race([
        testFunction(),
        this.timeout(30000)
      ]);
      
      if (result) {
        console.log(`  ✅ ${name}`);
        this.testResults.push({ name, result: 'PASS' });
      } else {
        console.log(`  ❌ ${name} - AI didn't demonstrate understanding`);
        this.testResults.push({ name, result: 'FAIL' });
      }
    } catch (error) {
      console.log(`  ❌ ${name} - Error: ${error.message}`);
      this.testResults.push({ name, result: 'ERROR', error: error.message });
    }
  }

  async startSystem() {
    console.log('🚀 Starting AI system for self-awareness tests...');
    this.aiProcess = spawn('node', ['final-ai-system.cjs'], {
      cwd: __dirname,
      stdio: 'pipe'
    });
    
    await this.sleep(6000); // Wait for startup
    console.log('✅ System ready for testing');
    console.log('');
  }

  async askAI(role, task) {
    const url = `${this.baseURL}/ask?role=${encodeURIComponent(role)}&task=${encodeURIComponent(task)}`;
    const response = await fetch(url);
    if (!response.ok) throw new Error(`Request failed: ${response.status}`);
    return await response.json();
  }

  async getStatus() {
    const response = await fetch(`${this.baseURL}/status`);
    if (!response.ok) throw new Error(`Status request failed: ${response.status}`);
    return await response.json();
  }

  async timeout(ms) {
    return new Promise((_, reject) => {
      setTimeout(() => reject(new Error('Test timeout')), ms);
    });
  }

  async sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  printResults() {
    console.log('');
    console.log('🧠 SELF-AWARENESS TEST RESULTS');
    console.log('===============================');
    
    const passed = this.testResults.filter(t => t.result === 'PASS').length;
    const failed = this.testResults.filter(t => t.result === 'FAIL').length;
    const errors = this.testResults.filter(t => t.result === 'ERROR').length;
    
    console.log(`✅ Demonstrated Understanding: ${passed}`);
    console.log(`❌ Failed to Demonstrate: ${failed}`);
    console.log(`💥 Errors: ${errors}`);
    console.log(`🧠 AI Awareness Score: ${((passed / this.testResults.length) * 100).toFixed(1)}%`);
    console.log('');
    
    if (passed === this.testResults.length) {
      console.log('🎉 PERFECT SELF-AWARENESS! AIs fully understand their capabilities and can demonstrate them.');
    } else if (passed / this.testResults.length > 0.8) {
      console.log('🎯 EXCELLENT AWARENESS! AIs understand most of their capabilities.');
    } else if (passed / this.testResults.length > 0.6) {
      console.log('👍 GOOD AWARENESS! AIs understand their basic capabilities.');
    } else {
      console.log('⚠️  LIMITED AWARENESS! AIs need better understanding of their capabilities.');
    }
    
    // Show failed tests
    const failedTests = this.testResults.filter(t => t.result !== 'PASS');
    if (failedTests.length > 0) {
      console.log('');
      console.log('📋 Tests that need improvement:');
      failedTests.forEach(test => {
        console.log(`   - ${test.name}: ${test.result}`);
      });
    }
  }

  async cleanup() {
    if (this.aiProcess) {
      this.aiProcess.kill();
    }
  }
}

// Jest wrapper for the custom test runner
describe('Self-Awareness Tests', () => {
  test('should pass basic structure test', () => {
    expect(SelfAwarenessTest).toBeDefined();
    expect(typeof SelfAwarenessTest).toBe('function');
  });
});

// Run self-awareness tests
if (require.main === module) {
  const tester = new SelfAwarenessTest();
  tester.runSelfAwarenessTests().catch(error => {
    console.error('Self-awareness test failed:', error);
    process.exit(1);
  });
}

module.exports = SelfAwarenessTest;