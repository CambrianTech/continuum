#!/usr/bin/env node
/**
 * CLAUDE AUTO WRAPPER
 * 
 * Wraps real Claude CLI instances with automatic response handling
 * Detects yes/no questions and common prompts, responds automatically
 * Allows real Claude to run autonomously without user intervention
 */

const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

class ClaudeAutoWrapper {
  constructor(instanceName = 'auto-claude') {
    this.instanceName = instanceName;
    this.claudeProcess = null;
    this.outputBuffer = '';
    this.conversationLog = [];
    this.autoResponses = new Map();
    this.isWaitingForInput = false;
    this.currentPrompt = '';
    
    this.setupAutoResponses();
    
    console.log(`🤖 CLAUDE AUTO WRAPPER - ${instanceName}`);
    console.log('========================================');
    console.log('🔄 Auto-responds to Claude prompts');
    console.log('✅ Handles yes/no questions automatically');
    console.log('🚀 Allows autonomous Claude operation');
    console.log('');
  }

  setupAutoResponses() {
    // Common Claude prompts and their auto-responses
    this.autoResponses.set(/would you like me to.*\?/i, 'yes');
    this.autoResponses.set(/should i.*\?/i, 'yes');
    this.autoResponses.set(/do you want.*\?/i, 'yes');
    this.autoResponses.set(/shall i.*\?/i, 'yes');
    this.autoResponses.set(/continue.*\?/i, 'yes');
    this.autoResponses.set(/proceed.*\?/i, 'yes');
    this.autoResponses.set(/is this correct.*\?/i, 'yes');
    this.autoResponses.set(/does this look right.*\?/i, 'yes');
    this.autoResponses.set(/ready to.*\?/i, 'yes');
    this.autoResponses.set(/confirm.*\?/i, 'yes');
    
    // File operation prompts
    this.autoResponses.set(/overwrite.*\?/i, 'yes');
    this.autoResponses.set(/replace.*\?/i, 'yes');
    this.autoResponses.set(/create.*file.*\?/i, 'yes');
    this.autoResponses.set(/save.*\?/i, 'yes');
    
    // Error handling prompts
    this.autoResponses.set(/try again.*\?/i, 'yes');
    this.autoResponses.set(/retry.*\?/i, 'yes');
    this.autoResponses.set(/ignore.*error.*\?/i, 'yes');
    
    // Multiple choice - default to first option
    this.autoResponses.set(/select.*\[1\]/i, '1');
    this.autoResponses.set(/choose.*option/i, '1');
    
    // Generic yes/no with context
    this.autoResponses.set(/\(y\/n\)/i, 'y');
    this.autoResponses.set(/\[y\/n\]/i, 'y');
    this.autoResponses.set(/yes\/no/i, 'yes');
    
    console.log(`📋 Loaded ${this.autoResponses.size} automatic response patterns`);
  }

  async launchClaude(initialPrompt = null) {
    console.log('🚀 Launching real Claude with auto-wrapper...');
    
    const args = [];
    
    if (initialPrompt) {
      // Pass the prompt directly as an argument
      args.push(initialPrompt);
    }
    
    this.claudeProcess = spawn('claude', args, {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: process.env
    });
    
    console.log(`✅ Claude process started (PID: ${this.claudeProcess.pid})`);
    
    this.setupProcessHandlers();
    
    return this.claudeProcess;
  }

  setupProcessHandlers() {
    // Handle Claude's stdout
    this.claudeProcess.stdout.on('data', (data) => {
      const output = data.toString();
      this.outputBuffer += output;
      
      console.log(`📤 Claude output: ${output.trim()}`);
      
      this.processOutput(output);
    });
    
    // Handle Claude's stderr  
    this.claudeProcess.stderr.on('data', (data) => {
      const error = data.toString();
      console.log(`⚠️  Claude stderr: ${error.trim()}`);
    });
    
    // Handle process exit
    this.claudeProcess.on('exit', (code) => {
      console.log(`🔄 Claude process exited with code ${code}`);
      this.logConversation();
    });
    
    // Handle process errors
    this.claudeProcess.on('error', (error) => {
      console.error(`❌ Claude process error: ${error.message}`);
    });
  }

  processOutput(output) {
    // Look for prompts that need responses
    const lines = output.split('\\n');
    
    for (const line of lines) {
      const trimmed = line.trim();
      
      if (this.looksLikePrompt(trimmed)) {
        console.log(`❓ Detected prompt: "${trimmed}"`);
        this.currentPrompt = trimmed;
        this.handlePrompt(trimmed);
      }
    }
    
    // Check if Claude is waiting for input (cursor at end, no new output for a bit)
    if (output.includes('?') && !output.endsWith('\\n')) {
      setTimeout(() => {
        if (this.outputBuffer.endsWith(output)) {
          console.log('⏳ Detected Claude waiting for input');
          this.handleWaitingState();
        }
      }, 1000);
    }
  }

  looksLikePrompt(text) {
    if (text.length < 5) return false;
    
    // Common prompt indicators
    const promptIndicators = [
      '?',           // Questions
      '(y/n)',       // Yes/no prompts
      '[y/n]',       // Bracketed yes/no
      'continue',    // Continue prompts
      'proceed',     // Proceed prompts
      'confirm',     // Confirmation prompts
      'select',      // Selection prompts
      'choose',      // Choice prompts
      'enter'        // Enter input prompts
    ];
    
    const lowerText = text.toLowerCase();
    return promptIndicators.some(indicator => lowerText.includes(indicator));
  }

  handlePrompt(prompt) {
    // Find matching auto-response
    for (const [pattern, response] of this.autoResponses) {
      if (pattern.test(prompt)) {
        console.log(`🎯 Matched pattern: ${pattern}`);
        console.log(`🤖 Auto-responding: "${response}"`);
        
        this.sendResponse(response);
        
        this.conversationLog.push({
          timestamp: new Date().toISOString(),
          type: 'prompt',
          content: prompt,
          autoResponse: response
        });
        
        return;
      }
    }
    
    // No pattern matched - try intelligent guessing
    console.log('🤔 No pattern matched, trying intelligent response...');
    const intelligentResponse = this.generateIntelligentResponse(prompt);
    
    if (intelligentResponse) {
      console.log(`🧠 Intelligent response: "${intelligentResponse}"`);
      this.sendResponse(intelligentResponse);
      
      this.conversationLog.push({
        timestamp: new Date().toISOString(),
        type: 'prompt',
        content: prompt,
        intelligentResponse: intelligentResponse
      });
    } else {
      console.log('❓ Could not determine appropriate response');
      // Default to "yes" for unknown prompts
      this.sendResponse('yes');
      
      this.conversationLog.push({
        timestamp: new Date().toISOString(),
        type: 'prompt',
        content: prompt,
        defaultResponse: 'yes'
      });
    }
  }

  generateIntelligentResponse(prompt) {
    const lower = prompt.toLowerCase();
    
    // Safety/destructive action detection
    if (lower.includes('delete') || lower.includes('remove') || lower.includes('destroy')) {
      if (lower.includes('temporary') || lower.includes('temp') || lower.includes('test')) {
        return 'yes'; // OK to delete temp/test files
      } else {
        return 'no';  // Don't delete important things
      }
    }
    
    // Permission/access requests
    if (lower.includes('permission') || lower.includes('access') || lower.includes('allow')) {
      return 'yes'; // Generally allow permissions for our AI system
    }
    
    // File operations
    if (lower.includes('create') || lower.includes('write') || lower.includes('save')) {
      return 'yes'; // Allow file creation
    }
    
    // Network operations
    if (lower.includes('download') || lower.includes('fetch') || lower.includes('connect')) {
      return 'yes'; // Allow network operations
    }
    
    // Installation/setup
    if (lower.includes('install') || lower.includes('setup') || lower.includes('configure')) {
      return 'yes'; // Allow setup operations
    }
    
    // Questions with obvious yes bias
    if (lower.includes('helpful') || lower.includes('useful') || lower.includes('improve')) {
      return 'yes';
    }
    
    // Default for unrecognized prompts
    return 'yes';
  }

  handleWaitingState() {
    if (this.currentPrompt) {
      console.log('⚡ Claude still waiting, re-attempting response...');
      this.handlePrompt(this.currentPrompt);
    } else {
      console.log('⚡ Claude waiting but no prompt detected, sending default...');
      this.sendResponse('yes');
    }
  }

  sendResponse(response) {
    if (!this.claudeProcess || this.claudeProcess.stdin.destroyed) {
      console.log('❌ Cannot send response - Claude process not available');
      return;
    }
    
    try {
      this.claudeProcess.stdin.write(response + '\\n');
      console.log(`📨 Sent to Claude: "${response}"`);
    } catch (error) {
      console.error(`❌ Failed to send response: ${error.message}`);
    }
  }

  sendMessage(message) {
    console.log(`👤 Sending message to Claude: "${message}"`);
    
    this.conversationLog.push({
      timestamp: new Date().toISOString(),
      type: 'user_message',
      content: message
    });
    
    this.sendResponse(message);
  }

  logConversation() {
    const logFile = path.join(process.cwd(), `.${this.instanceName}-conversation.json`);
    fs.writeFileSync(logFile, JSON.stringify(this.conversationLog, null, 2));
    console.log(`📝 Conversation logged to: ${logFile}`);
  }

  async terminate() {
    console.log('🛑 Terminating Claude auto-wrapper...');
    
    if (this.claudeProcess && !this.claudeProcess.killed) {
      this.claudeProcess.kill('SIGTERM');
      
      // Force kill after 5 seconds if needed
      setTimeout(() => {
        if (!this.claudeProcess.killed) {
          console.log('💥 Force killing Claude process...');
          this.claudeProcess.kill('SIGKILL');
        }
      }, 5000);
    }
    
    this.logConversation();
  }
}

// Test the wrapper
async function testWrapper() {
  const wrapper = new ClaudeAutoWrapper('test-instance');
  
  const testPrompt = `# Test Claude Instance

You are a test Claude instance for autonomous operation.

Your task:
1. Help the user with any requests
2. When you need confirmation, ask yes/no questions
3. When you need more information, ask specific questions
4. Be helpful and proactive

Ready to begin autonomous operation!`;

  try {
    await wrapper.launchClaude(testPrompt);
    
    // Send a test message after a delay
    setTimeout(() => {
      wrapper.sendMessage('Hello Claude! Can you help me create a simple test file?');
    }, 3000);
    
    // Terminate after 30 seconds for testing
    setTimeout(() => {
      wrapper.terminate();
    }, 30000);
    
  } catch (error) {
    console.error('💥 Test failed:', error.message);
  }
}

// Handle command line usage
if (require.main === module) {
  const args = process.argv.slice(2);
  
  if (args.includes('--test')) {
    testWrapper();
  } else {
    console.log('Usage:');
    console.log('  node claude-auto-wrapper.cjs --test');
    console.log('  (or use as module)');
  }
}

module.exports = ClaudeAutoWrapper;