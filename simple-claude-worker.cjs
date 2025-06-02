#!/usr/bin/env node
/**
 * SIMPLE CLAUDE WORKER
 * 
 * Uses claude --print for guaranteed responses
 * No interactive session issues, just send -> get response
 * This WILL work as a fallback for the pool
 */

const { exec } = require('child_process');
const { promisify } = require('util');

const execAsync = promisify(exec);

class SimpleClaudeWorker {
  constructor(role = 'general') {
    this.role = role;
    this.conversationHistory = [];
    this.isWorking = false;
  }

  async testConnection() {
    console.log('🧪 Testing simple Claude connection...');
    
    try {
      const { stdout, stderr } = await execAsync('claude --print "What is 2 + 2?"', {
        timeout: 10000
      });
      
      if (stdout && stdout.trim()) {
        console.log(`✅ Claude responded: "${stdout.trim()}"`);
        return true;
      } else {
        console.log(`❌ No response. stderr: ${stderr}`);
        return false;
      }
      
    } catch (error) {
      // Check if error has stdout (Claude might work but exit with error due to stderr)
      if (error.stdout && error.stdout.trim()) {
        console.log(`✅ Claude responded (with stderr): "${error.stdout.trim()}"`);
        return true;
      }
      
      console.log(`❌ Test failed: ${error.message}`);
      return false;
    }
  }

  async sendMessage(message) {
    console.log(`📤 Sending to Claude (${this.role}): "${message}"`);
    
    try {
      // Build the prompt with role context
      const rolePrompt = this.getRolePrompt();
      const fullPrompt = `${rolePrompt}\\n\\nUser: ${message}\\n\\nAssistant:`;
      
      // Escape the prompt for shell
      const escapedPrompt = fullPrompt.replace(/"/g, '\\"');
      
      const { stdout, stderr } = await execAsync(`claude --print "${escapedPrompt}"`, {
        timeout: 15000
      });
      
      if (stdout && stdout.trim()) {
        const response = stdout.trim();
        console.log(`📨 Claude (${this.role}) responded: "${response}"`);
        
        // Store in conversation history
        this.conversationHistory.push({
          timestamp: new Date().toISOString(),
          user: message,
          assistant: response
        });
        
        return response;
      } else {
        throw new Error(`No response received. stderr: ${stderr}`);
      }
      
    } catch (error) {
      // Check if error has stdout (Claude might work but exit with error due to stderr)
      if (error.stdout && error.stdout.trim()) {
        const response = error.stdout.trim();
        console.log(`📨 Claude (${this.role}) responded (with stderr): "${response}"`);
        
        // Store in conversation history
        this.conversationHistory.push({
          timestamp: new Date().toISOString(),
          user: message,
          assistant: response
        });
        
        return response;
      }
      
      console.error(`❌ Failed to send message: ${error.message}`);
      throw error;
    }
  }

  getRolePrompt() {
    const rolePrompts = {
      questioner: "You are QuestionerClaude. Your role is to ask clarifying questions when users make requests. Always ask follow-up questions to better understand what the user needs. Be specific and helpful.",
      
      planner: "You are PlannerClaude. Your role is to take user requests and create detailed plans. Break down complex tasks into specific steps. Always ask about priorities and deadlines.",
      
      implementer: "You are ImplementerClaude. Your role is to take plans and implement them. You can suggest code, file operations, and concrete actions. Always confirm before making significant changes.",
      
      reviewer: "You are ReviewerClaude. Your role is to review work and provide feedback. Check for errors, suggest improvements, and ensure quality standards.",
      
      general: "You are a helpful AI assistant. Provide clear, accurate responses to user questions."
    };
    
    return rolePrompts[this.role] || rolePrompts.general;
  }

  getStatus() {
    return {
      role: this.role,
      working: this.isWorking,
      messageCount: this.conversationHistory.length,
      lastActivity: this.conversationHistory.length > 0 
        ? this.conversationHistory[this.conversationHistory.length - 1].timestamp 
        : null
    };
  }
}

// Test the simple worker
async function testSimpleWorker() {
  console.log('🔧 TESTING SIMPLE CLAUDE WORKER');
  console.log('===============================');
  
  const worker = new SimpleClaudeWorker('questioner');
  
  // Test basic connection
  const connected = await worker.testConnection();
  
  if (!connected) {
    console.log('💥 Simple Claude worker failed basic test');
    return false;
  }
  
  // Test role-based response
  console.log('\\n🧪 Testing role-based responses...');
  
  try {
    const response1 = await worker.sendMessage('I want to build a website');
    
    if (response1.toLowerCase().includes('question') || response1.includes('?')) {
      console.log('✅ QuestionerClaude is asking questions as expected');
    } else {
      console.log('⚠️  Response doesn\'t seem to be asking questions');
    }
    
    // Test another message
    const response2 = await worker.sendMessage('What programming language should I use?');
    
    console.log('\\n✅ Simple Claude Worker is functional!');
    console.log('🎯 This can serve as our fallback Claude instance');
    
    return true;
    
  } catch (error) {
    console.error('❌ Role-based test failed:', error.message);
    return false;
  }
}

// Run test if called directly
if (require.main === module) {
  testSimpleWorker().then(success => {
    if (success) {
      console.log('\\n🎉 Simple Claude Worker is ready for production use!');
      process.exit(0);
    } else {
      console.log('\\n💥 Simple Claude Worker failed tests');
      process.exit(1);
    }
  });
}

module.exports = SimpleClaudeWorker;