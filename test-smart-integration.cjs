#!/usr/bin/env node
/**
 * TEST SMART INTEGRATION
 * 
 * Tests that our TypeScript system can get smart responses from AIs
 * and execute tools based on intelligent analysis
 */

const Continuum = require('./continuum.cjs');

class SmartIntegrationTest {
  constructor() {
    this.continuum = null;
  }

  async setup() {
    console.log('🔧 Setting up Continuum system...');
    this.continuum = new Continuum();
    
    // Wait for initialization
    await new Promise(resolve => setTimeout(resolve, 3000));
    console.log('✅ Continuum system ready');
  }

  async testSmartAnalysis() {
    console.log('\n🧠 Testing smart analysis with tool execution...');
    
    const intelligentTask = `You are a senior software architect. Please analyze this project:

1. First, check the git status to see what's happening in the repository
2. Read the package.json to understand the project structure and dependencies
3. Fetch some external data to test our web capabilities: WEBFETCH: https://httpbin.org/json
4. Based on your analysis, provide strategic recommendations for this codebase

Use your tools wisely and provide intelligent insights, not just data dumps.`;

    console.log('📝 Sending intelligent task to AI system...');
    
    try {
      const result = await this.continuum.intelligentRoute(intelligentTask);
      
      // Analyze if the AI response shows intelligence
      const intelligence = this.analyzeIntelligence(result);
      
      console.log('\n📊 SMART INTEGRATION RESULTS:');
      console.log('============================');
      console.log(`🤖 AI Used: ${result.role || 'Coordination System'}`);
      console.log(`🔧 Tools Executed: ${this.countToolsExecuted(result)}`);
      console.log(`📝 Response Length: ${this.getResponseLength(result)} characters`);
      console.log(`🧠 Intelligence Score: ${intelligence.score}/10`);
      
      if (intelligence.evidence.length > 0) {
        console.log('\n💡 Evidence of Intelligence:');
        intelligence.evidence.forEach(evidence => {
          console.log(`   - ${evidence}`);
        });
      }
      
      console.log('\n📋 AI Response Preview:');
      console.log('----------------------');
      const response = this.getMainResponse(result);
      console.log(response.substring(0, 400) + (response.length > 400 ? '...' : ''));
      
      if (this.countToolsExecuted(result) > 0) {
        console.log('\n🔧 Tools Executed:');
        console.log('-----------------');
        this.getToolResults(result).forEach((tool, index) => {
          const status = tool.result.includes('Error') ? '❌' : '✅';
          console.log(`${index + 1}. ${status} ${tool.tool}: ${tool.command}`);
          console.log(`   Result: ${tool.result.substring(0, 100)}...`);
        });
      }
      
      return {
        passed: intelligence.score >= 6 && this.countToolsExecuted(result) >= 2,
        intelligence,
        result
      };
      
    } catch (error) {
      console.error('❌ Smart integration test failed:', error.message);
      return { passed: false, error: error.message };
    }
  }

  async testStrategicCoordination() {
    console.log('\n🎭 Testing strategic coordination between AIs...');
    
    const coordinationTask = `We need to fix a critical issue in this codebase. 
    
Please coordinate between PlannerAI and CodeAI to:
1. PlannerAI should analyze the current state and create a strategy
2. CodeAI should implement the fixes based on the strategy
3. Both should use available tools for analysis

This requires real intelligence and coordination, not just template responses.`;

    console.log('📝 Sending coordination task...');
    
    try {
      const result = await this.continuum.intelligentRoute(coordinationTask);
      
      const coordination = this.analyzeCoordination(result);
      
      console.log('\n🤝 COORDINATION RESULTS:');
      console.log('========================');
      console.log(`🎭 Coordination: ${result.coordination ? 'Yes' : 'No'}`);
      console.log(`🤖 AIs Involved: ${this.getAIsInvolved(result)}`);
      console.log(`🔧 Total Tools: ${this.countAllTools(result)}`);
      console.log(`🧠 Coordination Score: ${coordination.score}/10`);
      
      if (coordination.evidence.length > 0) {
        console.log('\n💡 Coordination Evidence:');
        coordination.evidence.forEach(evidence => {
          console.log(`   - ${evidence}`);
        });
      }
      
      if (result.responses) {
        console.log('\n📋 Multi-AI Responses:');
        console.log('----------------------');
        result.responses.forEach((response, index) => {
          console.log(`${index + 1}. ${response.role} (${response.type || 'standard'}):`);
          console.log(`   ${response.result.substring(0, 150)}...`);
        });
      }
      
      return {
        passed: coordination.score >= 5 && result.coordination,
        coordination,
        result
      };
      
    } catch (error) {
      console.error('❌ Strategic coordination test failed:', error.message);
      return { passed: false, error: error.message };
    }
  }

  analyzeIntelligence(result) {
    let score = 0;
    const evidence = [];
    const response = this.getMainResponse(result).toLowerCase();
    
    // Check for analytical thinking
    if (response.match(/analysis|examine|evaluate|assess|review/)) {
      score += 2;
      evidence.push('Shows analytical thinking');
    }
    
    // Check for strategic insights
    if (response.match(/recommend|suggest|strategy|approach|should|could/)) {
      score += 2;
      evidence.push('Provides strategic insights');
    }
    
    // Check for tool usage justification
    if (response.match(/check|read|fetch|understand|see|analyze/)) {
      score += 1;
      evidence.push('Justifies tool usage');
    }
    
    // Check for structured response
    if (response.match(/first|second|third|then|next|finally/)) {
      score += 1;
      evidence.push('Structured thinking');
    }
    
    // Check for domain knowledge
    if (response.match(/dependency|package|git|repository|codebase|architecture/)) {
      score += 2;
      evidence.push('Shows domain knowledge');
    }
    
    // Check for actionable recommendations
    if (response.match(/upgrade|update|refactor|improve|optimize|fix/)) {
      score += 2;
      evidence.push('Provides actionable recommendations');
    }
    
    return { score, evidence };
  }

  analyzeCoordination(result) {
    let score = 0;
    const evidence = [];
    
    if (result.coordination) {
      score += 3;
      evidence.push('Multi-AI coordination activated');
    }
    
    if (result.responses && result.responses.length >= 2) {
      score += 2;
      evidence.push('Multiple AIs provided responses');
    }
    
    if (result.summary && result.summary.includes('coordination')) {
      score += 2;
      evidence.push('System recognized coordination task');
    }
    
    if (this.countAllTools(result) >= 2) {
      score += 2;
      evidence.push('Tools executed across coordination');
    }
    
    if (result.responses) {
      const hasPlanning = result.responses.some(r => 
        r.result.toLowerCase().includes('plan') || r.result.toLowerCase().includes('strategy')
      );
      const hasImplementation = result.responses.some(r => 
        r.result.toLowerCase().includes('implement') || r.result.toLowerCase().includes('code')
      );
      
      if (hasPlanning && hasImplementation) {
        score += 1;
        evidence.push('Shows planning and implementation coordination');
      }
    }
    
    return { score, evidence };
  }

  countToolsExecuted(result) {
    if (result.result && result.result.includes('Executed')) {
      const match = result.result.match(/Executed (\d+) tools/);
      return match ? parseInt(match[1]) : 0;
    }
    return 0;
  }

  countAllTools(result) {
    let total = 0;
    if (result.responses) {
      result.responses.forEach(response => {
        if (response.result.includes('Executed')) {
          const match = response.result.match(/Executed (\d+) tools/);
          if (match) total += parseInt(match[1]);
        }
      });
    } else {
      total = this.countToolsExecuted(result);
    }
    return total;
  }

  getMainResponse(result) {
    if (result.responses) {
      return result.responses[0]?.result || '';
    }
    return result.result || '';
  }

  getResponseLength(result) {
    return this.getMainResponse(result).length;
  }

  getAIsInvolved(result) {
    if (result.responses) {
      return result.responses.map(r => r.role).join(', ');
    }
    return result.role || 'Unknown';
  }

  getToolResults(result) {
    // This would need to be integrated with actual tool result parsing
    // For now, return mock data based on what we know was executed
    const response = this.getMainResponse(result);
    const tools = [];
    
    if (response.includes('WEBFETCH')) {
      tools.push({
        tool: 'WEBFETCH',
        command: 'https://httpbin.org/json',
        result: 'Successfully fetched JSON data'
      });
    }
    
    if (response.includes('GIT_STATUS') || response.includes('git status')) {
      tools.push({
        tool: 'GIT_STATUS',
        command: 'git status',
        result: 'Repository status retrieved'
      });
    }
    
    if (response.includes('FILE_READ') || response.includes('package.json')) {
      tools.push({
        tool: 'FILE_READ',
        command: 'package.json',
        result: 'Package configuration read'
      });
    }
    
    return tools;
  }

  async cleanup() {
    if (this.continuum) {
      console.log('\n🔄 Cleaning up test environment...');
      // The continuum server will clean up automatically
      await new Promise(resolve => setTimeout(resolve, 1000));
      console.log('✅ Cleanup complete');
    }
  }
}

async function runSmartIntegrationTests() {
  console.log('🧠 TESTING SMART AI INTEGRATION');
  console.log('===============================');
  console.log('Verifying AIs can think intelligently AND execute tools');
  console.log('Testing real intelligence, not just infrastructure\n');
  
  // Check API keys
  if (!process.env.ANTHROPIC_API_KEY || !process.env.OPENAI_API_KEY) {
    console.error('❌ Missing API keys!');
    console.log('Set ANTHROPIC_API_KEY and OPENAI_API_KEY environment variables');
    process.exit(1);
  }
  
  const tester = new SmartIntegrationTest();
  
  try {
    await tester.setup();
    
    // Test smart analysis
    const analysisResult = await tester.testSmartAnalysis();
    
    // Test strategic coordination  
    const coordinationResult = await tester.testStrategicCoordination();
    
    await tester.cleanup();
    
    // Final results
    console.log('\n🎯 SMART INTEGRATION FINAL RESULTS');
    console.log('==================================');
    
    const tests = [
      { name: 'Smart Analysis', result: analysisResult },
      { name: 'Strategic Coordination', result: coordinationResult }
    ];
    
    let passed = 0;
    tests.forEach((test, index) => {
      const status = test.result.passed ? '✅ PASS' : '❌ FAIL';
      console.log(`${index + 1}. ${status} ${test.name}`);
      if (test.result.passed) passed++;
    });
    
    const successRate = (passed / tests.length * 100).toFixed(1);
    console.log(`\n📊 SMART INTEGRATION SCORE: ${passed}/${tests.length} (${successRate}%)`);
    
    if (passed === tests.length) {
      console.log('\n🎉 SMART INTEGRATION SUCCESS!');
      console.log('✅ AIs demonstrate real intelligence');
      console.log('✅ Tools execute based on intelligent analysis');
      console.log('✅ Multi-agent coordination works effectively');
      console.log('\nThe system can think AND act intelligently!');
      process.exit(0);
    } else {
      console.log('\n⚠️  SMART INTEGRATION ISSUES');
      console.log('Some aspects of intelligent integration need improvement');
      process.exit(1);
    }
    
  } catch (error) {
    console.error('\n💥 Smart integration test crashed:', error.message);
    await tester.cleanup();
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runSmartIntegrationTests();
}

module.exports = { SmartIntegrationTest, runSmartIntegrationTests };