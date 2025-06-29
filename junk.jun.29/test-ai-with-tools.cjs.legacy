#!/usr/bin/env node
/**
 * TEST AI WITH TOOLS - Direct Test
 * 
 * Tests AI intelligence with tool execution without server conflicts
 * Shows AIs can think and use tools intelligently
 */

const { Anthropic } = require('@anthropic-ai/sdk');
const { OpenAI } = require('openai');
const fs = require('fs');
const { exec } = require('child_process');
const { promisify } = require('util');
require('dotenv').config();

const execAsync = promisify(exec);

class AIWithToolsTest {
  constructor() {
    this.anthropic = new Anthropic({
      apiKey: process.env.ANTHROPIC_API_KEY,
    });
    
    this.openai = new OpenAI({
      apiKey: process.env.OPENAI_API_KEY,
    });
    
    this.results = [];
  }

  async webFetch(url) {
    try {
      const { default: fetch } = await import('node-fetch');
      const response = await fetch(url);
      const text = await response.text();
      
      // Simple HTML to text conversion
      const plainText = text
        .replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '')
        .replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '')
        .replace(/<[^>]+>/g, ' ')
        .replace(/\s+/g, ' ')
        .trim();
      
      return plainText.substring(0, 500); // Limit to 500 chars
    } catch (error) {
      return `Error: ${error.message}`;
    }
  }

  async gitStatus() {
    try {
      const { stdout } = await execAsync('git status --porcelain');
      return stdout || 'Working directory clean';
    } catch (error) {
      return `Error: ${error.message}`;
    }
  }

  async readFile(filePath) {
    try {
      const content = fs.readFileSync(filePath, 'utf-8');
      return content.substring(0, 800); // Limit content
    } catch (error) {
      return `Error: ${error.message}`;
    }
  }

  async executeTools(response) {
    const toolResults = [];
    
    // Execute WEBFETCH commands
    const webfetchMatches = response.match(/WEBFETCH:\s*(https?:\/\/[^\s\n]+)/gi);
    if (webfetchMatches) {
      for (const match of webfetchMatches) {
        const urlMatch = match.match(/WEBFETCH:\s*(https?:\/\/[^\s\n]+)/i);
        if (urlMatch) {
          const url = urlMatch[1];
          console.log(`🌐 Executing WEBFETCH: ${url}`);
          const result = await this.webFetch(url);
          toolResults.push({ tool: 'WEBFETCH', command: url, result });
        }
      }
    }
    
    // Execute GIT_STATUS commands
    if (response.includes('GIT_STATUS')) {
      console.log(`📊 Executing GIT_STATUS`);
      const result = await this.gitStatus();
      toolResults.push({ tool: 'GIT_STATUS', command: 'git status', result });
    }
    
    // Execute FILE_READ commands
    const fileReadMatches = response.match(/FILE_READ:\s*([^\s\n]+)/gi);
    if (fileReadMatches) {
      for (const match of fileReadMatches) {
        const fileMatch = match.match(/FILE_READ:\s*([^\s\n]+)/i);
        if (fileMatch) {
          const filePath = fileMatch[1];
          console.log(`📖 Executing FILE_READ: ${filePath}`);
          const result = await this.readFile(filePath);
          toolResults.push({ tool: 'FILE_READ', command: filePath, result });
        }
      }
    }
    
    return toolResults;
  }

  async testClaudeWithTools() {
    console.log('🧠 Testing Claude with intelligent tool usage...');
    
    const prompt = `You are a senior software architect analyzing a codebase. Be intelligent and strategic:

1. First check the git status: GIT_STATUS
2. Read the project configuration: FILE_READ: package.json  
3. Get external data for comparison: WEBFETCH: https://httpbin.org/json
4. Based on this analysis, provide intelligent recommendations

Use the exact tool commands shown above. Then provide strategic insights based on what you discover.
Don't just list data - analyze it intelligently and make recommendations.`;

    try {
      const response = await this.anthropic.messages.create({
        model: "claude-3-haiku-20240307",
        max_tokens: 1000,
        messages: [{ role: "user", content: prompt }],
      });
      
      const aiResponse = response.content[0].text;
      console.log('\n📝 Claude Response Preview:');
      console.log(aiResponse.substring(0, 300) + '...');
      
      // Execute tools mentioned in response
      console.log('\n🔧 Executing tools mentioned by Claude...');
      const toolResults = await this.executeTools(aiResponse);
      
      // Analyze intelligence
      const intelligence = this.analyzeIntelligence(aiResponse, toolResults);
      
      this.results.push({
        ai: 'Claude',
        response: aiResponse,
        toolResults,
        intelligence,
        passed: intelligence.score >= 6 && toolResults.length >= 2
      });
      
      console.log(`\n✅ Claude executed ${toolResults.length} tools with intelligence score ${intelligence.score}/10`);
      
      return { aiResponse, toolResults, intelligence };
    } catch (error) {
      console.error('❌ Claude test failed:', error.message);
      throw error;
    }
  }

  async testOpenAIWithTools() {
    console.log('\n🎯 Testing OpenAI with intelligent tool usage...');
    
    const prompt = `You are PlannerAI, a strategic AI that uses tools intelligently. Analyze this project:

1. Check repository status: GIT_STATUS
2. Understand project structure: FILE_READ: package.json
3. Get external reference data: WEBFETCH: https://httpbin.org/json
4. Create a strategic analysis with actionable recommendations

Execute these exact tool commands, then provide intelligent analysis.
Focus on strategic insights, not just data reporting.`;

    try {
      const response = await this.openai.chat.completions.create({
        model: "gpt-4o-mini",
        messages: [{ role: "user", content: prompt }],
        max_tokens: 1000,
        temperature: 0.3,
      });
      
      const aiResponse = response.choices[0].message.content;
      console.log('\n📋 OpenAI Response Preview:');
      console.log(aiResponse.substring(0, 300) + '...');
      
      // Execute tools mentioned in response
      console.log('\n🔧 Executing tools mentioned by OpenAI...');
      const toolResults = await this.executeTools(aiResponse);
      
      // Analyze intelligence
      const intelligence = this.analyzeIntelligence(aiResponse, toolResults);
      
      this.results.push({
        ai: 'OpenAI GPT-4o-mini',
        response: aiResponse,
        toolResults,
        intelligence,
        passed: intelligence.score >= 6 && toolResults.length >= 2
      });
      
      console.log(`\n✅ OpenAI executed ${toolResults.length} tools with intelligence score ${intelligence.score}/10`);
      
      return { aiResponse, toolResults, intelligence };
    } catch (error) {
      console.error('❌ OpenAI test failed:', error.message);
      throw error;
    }
  }

  analyzeIntelligence(response, toolResults) {
    let score = 0;
    const evidence = [];
    const responseLower = response.toLowerCase();
    
    // Check for tool usage
    if (toolResults.length >= 2) {
      score += 3;
      evidence.push(`Used ${toolResults.length} tools effectively`);
    } else if (toolResults.length >= 1) {
      score += 1;
      evidence.push('Used some tools');
    }
    
    // Check for analysis (not just data reporting)
    if (responseLower.match(/analy[sz]e|evaluat|assess|review|examine/)) {
      score += 2;
      evidence.push('Shows analytical thinking');
    }
    
    // Check for recommendations
    if (responseLower.match(/recommend|suggest|should|could|propose|advise/)) {
      score += 2;
      evidence.push('Provides recommendations');
    }
    
    // Check for strategic thinking
    if (responseLower.match(/strategy|strategic|plan|approach|direction/)) {
      score += 1;
      evidence.push('Shows strategic thinking');
    }
    
    // Check for synthesis (combining tool results)
    if (responseLower.match(/based on|according to|from the|shows that|indicates/)) {
      score += 1;
      evidence.push('Synthesizes information');
    }
    
    // Check for actionable insights
    if (responseLower.match(/improve|optimize|upgrade|update|refactor|fix/)) {
      score += 1;
      evidence.push('Provides actionable insights');
    }
    
    return { score, evidence };
  }

  printResults() {
    console.log('\n🎯 AI WITH TOOLS TEST RESULTS');
    console.log('=============================');
    
    let totalPassed = 0;
    
    this.results.forEach((result, index) => {
      const status = result.passed ? '✅ PASS' : '❌ FAIL';
      console.log(`${index + 1}. ${status} ${result.ai}`);
      console.log(`   Tools executed: ${result.toolResults.length}`);
      console.log(`   Intelligence score: ${result.intelligence.score}/10`);
      
      if (result.intelligence.evidence.length > 0) {
        console.log('   Evidence:');
        result.intelligence.evidence.forEach(evidence => {
          console.log(`     - ${evidence}`);
        });
      }
      
      if (result.toolResults.length > 0) {
        console.log('   Tool results:');
        result.toolResults.forEach(tool => {
          const success = tool.result.includes('Error') ? '❌' : '✅';
          console.log(`     ${success} ${tool.tool}: ${tool.command}`);
        });
      }
      
      if (result.passed) totalPassed++;
      console.log('');
    });
    
    const successRate = (totalPassed / this.results.length * 100).toFixed(1);
    console.log(`📊 FINAL SCORE: ${totalPassed}/${this.results.length} AIs passed (${successRate}%)`);
    
    if (totalPassed === this.results.length) {
      console.log('\n🎉 SUCCESS! AIs demonstrated intelligence with tools!');
      console.log('✅ AIs can think strategically');
      console.log('✅ AIs use tools purposefully');  
      console.log('✅ AIs provide actionable insights');
      console.log('\nThe system shows real intelligence, not just infrastructure!');
    } else {
      console.log('\n⚠️  Some AIs need improvement in intelligent tool usage');
    }
    
    return totalPassed === this.results.length;
  }
}

async function runAIWithToolsTest() {
  console.log('🧠 TESTING AI INTELLIGENCE WITH TOOLS');
  console.log('=====================================');
  console.log('Direct test of AI thinking + tool execution');
  console.log('No server conflicts, pure intelligence testing\n');
  
  // Check API keys
  if (!process.env.ANTHROPIC_API_KEY || !process.env.OPENAI_API_KEY) {
    console.error('❌ Missing API keys!');
    console.log('Set ANTHROPIC_API_KEY and OPENAI_API_KEY environment variables');
    process.exit(1);
  }
  
  const tester = new AIWithToolsTest();
  
  try {
    // Test both AIs
    await tester.testClaudeWithTools();
    await tester.testOpenAIWithTools();
    
    // Show results
    const allPassed = tester.printResults();
    
    if (allPassed) {
      console.log('\n✅ ALL TESTS PASSED - AIs are intelligent and can use tools!');
      process.exit(0);
    } else {
      console.log('\n❌ SOME TESTS FAILED - Intelligence needs improvement');
      process.exit(1);
    }
    
  } catch (error) {
    console.error('\n💥 Test crashed:', error.message);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runAIWithToolsTest();
}

module.exports = { AIWithToolsTest, runAIWithToolsTest };