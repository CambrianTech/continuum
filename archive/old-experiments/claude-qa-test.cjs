#!/usr/bin/env node
/**
 * CLAUDE Q&A UNIT TEST
 * 
 * Tests Claude with questions where we know the correct answers
 * Claude can't see the answers, so if it gets them right, it's really thinking
 * Tests both factual knowledge and reasoning ability
 */

const ClaudeAutoWrapper = require('./claude-auto-wrapper.cjs');
const fs = require('fs');

class ClaudeQATest {
  constructor() {
    this.testQuestions = [
      // Math questions (exact answers expected)
      {
        question: "What is 7 × 8?",
        expectedAnswers: ["56"],
        type: "math",
        acceptPartial: false
      },
      {
        question: "What is the square root of 144?",
        expectedAnswers: ["12"],
        type: "math", 
        acceptPartial: false
      },
      
      // Logic questions
      {
        question: "If all cats are mammals and Fluffy is a cat, what is Fluffy?",
        expectedAnswers: ["mammal", "a mammal"],
        type: "logic",
        acceptPartial: true
      },
      
      // Common knowledge (multiple acceptable answers)
      {
        question: "What color do you get when you mix red and blue?",
        expectedAnswers: ["purple", "violet"],
        type: "knowledge",
        acceptPartial: true
      },
      {
        question: "What is the capital of France?",
        expectedAnswers: ["Paris"],
        type: "knowledge",
        acceptPartial: false
      },
      
      // Reasoning questions
      {
        question: "A farmer has 17 sheep. All but 9 die. How many are left?",
        expectedAnswers: ["9", "nine"],
        type: "reasoning",
        acceptPartial: false
      },
      {
        question: "What comes next in this sequence: 2, 4, 8, 16, ?",
        expectedAnswers: ["32"],
        type: "reasoning",
        acceptPartial: false
      },
      
      // Creativity (we just check if it gives a reasonable response)
      {
        question: "Name three things that are typically red.",
        expectedAnswers: ["fire", "blood", "roses", "apples", "tomatoes", "strawberries", "stop signs"],
        type: "creativity",
        acceptPartial: true,
        minMatches: 1 // Just need to match at least one reasonable answer
      }
    ];
    
    this.results = [];
    this.wrapper = null;
  }

  async runQATest() {
    console.log('🧪 CLAUDE Q&A UNIT TEST');
    console.log('========================');
    console.log(`📝 ${this.testQuestions.length} questions prepared`);
    console.log('🤖 Questions test real knowledge and reasoning');
    console.log('❓ Claude cannot see the expected answers');
    console.log('');

    // Launch Claude with a simple prompt
    this.wrapper = new ClaudeAutoWrapper('qa-test');
    
    const qaPrompt = `You are being tested on your knowledge and reasoning ability. 
I will ask you questions and you should give concise, accurate answers.
Just answer the question directly without explaining your reasoning unless asked.

Ready for the first question!`;

    console.log('🚀 Launching Claude for Q&A test...');
    
    try {
      await this.wrapper.launchClaude(qaPrompt);
      
      // Wait a moment for Claude to be ready
      await this.sleep(3000);
      
      // Run through all questions
      await this.askAllQuestions();
      
      // Analyze results
      this.analyzeResults();
      
    } catch (error) {
      console.error('❌ Test failed:', error.message);
    } finally {
      if (this.wrapper) {
        await this.wrapper.terminate();
      }
    }
  }

  async askAllQuestions() {
    console.log('❓ Starting Q&A sequence...');
    
    for (let i = 0; i < this.testQuestions.length; i++) {
      const question = this.testQuestions[i];
      
      console.log(`\\n📋 Question ${i + 1}/${this.testQuestions.length}: ${question.question}`);
      
      try {
        const answer = await this.askQuestionAndGetResponse(question.question);
        
        const isCorrect = this.checkAnswer(answer, question);
        
        this.results.push({
          questionIndex: i,
          question: question.question,
          answer: answer,
          expected: question.expectedAnswers,
          type: question.type,
          correct: isCorrect,
          timestamp: new Date().toISOString()
        });
        
        const status = isCorrect ? '✅ CORRECT' : '❌ INCORRECT';
        console.log(`   🤖 Claude answered: "${answer}"`);
        console.log(`   ${status}`);
        
      } catch (error) {
        console.log(`   ❌ Failed to get answer: ${error.message}`);
        
        this.results.push({
          questionIndex: i,
          question: question.question,
          answer: null,
          expected: question.expectedAnswers,
          type: question.type,
          correct: false,
          error: error.message,
          timestamp: new Date().toISOString()
        });
      }
      
      // Brief pause between questions
      await this.sleep(2000);
    }
  }

  async askQuestionAndGetResponse(question) {
    return new Promise((resolve, reject) => {
      let responseBuffer = '';
      let timeoutId;
      
      // Set up output capture
      const originalStdoutWrite = this.wrapper.claudeProcess.stdout.on;
      
      const outputHandler = (data) => {
        const text = data.toString();
        responseBuffer += text;
        
        // Look for a complete response (ends with newline or question mark)
        if (text.includes('\\n') || text.includes('?')) {
          clearTimeout(timeoutId);
          resolve(this.extractAnswer(responseBuffer));
        }
      };
      
      // Temporarily capture Claude's output
      this.wrapper.claudeProcess.stdout.on('data', outputHandler);
      
      // Send the question
      this.wrapper.sendMessage(question);
      
      // Timeout after 10 seconds
      timeoutId = setTimeout(() => {
        this.wrapper.claudeProcess.stdout.removeListener('data', outputHandler);
        
        if (responseBuffer.trim()) {
          resolve(this.extractAnswer(responseBuffer));
        } else {
          reject(new Error('No response received within timeout'));
        }
      }, 10000);
    });
  }

  extractAnswer(responseText) {
    // Clean up the response to extract just the answer
    let answer = responseText
      .replace(/^[\\s\\n]+/, '') // Remove leading whitespace
      .replace(/[\\s\\n]+$/, '') // Remove trailing whitespace
      .split('\\n')[0] // Take first line if multi-line
      .trim();
    
    // Remove common prefixes
    answer = answer
      .replace(/^(Answer:|A:|The answer is|It is|That would be)\\s*/i, '')
      .replace(/\\.$/, '') // Remove trailing period
      .trim();
    
    return answer;
  }

  checkAnswer(actualAnswer, question) {
    if (!actualAnswer) return false;
    
    const actual = actualAnswer.toLowerCase().trim();
    const expected = question.expectedAnswers.map(a => a.toLowerCase().trim());
    
    if (question.type === 'creativity' && question.minMatches) {
      // For creativity questions, check if answer contains any expected elements
      let matches = 0;
      for (const expectedItem of expected) {
        if (actual.includes(expectedItem)) {
          matches++;
        }
      }
      return matches >= question.minMatches;
    }
    
    if (question.acceptPartial) {
      // Check if any expected answer is contained in the actual answer
      return expected.some(exp => actual.includes(exp) || exp.includes(actual));
    } else {
      // Exact match required
      return expected.includes(actual);
    }
  }

  analyzeResults() {
    console.log('\\n\\n📊 Q&A TEST RESULTS');
    console.log('===================');
    
    const total = this.results.length;
    const correct = this.results.filter(r => r.correct).length;
    const failed = this.results.filter(r => r.answer === null).length;
    
    console.log(`📈 Total Questions: ${total}`);
    console.log(`✅ Correct Answers: ${correct}`);
    console.log(`❌ Incorrect Answers: ${total - correct - failed}`);
    console.log(`💥 Failed to Answer: ${failed}`);
    
    const successRate = total > 0 ? (correct / total * 100).toFixed(1) : 0;
    console.log(`🎯 Success Rate: ${successRate}%`);
    
    // Break down by question type
    const byType = {};
    this.results.forEach(r => {
      if (!byType[r.type]) {
        byType[r.type] = { total: 0, correct: 0 };
      }
      byType[r.type].total++;
      if (r.correct) byType[r.type].correct++;
    });
    
    console.log('\\n📋 Results by Type:');
    Object.entries(byType).forEach(([type, stats]) => {
      const rate = ((stats.correct / stats.total) * 100).toFixed(1);
      console.log(`   ${type}: ${stats.correct}/${stats.total} (${rate}%)`);
    });
    
    // Show detailed results
    console.log('\\n📝 Detailed Results:');
    this.results.forEach((result, i) => {
      const status = result.correct ? '✅' : '❌';
      console.log(`   ${status} Q${i + 1}: ${result.question}`);
      if (result.answer) {
        console.log(`      Answer: "${result.answer}"`);
      } else {
        console.log(`      Failed: ${result.error || 'No response'}`);
      }
    });
    
    // Save results to file
    const resultsFile = 'claude-qa-results.json';
    fs.writeFileSync(resultsFile, JSON.stringify({
      testDate: new Date().toISOString(),
      totalQuestions: total,
      correctAnswers: correct,
      successRate: successRate,
      resultsByType: byType,
      detailedResults: this.results
    }, null, 2));
    
    console.log(`\\n💾 Results saved to: ${resultsFile}`);
    
    // Determine if Claude is actually working
    if (failed === total) {
      console.log('\\n💥 COMPLETE FAILURE: Claude never responded to any questions');
      console.log('🔧 The Claude integration is completely broken');
    } else if (successRate < 30) {
      console.log('\\n⚠️  POOR PERFORMANCE: Success rate too low for legitimate Claude');
      console.log('🔧 Either Claude integration is broken or responses are corrupted');
    } else if (successRate >= 60) {
      console.log('\\n🎉 SUCCESS: Claude appears to be working correctly!');
      console.log('✅ Responses show genuine reasoning and knowledge');
    } else {
      console.log('\\n🤔 MIXED RESULTS: Some responses work but success rate is concerning');
      console.log('🔧 Claude may be working but with communication issues');
    }
  }

  sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

// Run the test
if (require.main === module) {
  const tester = new ClaudeQATest();
  tester.runQATest().catch(error => {
    console.error('💥 Q&A test failed:', error.message);
    process.exit(1);
  });
}

module.exports = ClaudeQATest;