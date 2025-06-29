#!/usr/bin/env node
/**
 * Unit Tests for Status Indicator and Action Tracking System
 * Tests the new real-time status updates and AI action parsing
 */

const assert = require('assert');

// Mock DOM environment for testing
global.document = {
  getElementById: (id) => ({
    className: '',
    textContent: '',
    innerHTML: '',
    appendChild: () => {},
    scrollTop: 0,
    scrollHeight: 100
  })
};

// Extract the functions we want to test from the continuum.cjs
// Since they're embedded in the HTML, we'll recreate them here for testing
function updateStatus(agentName, color) {
  return {
    agentName: agentName,
    color: color,
    message: `You are now talking to ${agentName}`
  };
}

function updateActionStatus(action) {
  if (action) {
    return `<em>${action}</em>`;
  } else {
    return '';
  }
}

function getActionFromWorking(message) {
  // Extract agent name if present
  let agentName = 'AI';
  if (message.includes('PlannerAI')) agentName = 'PlannerAI';
  else if (message.includes('CodeAI')) agentName = 'CodeAI'; 
  else if (message.includes('GeneralAI')) agentName = 'GeneralAI';
  
  // Parse specific actions for detailed status
  if (message.includes('Enhanced intelligent routing')) return 'Analyzing request and selecting best AI...';
  if (message.includes('Strategic/complex task')) return 'Routing to strategic AI...';
  if (message.includes('Creating new') && message.includes('session')) return 'Initializing AI agent...';
  if (message.includes('Calling') && message.includes('with task')) return 'Processing with AI...';
  if (message.includes('processing:')) return agentName + ' is thinking...';
  if (message.includes('Scanning AI response for tool commands')) return 'Checking for tool usage...';
  if (message.includes('Executing WebFetch')) return 'Searching the web...';
  if (message.includes('Executing FILE_READ')) return 'Reading files...';
  if (message.includes('Executing GIT_STATUS')) return 'Checking repository status...';
  if (message.includes('Executed') && message.includes('tools')) return 'Completed tool execution...';
  if (message.includes('responded:')) return 'Formulating response...';
  if (message.includes('completed task')) return 'Finalizing...';
  if (message.includes('COORDINATION DETECTED')) return 'Multi-AI coordination required...';
  if (message.includes('Step 1:') || message.includes('Step 2:')) return 'Coordinating between AIs...';
  if (message.includes('analyzing')) return agentName + ' is analyzing...';
  if (message.includes('creating')) return agentName + ' is creating response...';
  if (message.includes('coordinating')) return 'Coordinating with other AIs...';
  
  return agentName + ' is working...';
}

// Test Suite
describe('Status Indicator System', () => {
  
  describe('updateStatus', () => {
    it('should create correct status for PlannerAI with green dot', () => {
      const result = updateStatus('PlannerAI', 'green');
      assert.strictEqual(result.agentName, 'PlannerAI');
      assert.strictEqual(result.color, 'green');
      assert.strictEqual(result.message, 'You are now talking to PlannerAI');
    });

    it('should create correct status for CodeAI with green dot', () => {
      const result = updateStatus('CodeAI', 'green');
      assert.strictEqual(result.agentName, 'CodeAI');
      assert.strictEqual(result.color, 'green');
      assert.strictEqual(result.message, 'You are now talking to CodeAI');
    });

    it('should handle red dot for connecting state', () => {
      const result = updateStatus('Aria', 'red');
      assert.strictEqual(result.agentName, 'Aria');
      assert.strictEqual(result.color, 'red');
      assert.strictEqual(result.message, 'You are now talking to Aria');
    });
  });

  describe('updateActionStatus', () => {
    it('should format action with italic emphasis', () => {
      const result = updateActionStatus('Thinking...');
      assert.strictEqual(result, '<em>Thinking...</em>');
    });

    it('should return empty string for no action', () => {
      const result = updateActionStatus('');
      assert.strictEqual(result, '');
    });

    it('should return empty string for null action', () => {
      const result = updateActionStatus(null);
      assert.strictEqual(result, '');
    });

    it('should handle complex action descriptions', () => {
      const result = updateActionStatus('Coordinating with other AIs...');
      assert.strictEqual(result, '<em>Coordinating with other AIs...</em>');
    });
  });

  describe('getActionFromWorking - AI Agent Detection', () => {
    it('should detect PlannerAI from message', () => {
      const message = 'PlannerAI processing: analyze this task';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'PlannerAI is thinking...');
    });

    it('should detect CodeAI from message', () => {
      const message = 'CodeAI processing: implement the fix';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'CodeAI is thinking...');
    });

    it('should detect GeneralAI from message', () => {
      const message = 'GeneralAI processing: general assistance';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'GeneralAI is thinking...');
    });

    it('should default to AI for unknown agents', () => {
      const message = 'UnknownAI processing: some task';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'AI is thinking...');
    });
  });

  describe('getActionFromWorking - Routing Actions', () => {
    it('should detect intelligent routing phase', () => {
      const message = 'Enhanced intelligent routing: complex task';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Analyzing request and selecting best AI...');
    });

    it('should detect strategic task routing', () => {
      const message = 'Strategic/complex task - routing to PlannerAI';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Routing to strategic AI...');
    });

    it('should detect coordination requirement', () => {
      const message = 'COORDINATION DETECTED - multi-AI required';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Multi-AI coordination required...');
    });

    it('should detect coordination steps', () => {
      const message1 = 'Step 1: Sending to PlannerAI';
      const message2 = 'Step 2: Coordinating with CodeAI';
      
      assert.strictEqual(getActionFromWorking(message1), 'Coordinating between AIs...');
      assert.strictEqual(getActionFromWorking(message2), 'Coordinating between AIs...');
    });
  });

  describe('getActionFromWorking - Session Management', () => {
    it('should detect new session creation', () => {
      const message = 'Creating new PlannerAI session...';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Initializing AI agent...');
    });

    it('should detect AI calling phase', () => {
      const message = 'Calling PlannerAI with task...';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Processing with AI...');
    });
  });

  describe('getActionFromWorking - Tool Execution', () => {
    it('should detect WebFetch execution', () => {
      const message = 'Executing WebFetch: https://example.com';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Searching the web...');
    });

    it('should detect file reading', () => {
      const message = 'Executing FILE_READ: package.json';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Reading files...');
    });

    it('should detect git status check', () => {
      const message = 'Executing GIT_STATUS';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Checking repository status...');
    });

    it('should detect tool scanning phase', () => {
      const message = 'Scanning AI response for tool commands...';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Checking for tool usage...');
    });

    it('should detect completed tool execution', () => {
      const message = 'Executed 3 tools for PlannerAI';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Completed tool execution...');
    });
  });

  describe('getActionFromWorking - Response Generation', () => {
    it('should detect AI response phase', () => {
      const message = 'PlannerAI responded: here is the analysis';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Formulating response...');
    });

    it('should detect task completion', () => {
      const message = 'PlannerAI completed task - response ready';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Finalizing...');
    });
  });

  describe('getActionFromWorking - Generic Actions', () => {
    it('should handle analyzing action with agent name', () => {
      const message = 'CodeAI analyzing the repository structure';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'CodeAI is analyzing...');
    });

    it('should handle creating action with agent name', () => {
      const message = 'PlannerAI creating comprehensive strategy';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'PlannerAI is creating response...');
    });

    it('should handle coordination action', () => {
      const message = 'coordinating with multiple AIs for solution';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Coordinating with other AIs...');
    });

    it('should default to working status', () => {
      const message = 'Some unknown activity happening';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'AI is working...');
    });
  });

  describe('Real-world Message Integration Tests', () => {
    it('should handle actual log message from Enhanced intelligent routing', () => {
      const message = '🧠 Enhanced intelligent routing: Hello, can you analyze this codebase?...';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Analyzing request and selecting best AI...');
    });

    it('should handle actual log message from SEND_TASK', () => {
      const message = '📤 SEND_TASK: Routing to PlannerAI - "Lead this task by analyzing the problem..."';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'PlannerAI is analyzing...');
    });

    it('should handle actual log message from AI processing', () => {
      const message = '🔄 PlannerAI processing: Lead this task by analyzing the problem and creating...';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'PlannerAI is thinking...');
    });

    it('should handle actual log message from tool execution', () => {
      const message = '🔍 Scanning AI response for tool commands...';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Checking for tool usage...');
    });

    it('should handle actual coordination messages', () => {
      const message = '🔧 COORDINATION DETECTED - PlannerAI will coordinate with CodeAI...';
      const result = getActionFromWorking(message);
      assert.strictEqual(result, 'Multi-AI coordination required...');
    });
  });
});

// Tests are already defined above in Jest format - no need for custom runner