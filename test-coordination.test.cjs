#!/usr/bin/env node
/**
 * Unit tests for AI coordination system
 * Tests whether AIs actually coordinate vs just talking about it
 */

const Continuum = require('./continuum.cjs');

// Mock AI responses for testing
const mockResponses = {
  PlannerAI: "I will coordinate with CodeAI to implement the modular agent factory. Step 1: Create base agent class. Step 2: Implement factory pattern.",
  CodeAI: "Based on PlannerAI's plan, I will now create the BaseAgent class and AgentFactory. Here's the implementation: [code block]",
  GeneralAI: "I can explain the system architecture and help with documentation."
};

class TestContinuum extends Continuum {
  constructor() {
    // Don't call super() to avoid starting the server
    this.sessions = new Map();
    this.costs = { total: 0, requests: 0 };
    this.port = 5556; // Different port
    this.repoContext = { test: true };
    this.callLog = [];
    this.coordinationAttempts = [];
    
    // Skip loadRepoContext and start for testing
  }

  // Override callAI to track what's actually happening
  async callAI(role, prompt) {
    this.callLog.push({
      role,
      prompt: prompt.substring(0, 100),
      timestamp: Date.now()
    });

    console.log(`🧪 TEST: ${role} called with: ${prompt.substring(0, 60)}...`);

    // Return mock response
    const response = mockResponses[role] || `Mock response from ${role}`;
    return {
      result: response,
      cost: 0.001
    };
  }

  // Track coordination attempts
  async sendTask(role, task) {
    this.coordinationAttempts.push({
      role,
      task: task.substring(0, 80),
      timestamp: Date.now()
    });

    return super.sendTask(role, task);
  }
}

async function testCoordination() {
  console.log('🧪 Starting coordination tests...\n');

  const continuum = new TestContinuum();
  
  // Test 1: Does PlannerAI mention coordination?
  console.log('Test 1: Planning task that mentions coordination');
  const result1 = await continuum.intelligentRoute('coordinate with CodeAI to implement the modular agent factory system');
  
  console.log('\n📊 Test 1 Results:');
  console.log(`- Calls made: ${continuum.callLog.length}`);
  console.log(`- Coordination attempts: ${continuum.coordinationAttempts.length}`);
  console.log(`- Is coordination: ${result1.coordination}`);
  
  if (result1.coordination) {
    console.log(`- Responses: ${result1.responses.length}`);
    result1.responses.forEach((resp, i) => {
      console.log(`  ${i+1}. ${resp.role} (${resp.type}): ${resp.result.substring(0, 50)}...`);
    });
  }

  // Test 2: Check the call log
  console.log('\n📋 Call Log Analysis:');
  continuum.callLog.forEach((call, i) => {
    console.log(`${i+1}. ${call.role}: ${call.prompt}...`);
  });

  // Test 3: Verify actual coordination happened
  console.log('\n🔍 Coordination Analysis:');
  const plannerCalls = continuum.callLog.filter(call => call.role === 'PlannerAI');
  const codeAICalls = continuum.callLog.filter(call => call.role === 'CodeAI');
  
  console.log(`- PlannerAI calls: ${plannerCalls.length}`);
  console.log(`- CodeAI calls: ${codeAICalls.length}`);
  
  if (plannerCalls.length > 0 && codeAICalls.length > 0) {
    console.log('✅ COORDINATION DETECTED: Both PlannerAI and CodeAI were called');
    
    // Check if CodeAI received PlannerAI's output
    const codeAIPrompt = codeAICalls[0].prompt.toLowerCase();
    if (codeAIPrompt.includes('based on') || codeAIPrompt.includes('plannerai')) {
      console.log('✅ HANDOFF DETECTED: CodeAI received PlannerAI context');
    } else {
      console.log('❌ NO HANDOFF: CodeAI did not receive PlannerAI context');
    }
  } else {
    console.log('❌ NO COORDINATION: Only one AI was called');
  }

  // Test 4: Check if the routing condition is working
  console.log('\n🎯 Routing Test:');
  const task = 'coordinate with CodeAI to implement the modular agent factory system';
  const taskLower = task.toLowerCase();
  
  console.log(`Task: "${task}"`);
  console.log(`Contains "coordinate": ${taskLower.includes('coordinate')}`);
  console.log(`Contains "codeai": ${taskLower.includes('codeai')}`);
  console.log(`Should trigger coordination: ${taskLower.includes('coordinate') && taskLower.includes('codeai')}`);
}

// Run the test
testCoordination().catch(console.error);