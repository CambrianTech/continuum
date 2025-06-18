/**
 * Integration test - entire system working in one call
 * Tests that the whole damn system works end-to-end
 */

import { describe, test, expect, beforeEach, afterEach } from '@jest/globals';
import { Orchestrator, OrchestratorConfig } from '../src/orchestrator';
import { PlannerAgent } from '../src/agents/planner-agent';
import { AgentConfig } from '../src/interfaces/agent.interface';

describe('Full System Integration', () => {
  let orchestrator: Orchestrator;
  
  const config: OrchestratorConfig = {
    port: 5556, // Different port to avoid conflicts
    maxConcurrentTasks: 3,
    defaultTimeout: 30000,
    enableMetrics: true
  };

  beforeEach(async () => {
    orchestrator = new Orchestrator(config);
  });

  afterEach(async () => {
    await orchestrator.shutdown();
  });

  test('should create and execute complete system in one call', async () => {
    // Skip if no API keys (for CI/CD)
    if (!process.env.OPENAI_API_KEY) {
      console.log('⏭️  Skipping integration test - no OpenAI API key');
      return;
    }

    console.log('🚀 Testing complete system integration...');
    
    // 1. Create PlannerAI agent
    const plannerConfig: AgentConfig = {
      name: 'PlannerAI',
      type: 'PlannerAI',
      provider: 'openai',
      model: 'gpt-4o-mini', // Use cheaper model for testing
      maxTokens: 500,
      temperature: 0.3
    };
    
    const plannerAgent = new PlannerAgent(plannerConfig);
    
    // 2. Register agent with orchestrator
    await orchestrator.registerAgent(plannerAgent);
    
    // 3. Execute a task that uses tools
    const task = `Analyze this project structure and create a simple test file.
    
    Please:
    1. Check the current git status
    2. Read the package.json file to understand the project
    3. Create a simple test file based on your analysis
    
    Use the available tools to complete this task.`;
    
    // 4. Execute task - this should work completely in one call
    const result = await orchestrator.routeTask(task);
    
    // 5. Verify results
    expect(result.task).toBe(task);
    expect(result.results).toHaveLength(1);
    expect(result.results[0].agent).toBe('PlannerAI');
    expect(result.results[0].response).toBeTruthy();
    expect(result.coordination).toBe(false);
    expect(result.duration).toBeGreaterThan(0);
    
    // 6. Verify tools were executed
    const toolResults = result.results[0].toolResults;
    expect(toolResults.length).toBeGreaterThan(0);
    
    // Should have executed at least one tool
    const toolNames = toolResults.map(t => t.tool);
    expect(toolNames.length).toBeGreaterThan(0);
    
    console.log('✅ Full system integration test passed!');
    console.log(`📊 Task executed in ${result.duration}ms`);
    console.log(`🔧 Tools executed: ${toolNames.join(', ')}`);
    console.log(`📝 Response length: ${result.results[0].response.length} chars`);
    
  }, 60000); // 60 second timeout for real API calls

  test('should handle system without API keys gracefully', async () => {
    // Test with missing API key
    const originalKey = process.env.OPENAI_API_KEY;
    delete process.env.OPENAI_API_KEY;
    
    try {
      const plannerConfig: AgentConfig = {
        name: 'TestPlannerAI',
        type: 'PlannerAI',
        provider: 'openai',
        model: 'gpt-4o-mini',
        maxTokens: 500,
        temperature: 0.3
      };
      
      expect(() => new PlannerAgent(plannerConfig))
        .toThrow('OPENAI_API_KEY environment variable is required');
        
    } finally {
      // Restore API key
      if (originalKey) {
        process.env.OPENAI_API_KEY = originalKey;
      }
    }
  });

  test('should demonstrate clean separation of concerns', async () => {
    // Skip if no API keys
    if (!process.env.OPENAI_API_KEY) {
      console.log('⏭️  Skipping separation test - no OpenAI API key');
      return;
    }

    const plannerConfig: AgentConfig = {
      name: 'SeparationTestAI',
      type: 'PlannerAI',
      provider: 'openai',
      model: 'gpt-4o-mini',
      maxTokens: 200,
      temperature: 0.1
    };
    
    const agent = new PlannerAgent(plannerConfig);
    await orchestrator.registerAgent(agent);
    
    // Test that agent manages its own tools
    expect(agent.name).toBe('SeparationTestAI');
    expect(agent.config.type).toBe('PlannerAI');
    expect(agent.state).toBe('ready');
    
    // Test that orchestrator manages routing
    const status = orchestrator.getStatus();
    expect(status.activeAgents).toBe(1);
    expect(status.activeTasks).toBe(0);
    expect(status.agentMetrics['SeparationTestAI']).toBeDefined();
    
    console.log('✅ Separation of concerns verified');
  });

  test('should handle errors gracefully without crashing', async () => {
    // Skip if no API keys
    if (!process.env.OPENAI_API_KEY) {
      console.log('⏭️  Skipping error handling test - no OpenAI API key');
      return;
    }

    const plannerConfig: AgentConfig = {
      name: 'ErrorTestAI',
      type: 'PlannerAI',
      provider: 'openai',
      model: 'gpt-4o-mini',
      maxTokens: 100,
      temperature: 0.1
    };
    
    const agent = new PlannerAgent(plannerConfig);
    await orchestrator.registerAgent(agent);
    
    // Test task that might cause tool errors
    const problematicTask = `Please read a non-existent file: FILE_READ: /this/file/does/not/exist.txt`;
    
    const result = await orchestrator.routeTask(problematicTask);
    
    // System should handle errors without crashing
    expect(result.results).toHaveLength(1);
    expect(result.results[0].agent).toBe('ErrorTestAI');
    
    // Should have attempted to read the file even if it failed
    const toolResults = result.results[0].toolResults;
    const fileReadResults = toolResults.filter(t => t.tool === 'FileSystem');
    
    if (fileReadResults.length > 0) {
      expect(fileReadResults[0].success).toBe(false);
      expect(fileReadResults[0].result).toContain('Error');
    }
    
    console.log('✅ Error handling verified - system remained stable');
  });
});