#!/usr/bin/env node
/**
 * REAL CLAUDE CONNECTOR
 * 
 * Actually connects to Claude API and sends real messages
 * No fake responses - this uses actual Anthropic Claude API
 * Requires real API key and makes real HTTP requests
 */

const https = require('https');
const fs = require('fs');
const path = require('path');

class RealClaudeConnector {
  constructor() {
    this.apiKey = this.loadApiKey();
    this.baseUrl = 'api.anthropic.com';
    this.projectRoot = process.cwd();
    
    console.log('🔌 REAL CLAUDE CONNECTOR');
    console.log('========================');
    
    if (!this.apiKey) {
      console.log('❌ No Claude API key found');
      console.log('💡 Set ANTHROPIC_API_KEY environment variable');
      console.log('💡 Or create .env file with ANTHROPIC_API_KEY=your_key');
      console.log('💡 Get key from: https://console.anthropic.com/');
      process.exit(1);
    }
    
    console.log('✅ Claude API key loaded');
    console.log('🚀 Ready to make REAL Claude API calls');
  }

  loadApiKey() {
    // Try environment variable first
    if (process.env.ANTHROPIC_API_KEY) {
      return process.env.ANTHROPIC_API_KEY;
    }
    
    // Try .env file
    const envFile = path.join(this.projectRoot, '.env');
    if (fs.existsSync(envFile)) {
      const envContent = fs.readFileSync(envFile, 'utf-8');
      const match = envContent.match(/ANTHROPIC_API_KEY=(.+)/);
      if (match) {
        return match[1].trim();
      }
    }
    
    return null;
  }

  async makeClaudeRequest(messages, systemPrompt = '') {
    return new Promise((resolve, reject) => {
      const requestData = JSON.stringify({
        model: 'claude-3-sonnet-20240229',
        max_tokens: 1000,
        system: systemPrompt,
        messages: messages
      });

      const options = {
        hostname: this.baseUrl,
        path: '/v1/messages',
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'x-api-key': this.apiKey,
          'anthropic-version': '2023-06-01',
          'Content-Length': Buffer.byteLength(requestData)
        }
      };

      const req = https.request(options, (res) => {
        let data = '';
        
        res.on('data', (chunk) => {
          data += chunk;
        });
        
        res.on('end', () => {
          try {
            const response = JSON.parse(data);
            
            if (res.statusCode !== 200) {
              reject(new Error(`API Error ${res.statusCode}: ${response.error?.message || data}`));
              return;
            }
            
            resolve(response);
          } catch (error) {
            reject(new Error(`Failed to parse response: ${error.message}`));
          }
        });
      });

      req.on('error', (error) => {
        reject(new Error(`Request failed: ${error.message}`));
      });

      req.write(requestData);
      req.end();
    });
  }

  async testRealClaudeConnection() {
    console.log('📡 Testing REAL Claude API connection...');
    
    try {
      const response = await this.makeClaudeRequest([
        {
          role: 'user',
          content: 'Please respond with exactly: "REAL CLAUDE CONNECTION WORKING"'
        }
      ]);

      if (response.content && response.content[0] && response.content[0].text) {
        const claudeResponse = response.content[0].text;
        console.log(`🤖 Claude responded: "${claudeResponse}"`);
        
        if (claudeResponse.includes('REAL CLAUDE CONNECTION WORKING')) {
          console.log('✅ REAL Claude API connection verified!');
          return true;
        } else {
          console.log('⚠️  Claude responded but not as expected');
          return false;
        }
      } else {
        console.log('❌ Unexpected response format from Claude API');
        return false;
      }
      
    } catch (error) {
      console.error('❌ Failed to connect to Claude API:', error.message);
      
      if (error.message.includes('401')) {
        console.log('💡 API key is invalid or expired');
      } else if (error.message.includes('403')) {
        console.log('💡 API key does not have required permissions');
      } else if (error.message.includes('429')) {
        console.log('💡 Rate limit exceeded - wait and try again');
      }
      
      return false;
    }
  }

  async askClaudeQuestion(userMessage, conversationHistory = []) {
    console.log(`📨 Asking Claude: "${userMessage}"`);
    
    const systemPrompt = `You are an interactive AI assistant that asks clarifying questions. 
When the user asks you something, you should:
1. Ask 1-2 specific follow-up questions to better understand their needs
2. Offer specific options when appropriate
3. Be helpful and conversational

Format your response as:
RESPONSE: [your response]
QUESTION: [your specific question]
OPTIONS: [comma-separated options if applicable, or "none"]`;

    const messages = [
      ...conversationHistory,
      {
        role: 'user',
        content: userMessage
      }
    ];

    try {
      const response = await this.makeClaudeRequest(messages, systemPrompt);
      
      if (response.content && response.content[0] && response.content[0].text) {
        const fullResponse = response.content[0].text;
        console.log(`🤖 Claude's full response: "${fullResponse}"`);
        
        // Parse the structured response
        const responseMatch = fullResponse.match(/RESPONSE: (.+?)(?=QUESTION:|$)/s);
        const questionMatch = fullResponse.match(/QUESTION: (.+?)(?=OPTIONS:|$)/s);
        const optionsMatch = fullResponse.match(/OPTIONS: (.+)/);
        
        const result = {
          response: responseMatch ? responseMatch[1].trim() : fullResponse,
          question: questionMatch ? questionMatch[1].trim() : null,
          options: optionsMatch && optionsMatch[1].trim() !== 'none' 
            ? optionsMatch[1].split(',').map(o => o.trim()) 
            : null
        };
        
        return result;
      } else {
        throw new Error('Unexpected response format from Claude');
      }
      
    } catch (error) {
      console.error('❌ Failed to get response from Claude:', error.message);
      throw error;
    }
  }

  async runInteractiveTest() {
    console.log('🎬 RUNNING REAL INTERACTIVE TEST WITH CLAUDE API');
    console.log('===============================================');
    
    // Test basic connection first
    const connectionWorking = await this.testRealClaudeConnection();
    if (!connectionWorking) {
      console.log('💥 Cannot proceed - Claude API connection failed');
      process.exit(1);
    }
    
    console.log('');
    console.log('🗣️  Testing interactive conversation...');
    
    // Test asking Claude to ask questions
    const testMessage = 'I want to integrate an AI API into my project';
    const result = await this.askClaudeQuestion(testMessage);
    
    console.log('');
    console.log('📊 RESULTS:');
    console.log(`💬 Claude's response: "${result.response}"`);
    
    if (result.question) {
      console.log(`❓ Claude asked: "${result.question}"`);
      
      if (result.options) {
        console.log(`🔘 Options provided: ${result.options.join(', ')}`);
      }
      
      console.log('');
      console.log('🎉 SUCCESS: Claude is asking REAL questions!');
      console.log('✅ This is genuine AI interaction, not fake responses');
      
      // Test answering the question
      console.log('');
      console.log('💬 Testing follow-up conversation...');
      
      const answer = result.options ? result.options[0] : 'Yes, I need help with that';
      console.log(`👤 Simulated user answer: "${answer}"`);
      
      const conversationHistory = [
        { role: 'user', content: testMessage },
        { role: 'assistant', content: result.response }
      ];
      
      const followUp = await this.askClaudeQuestion(answer, conversationHistory);
      
      console.log(`🤖 Claude's follow-up: "${followUp.response}"`);
      
      if (followUp.question) {
        console.log(`❓ Claude's next question: "${followUp.question}"`);
      }
      
      console.log('');
      console.log('🚀 REAL INTERACTIVE CONVERSATION VERIFIED!');
      console.log('✅ Claude can ask questions AND respond to answers');
      console.log('✅ This is 100% genuine AI interaction');
      
    } else {
      console.log('⚠️  Claude responded but didn\'t ask a follow-up question');
      console.log('🔧 May need to adjust the prompt to encourage more questions');
    }
  }
}

// Check if this is being run directly
if (require.main === module) {
  const connector = new RealClaudeConnector();
  connector.runInteractiveTest().catch(error => {
    console.error('💥 Test failed:', error.message);
    process.exit(1);
  });
}

module.exports = RealClaudeConnector;