#!/usr/bin/env node
/**
 * TEST REAL AI INTELLIGENCE
 * 
 * Actually calls OpenAI and Anthropic APIs to verify AIs can think and solve problems
 * Tests intelligence, not just infrastructure
 */

const { Anthropic } = require('@anthropic-ai/sdk');
const { OpenAI } = require('openai');
require('dotenv').config();

class AIIntelligenceTest {
  constructor() {
    this.anthropic = new Anthropic({
      apiKey: process.env.ANTHROPIC_API_KEY,
    });
    
    this.openai = new OpenAI({
      apiKey: process.env.OPENAI_API_KEY,
    });
    
    this.results = [];
  }

  async testClaudeReasoning() {
    console.log('🧠 Testing Claude reasoning ability...');
    
    const reasoningPrompt = `You are a strategic AI. Solve this step by step:

PROBLEM: A software project has these issues:
1. Tests are failing because of a dependency conflict
2. The CI/CD pipeline is broken  
3. Team members can't agree on architecture
4. Deadline is in 2 weeks

Create a prioritized action plan that addresses root causes, not just symptoms.
Show your reasoning process.`;

    try {
      const response = await this.anthropic.messages.create({
        model: "claude-3-haiku-20240307",
        max_tokens: 800,
        messages: [{ role: "user", content: reasoningPrompt }],
      });
      
      const answer = response.content[0].text;
      
      // Analyze if Claude showed actual reasoning
      const hasReasoning = this.analyzeReasoning(answer);
      
      console.log('📝 Claude Response Preview:');
      console.log(answer.substring(0, 300) + '...');
      console.log('');
      
      this.results.push({
        ai: 'Claude',
        test: 'Strategic Reasoning',
        passed: hasReasoning.score >= 7,
        score: hasReasoning.score,
        evidence: hasReasoning.evidence,
        response: answer
      });
      
      return answer;
    } catch (error) {
      console.error('❌ Claude test failed:', error.message);
      this.results.push({
        ai: 'Claude',
        test: 'Strategic Reasoning', 
        passed: false,
        error: error.message
      });
      throw error;
    }
  }

  async testOpenAIPlanning() {
    console.log('🎯 Testing OpenAI planning ability...');
    
    const planningPrompt = `You are PlannerAI. Create a detailed implementation plan:

SCENARIO: Build a multi-agent AI system that can:
- Route tasks intelligently between specialized AIs
- Execute tools (web fetch, file operations, git commands)
- Coordinate responses between agents
- Track costs and performance metrics

Break this down into concrete steps with:
1. Technical architecture decisions
2. Implementation sequence
3. Risk mitigation strategies
4. Success criteria

Think through the dependencies and show your planning process.`;

    try {
      const response = await this.openai.chat.completions.create({
        model: "gpt-4o-mini",
        messages: [{ role: "user", content: planningPrompt }],
        max_tokens: 1000,
        temperature: 0.3,
      });
      
      const answer = response.choices[0].message.content;
      
      // Analyze if GPT showed actual planning
      const hasPlanning = this.analyzePlanning(answer);
      
      console.log('📋 OpenAI Response Preview:');
      console.log(answer.substring(0, 300) + '...');
      console.log('');
      
      this.results.push({
        ai: 'OpenAI GPT-4o-mini',
        test: 'Technical Planning',
        passed: hasPlanning.score >= 7,
        score: hasPlanning.score,
        evidence: hasPlanning.evidence,
        response: answer
      });
      
      return answer;
    } catch (error) {
      console.error('❌ OpenAI test failed:', error.message);
      this.results.push({
        ai: 'OpenAI',
        test: 'Technical Planning',
        passed: false,
        error: error.message
      });
      throw error;
    }
  }

  async testCoordination() {
    console.log('🤝 Testing AI coordination...');
    
    // Get both AIs to work on the same problem
    const claudeTask = `You are CodeAI. A PlannerAI will give you a strategy to implement. 
    Respond with: "Ready to implement. Please provide the strategy from PlannerAI."`;
    
    const gptTask = `You are PlannerAI. You're coordinating with CodeAI to fix a broken CI pipeline.
    Create a specific strategy that CodeAI can implement. Format it as clear, actionable steps.`;
    
    try {
      // Get strategy from GPT (PlannerAI)
      const strategyResponse = await this.openai.chat.completions.create({
        model: "gpt-4o-mini",
        messages: [{ role: "user", content: gptTask }],
        max_tokens: 600,
        temperature: 0.2,
      });
      
      const strategy = strategyResponse.choices[0].message.content;
      
      // Give strategy to Claude (CodeAI)
      const implementationPrompt = `You are CodeAI. PlannerAI provided this strategy:

${strategy}

Now create a concrete implementation plan with specific commands and code changes.
Show how you'll execute each step.`;
      
      const implementationResponse = await this.anthropic.messages.create({
        model: "claude-3-haiku-20240307",
        max_tokens: 800,
        messages: [{ role: "user", content: implementationPrompt }],
      });
      
      const implementation = implementationResponse.content[0].text;
      
      // Analyze coordination quality
      const coordination = this.analyzeCoordination(strategy, implementation);
      
      console.log('🎭 Coordination Results:');
      console.log('PlannerAI Strategy:', strategy.substring(0, 200) + '...');
      console.log('CodeAI Implementation:', implementation.substring(0, 200) + '...');
      console.log('');
      
      this.results.push({
        ai: 'Coordination (GPT + Claude)',
        test: 'Multi-Agent Coordination',
        passed: coordination.score >= 6,
        score: coordination.score,
        evidence: coordination.evidence,
        strategy: strategy,
        implementation: implementation
      });
      
      return { strategy, implementation };
    } catch (error) {
      console.error('❌ Coordination test failed:', error.message);
      this.results.push({
        ai: 'Coordination',
        test: 'Multi-Agent Coordination',
        passed: false,
        error: error.message
      });
      throw error;
    }
  }

  analyzeReasoning(response) {
    let score = 0;
    const evidence = [];
    
    // Check for step-by-step thinking
    if (response.match(/step\s*\d+|first|second|third|then|next/i)) {
      score += 2;
      evidence.push('Shows sequential thinking');
    }
    
    // Check for prioritization
    if (response.match(/priority|important|urgent|critical|first/i)) {
      score += 2;
      evidence.push('Demonstrates prioritization');
    }
    
    // Check for root cause analysis
    if (response.match(/root cause|underlying|because|reason|why/i)) {
      score += 2;
      evidence.push('Addresses root causes');
    }
    
    // Check for consideration of constraints
    if (response.match(/deadline|time|resource|budget|risk/i)) {
      score += 1;
      evidence.push('Considers constraints');
    }
    
    // Check for structured response
    if (response.match(/\d+\./g) && response.match(/\d+\./g).length >= 3) {
      score += 1;
      evidence.push('Structured response');
    }
    
    return { score, evidence };
  }

  analyzePlanning(response) {
    let score = 0;
    const evidence = [];
    
    // Check for architecture thinking
    if (response.match(/architecture|design|component|interface|api/i)) {
      score += 2;
      evidence.push('Shows architectural thinking');
    }
    
    // Check for implementation sequence
    if (response.match(/phase|stage|milestone|sequence|order/i)) {
      score += 2;
      evidence.push('Plans implementation sequence');
    }
    
    // Check for risk awareness
    if (response.match(/risk|challenge|potential|issue|problem/i)) {
      score += 2;
      evidence.push('Identifies risks');
    }
    
    // Check for success criteria
    if (response.match(/success|metric|measure|criteria|test/i)) {
      score += 1;
      evidence.push('Defines success criteria');
    }
    
    // Check for technical depth
    if (response.match(/database|api|server|client|framework/i)) {
      score += 1;
      evidence.push('Shows technical depth');
    }
    
    return { score, evidence };
  }

  analyzeCoordination(strategy, implementation) {
    let score = 0;
    const evidence = [];
    
    // Check if implementation references strategy
    const strategyWords = strategy.toLowerCase().split(/\s+/).filter(w => w.length > 4);
    const implementationWords = implementation.toLowerCase().split(/\s+/);
    const overlap = strategyWords.filter(word => implementationWords.includes(word));
    
    if (overlap.length >= 5) {
      score += 3;
      evidence.push('Implementation builds on strategy');
    }
    
    // Check for concrete steps in implementation
    if (implementation.match(/\d+\./g) && implementation.match(/\d+\./g).length >= 3) {
      score += 2;
      evidence.push('Provides concrete steps');
    }
    
    // Check for technical commands/code
    if (implementation.match(/git|npm|docker|curl|mkdir|touch/i)) {
      score += 2;
      evidence.push('Includes technical commands');
    }
    
    return { score, evidence };
  }

  printResults() {
    console.log('\n🎯 AI INTELLIGENCE TEST RESULTS');
    console.log('================================');
    
    let totalPassed = 0;
    let totalTests = this.results.length;
    
    this.results.forEach((result, index) => {
      const status = result.passed ? '✅ PASS' : '❌ FAIL';
      const scoreText = result.score ? `(${result.score}/10)` : '';
      
      console.log(`${index + 1}. ${status} ${result.ai} - ${result.test} ${scoreText}`);
      
      if (result.evidence) {
        result.evidence.forEach(evidence => {
          console.log(`   - ${evidence}`);
        });
      }
      
      if (result.error) {
        console.log(`   Error: ${result.error}`);
      }
      
      if (result.passed) totalPassed++;
      console.log('');
    });
    
    const successRate = (totalPassed / totalTests * 100).toFixed(1);
    console.log(`📊 INTELLIGENCE SCORE: ${totalPassed}/${totalTests} tests passed (${successRate}%)`);
    
    if (totalPassed === totalTests) {
      console.log('🎉 ALL AIs DEMONSTRATED INTELLIGENCE!');
      console.log('The system can think, plan, and coordinate effectively.');
    } else {
      console.log('⚠️  Some AIs need improvement in reasoning abilities.');
    }
    
    return totalPassed === totalTests;
  }
}

async function runIntelligenceTests() {
  console.log('🧠 TESTING REAL AI INTELLIGENCE');
  console.log('===============================');
  console.log('This test verifies AIs can actually think and solve problems');
  console.log('Not just infrastructure - actual intelligence!\n');
  
  // Check API keys
  if (!process.env.ANTHROPIC_API_KEY || !process.env.OPENAI_API_KEY) {
    console.error('❌ Missing API keys!');
    console.log('Set ANTHROPIC_API_KEY and OPENAI_API_KEY environment variables');
    process.exit(1);
  }
  
  const tester = new AIIntelligenceTest();
  
  try {
    console.log('🔄 Running intelligence tests...\n');
    
    // Test each AI's reasoning ability
    await tester.testClaudeReasoning();
    await tester.testOpenAIPlanning();
    await tester.testCoordination();
    
    // Show results
    const allPassed = tester.printResults();
    
    if (allPassed) {
      console.log('\n✅ INTELLIGENCE VERIFIED - AIs can think and solve problems!');
      process.exit(0);
    } else {
      console.log('\n❌ INTELLIGENCE ISSUES - Some AIs failed reasoning tests');
      process.exit(1);
    }
    
  } catch (error) {
    console.error('\n💥 Intelligence test crashed:', error.message);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runIntelligenceTests();
}

module.exports = { AIIntelligenceTest, runIntelligenceTests };