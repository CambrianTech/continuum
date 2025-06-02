#!/usr/bin/env npx ts-node
/**
 * Demo: Complete TypeScript System Working in One Call
 * 
 * Shows the entire system working end-to-end:
 * - Clean agent initialization
 * - Tool loading and management
 * - Task execution with real AI responses
 * - Proper separation of concerns throughout
 */

import { Orchestrator } from './src/orchestrator';
import { PlannerAgent } from './src/agents/planner-agent';
import { AgentConfig } from './src/interfaces/agent.interface';

async function demonstrateCompleteSystem() {
  console.log('🌟 COMPLETE TYPESCRIPT SYSTEM DEMO');
  console.log('==================================');
  console.log('Demonstrating the entire system working in one call');
  console.log('No monolithic disasters, clean separation of concerns\n');

  try {
    // 1. Create orchestrator with clean configuration
    console.log('🔧 Creating orchestrator...');
    const orchestrator = new Orchestrator({
      port: 5557,
      maxConcurrentTasks: 3,
      defaultTimeout: 30000,
      enableMetrics: true
    });

    // 2. Create agent with proper configuration
    console.log('🤖 Creating PlannerAI agent...');
    const agentConfig: AgentConfig = {
      name: 'DemoPlannerAI',
      type: 'PlannerAI',
      provider: 'openai',
      model: 'gpt-4o-mini',
      maxTokens: 800,
      temperature: 0.3
    };

    const plannerAgent = new PlannerAgent(agentConfig);

    // 3. Register agent (agent manages its own initialization)
    console.log('📝 Registering agent with orchestrator...');
    await orchestrator.registerAgent(plannerAgent);

    console.log('✅ System initialized successfully!');
    console.log(`   - Agent: ${plannerAgent.name} (${plannerAgent.state})`);
    console.log(`   - Orchestrator: ${orchestrator.getStatus().activeAgents} agents ready\n`);

    // 4. Execute a comprehensive task that demonstrates all capabilities
    console.log('🚀 Executing comprehensive task...');
    const task = `Please help me understand this project and create a summary:

1. Check the current git status to see what files have changed
2. Read the package.json to understand project dependencies
3. Fetch information from https://httpbin.org/json to test web capabilities
4. Create a brief project analysis based on your findings

Use all available tools to complete this comprehensive analysis.`;

    console.log(`📝 Task: ${task.substring(0, 100)}...\n`);

    // 5. Execute task - everything happens in one call
    const startTime = Date.now();
    const result = await orchestrator.routeTask(task);
    const duration = Date.now() - startTime;

    // 6. Display results
    console.log('✅ TASK COMPLETED SUCCESSFULLY!');
    console.log('================================');
    console.log(`⏱️  Duration: ${duration}ms`);
    console.log(`🤖 Agent: ${result.results[0].agent}`);
    console.log(`🔧 Tools executed: ${result.results[0].toolResults.length}`);
    console.log(`📊 Coordination: ${result.coordination ? 'Yes' : 'No'}`);
    console.log(`📝 Response length: ${result.results[0].response.length} characters\n`);

    // 7. Show tool execution details
    if (result.results[0].toolResults.length > 0) {
      console.log('🔧 TOOL EXECUTION DETAILS:');
      console.log('--------------------------');
      result.results[0].toolResults.forEach((toolResult, index) => {
        const status = toolResult.success ? '✅' : '❌';
        const resultPreview = toolResult.result.substring(0, 80) + 
          (toolResult.result.length > 80 ? '...' : '');
        console.log(`${index + 1}. ${status} ${toolResult.tool}: ${toolResult.command}`);
        console.log(`   Result: ${resultPreview}`);
        console.log(`   Duration: ${toolResult.duration || 0}ms\n`);
      });
    }

    // 8. Show AI response preview
    console.log('🧠 AI RESPONSE PREVIEW:');
    console.log('----------------------');
    const responsePreview = result.results[0].response.substring(0, 300) + 
      (result.results[0].response.length > 300 ? '...' : '');
    console.log(responsePreview);
    console.log('');

    // 9. Show system metrics
    const status = orchestrator.getStatus();
    console.log('📊 SYSTEM METRICS:');
    console.log('------------------');
    console.log(`Active agents: ${status.activeAgents}`);
    console.log(`Active tasks: ${status.activeTasks}`);
    console.log(`Agent metrics:`, JSON.stringify(status.agentMetrics, null, 2));

    // 10. Clean shutdown
    console.log('\n🔄 Shutting down system...');
    await orchestrator.shutdown();
    console.log('✅ System shutdown complete');

    console.log('\n🎉 DEMO COMPLETED SUCCESSFULLY!');
    console.log('The entire system worked in one call with:');
    console.log('- Clean TypeScript interfaces');
    console.log('- Proper separation of concerns');
    console.log('- Agents managing their own tools');
    console.log('- Tools handling their own I/O');
    console.log('- No monolithic god objects');

  } catch (error) {
    console.error('\n❌ DEMO FAILED:', error.message);
    
    if (error.message.includes('OPENAI_API_KEY')) {
      console.log('\n💡 To run this demo:');
      console.log('1. Set your OPENAI_API_KEY environment variable');
      console.log('2. Run: npm run demo');
    }
    
    process.exit(1);
  }
}

// Run demo if called directly
if (require.main === module) {
  demonstrateCompleteSystem().catch(error => {
    console.error('Demo crashed:', error);
    process.exit(1);
  });
}

export { demonstrateCompleteSystem };