#!/usr/bin/env node
/**
 * CLAUDE COGNITION TEST
 * 
 * Better tests for actual AI cognition that are easy to verify:
 * 1. Pattern completion (if it gets these right, it's thinking)
 * 2. Simple logic (can't be memorized easily)  
 * 3. Novel word problems (tests reasoning)
 * 4. Contradiction detection (tests understanding)
 */

const ClaudeAutoWrapper = require('./claude-auto-wrapper.cjs');
const fs = require('fs');

class ClaudeCognitionTest {
  constructor() {
    this.cognitiveTests = [
      // Pattern completion - requires actual reasoning
      {
        test: "Complete this pattern: A, C, E, G, ?",
        answer: "I",
        explanation: "Every other letter: A(1), C(3), E(5), G(7), I(9)",
        type: "pattern",
        verify: (response) => response.toLowerCase().includes('i')
      },
      
      {
        test: "What comes next: 1, 1, 2, 3, 5, 8, ?",
        answer: "13", 
        explanation: "Fibonacci sequence: each number is sum of previous two",
        type: "pattern",
        verify: (response) => response.includes('13')
      },
      
      // Logic puzzles - can't be easily memorized
      {
        test: "Tom is taller than Jerry. Jerry is taller than Mike. Who is shortest?",
        answer: "Mike",
        explanation: "Tom > Jerry > Mike, so Mike is shortest",
        type: "logic",
        verify: (response) => response.toLowerCase().includes('mike')
      },
      
      // Novel reasoning - tests understanding, not memory
      {
        test: "If a flibber costs 3 glorps and you have 10 glorps, how many flibbers can you buy?",
        answer: "3",
        explanation: "10 ÷ 3 = 3 remainder 1, so 3 complete flibbers",
        type: "reasoning", 
        verify: (response) => response.includes('3') && !response.includes('10')
      },
      
      // Contradiction detection - tests reading comprehension
      {
        test: "John said 'I never lie' but then said 'I sometimes lie'. Is there a problem?",
        answer: "Yes/contradiction",
        explanation: "If he never lies, the second statement is false. If he sometimes lies, the first statement is false.",
        type: "contradiction",
        verify: (response) => {
          const r = response.toLowerCase();
          return r.includes('contradict') || r.includes('problem') || r.includes('paradox') || 
                 (r.includes('yes') && (r.includes('both') || r.includes('cannot')));
        }
      },
      
      // Creative reasoning with constraints
      {
        test: "Name an animal that starts with the last letter of 'elephant'.",
        answer: "Any animal starting with 't'",
        explanation: "Elephant ends with 't', so turtle, tiger, turkey, etc.",
        type: "creative_logic",
        verify: (response) => {
          const r = response.toLowerCase();
          const tAnimals = ['turtle', 'tiger', 'turkey', 'tuna', 'toucan', 'tarantula', 'toad', 'termite'];
          return tAnimals.some(animal => r.includes(animal));
        }
      },
      
      // Spatial reasoning
      {
        test: "If you're facing north and turn 90 degrees to your right, then 180 degrees, which direction are you facing?",
        answer: "West",
        explanation: "North → right 90° → East → 180° → West", 
        type: "spatial",
        verify: (response) => response.toLowerCase().includes('west')
      },
      
      // Math word problem with distractor
      {
        test: "Sarah has 15 apples. She gives away 7 apples and buys 4 oranges. How many apples does she have now?",
        answer: "8",
        explanation: "15 - 7 = 8 apples (oranges are irrelevant)",
        type: "math_reasoning",
        verify: (response) => response.includes('8') && !response.includes('12')
      }
    ];
    
    this.results = [];
    this.wrapper = null;
  }

  async runCognitionTest() {
    console.log('🧠 CLAUDE COGNITION TEST');
    console.log('========================');
    console.log('🎯 Testing actual reasoning ability');
    console.log('🔍 These problems require thinking, not just memory');
    console.log(`📝 ${this.cognitiveTests.length} cognitive challenges prepared`);
    console.log('');

    // First, try the simplest possible test
    await this.testBasicCommunication();
    
    if (this.results.length === 0 || !this.results[0].gotResponse) {
      console.log('💥 FAILED: Cannot even communicate with Claude');
      return;
    }

    // If basic communication works, run cognitive tests
    await this.runAllCognitiveTests();
    this.analyzeCognitionResults();
  }

  async testBasicCommunication() {
    console.log('📡 Testing basic Claude communication...');
    
    this.wrapper = new ClaudeAutoWrapper('cognition-test');
    
    try {
      await this.wrapper.launchClaude('You are a helpful AI assistant. Please answer questions briefly and accurately.');
      
      console.log('⏳ Waiting for Claude to initialize...');
      await this.sleep(3000);
      
      // Send simplest possible test
      console.log('📤 Sending: "What is 2 + 2?"');
      
      const response = await this.getClaudeResponse('What is 2 + 2?', 8000);
      
      if (response && response.trim()) {
        console.log(`📨 Claude responded: "${response}"`);
        
        const hasCorrectAnswer = response.includes('4');
        
        this.results.push({
          test: 'Basic communication',
          expected: '4',
          response: response,
          gotResponse: true,
          correct: hasCorrectAnswer,
          type: 'communication'
        });
        
        if (hasCorrectAnswer) {
          console.log('✅ Basic communication working!');
        } else {
          console.log('⚠️  Got response but answer seems wrong');
        }
      } else {
        console.log('❌ No response received from Claude');
        this.results.push({
          test: 'Basic communication', 
          expected: '4',
          response: null,
          gotResponse: false,
          correct: false,
          type: 'communication'
        });
      }
      
    } catch (error) {
      console.error(`❌ Communication test failed: ${error.message}`);
      this.results.push({
        test: 'Basic communication',
        expected: '4', 
        response: null,
        gotResponse: false,
        correct: false,
        error: error.message,
        type: 'communication'
      });
    }
  }

  async runAllCognitiveTests() {
    console.log('\\n🧠 Running cognitive reasoning tests...');
    
    for (let i = 0; i < this.cognitiveTests.length; i++) {
      const cogTest = this.cognitiveTests[i];
      
      console.log(`\\n🧪 Test ${i + 1}/${this.cognitiveTests.length} (${cogTest.type}):`);
      console.log(`   Question: ${cogTest.test}`);
      
      try {
        const response = await this.getClaudeResponse(cogTest.test, 12000);
        
        if (response && response.trim()) {
          const isCorrect = cogTest.verify(response);
          
          console.log(`   🤖 Claude: "${response}"`);
          console.log(`   ${isCorrect ? '✅ CORRECT' : '❌ INCORRECT'}`);
          if (!isCorrect) {
            console.log(`   💡 Expected: ${cogTest.answer}`);
          }
          
          this.results.push({
            test: cogTest.test,
            expected: cogTest.answer,
            response: response,
            gotResponse: true,
            correct: isCorrect,
            type: cogTest.type,
            explanation: cogTest.explanation
          });
          
        } else {
          console.log('   ❌ No response received');
          this.results.push({
            test: cogTest.test,
            expected: cogTest.answer, 
            response: null,
            gotResponse: false,
            correct: false,
            type: cogTest.type
          });
        }
        
      } catch (error) {
        console.log(`   ❌ Test failed: ${error.message}`);
        this.results.push({
          test: cogTest.test,
          expected: cogTest.answer,
          response: null,
          gotResponse: false,
          correct: false,
          error: error.message,
          type: cogTest.type
        });
      }
      
      // Brief pause between tests
      await this.sleep(2000);
    }
  }

  async getClaudeResponse(question, timeoutMs = 10000) {
    return new Promise((resolve, reject) => {
      let responseBuffer = '';
      let hasReceivedData = false;
      
      const outputHandler = (data) => {
        hasReceivedData = true;
        const text = data.toString();
        responseBuffer += text;
        
        // Claude usually ends responses with newline or stops outputting
        // We'll wait a bit after last output to ensure we get the complete response
      };
      
      // Capture Claude's output
      this.wrapper.claudeProcess.stdout.on('data', outputHandler);
      
      // Send the question
      this.wrapper.sendMessage(question);
      
      // Wait for response with adaptive timeout
      let lastOutputTime = Date.now();
      let checkInterval;
      
      checkInterval = setInterval(() => {
        const now = Date.now();
        
        // If we got data and no new data for 2 seconds, consider response complete
        if (hasReceivedData && (now - lastOutputTime) > 2000) {
          clearInterval(checkInterval);
          this.wrapper.claudeProcess.stdout.removeListener('data', outputHandler);
          resolve(this.cleanResponse(responseBuffer));
          return;
        }
        
        // Overall timeout
        if (now - lastOutputTime > timeoutMs) {
          clearInterval(checkInterval);
          this.wrapper.claudeProcess.stdout.removeListener('data', outputHandler);
          
          if (responseBuffer.trim()) {
            resolve(this.cleanResponse(responseBuffer));
          } else {
            reject(new Error('No response within timeout'));
          }
        }
      }, 500);
      
      // Update last output time when we receive data
      const originalHandler = outputHandler;
      outputHandler = (data) => {
        lastOutputTime = Date.now();
        originalHandler(data);
      };
    });
  }

  cleanResponse(rawResponse) {
    return rawResponse
      .replace(/\\n+/g, ' ')  // Replace newlines with spaces
      .replace(/\\s+/g, ' ')  // Collapse multiple spaces
      .trim()
      .replace(/^(Answer:|Response:|Claude:)\\s*/i, '')  // Remove common prefixes
      .replace(/\\.$/, '')  // Remove trailing period
      .trim();
  }

  analyzeCognitionResults() {
    console.log('\\n\\n🧠 COGNITION TEST ANALYSIS');
    console.log('===========================');
    
    const total = this.results.length;
    const responded = this.results.filter(r => r.gotResponse).length;
    const correct = this.results.filter(r => r.correct).length;
    
    console.log(`📊 Communication Rate: ${responded}/${total} (${(responded/total*100).toFixed(1)}%)`);
    console.log(`🎯 Accuracy Rate: ${correct}/${responded} (${responded > 0 ? (correct/responded*100).toFixed(1) : 0}%)`);
    
    // Determine Claude's cognitive status
    if (responded === 0) {
      console.log('\\n💥 VERDICT: Claude is not communicating at all');
      console.log('🔧 The integration is completely broken');
    } else if (responded < total * 0.5) {
      console.log('\\n⚠️  VERDICT: Claude communication is unreliable'); 
      console.log('🔧 Integration works sometimes but is unstable');
    } else if (correct < responded * 0.3) {
      console.log('\\n❌ VERDICT: Claude is responding but not reasoning correctly');
      console.log('🔧 May be getting corrupted input/output or wrong Claude model');
    } else if (correct >= responded * 0.6) {
      console.log('\\n🎉 VERDICT: Claude is demonstrating genuine cognition!');
      console.log('✅ Responses show reasoning, not just pattern matching');
    } else {
      console.log('\\n🤔 VERDICT: Mixed cognitive performance');
      console.log('🔧 Claude is thinking but may have input/output issues');
    }
    
    // Show cognitive performance by type
    const byType = {};
    this.results.forEach(r => {
      if (!byType[r.type]) byType[r.type] = { total: 0, correct: 0, responded: 0 };
      byType[r.type].total++;
      if (r.gotResponse) byType[r.type].responded++;
      if (r.correct) byType[r.type].correct++;
    });
    
    console.log('\\n📋 Performance by Cognitive Type:');
    Object.entries(byType).forEach(([type, stats]) => {
      const accuracy = stats.responded > 0 ? (stats.correct/stats.responded*100).toFixed(1) : 0;
      console.log(`   ${type}: ${stats.correct}/${stats.responded} correct (${accuracy}%)`);
    });
    
    // Save detailed results
    const resultsFile = 'claude-cognition-results.json';
    fs.writeFileSync(resultsFile, JSON.stringify({
      testDate: new Date().toISOString(),
      summary: {
        totalTests: total,
        responded: responded,
        correct: correct,
        communicationRate: responded/total,
        accuracyRate: responded > 0 ? correct/responded : 0
      },
      resultsByType: byType,
      detailedResults: this.results
    }, null, 2));
    
    console.log(`\\n💾 Detailed results saved to: ${resultsFile}`);
    
    // Cleanup
    if (this.wrapper) {
      this.wrapper.terminate();
    }
  }

  sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

// Run the cognition test
if (require.main === module) {
  const tester = new ClaudeCognitionTest();
  tester.runCognitionTest().catch(error => {
    console.error('💥 Cognition test failed:', error.message);
    process.exit(1);
  });
}

module.exports = ClaudeCognitionTest;