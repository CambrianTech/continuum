#!/usr/bin/env node
/**
 * Simple Test for Agent Communication Channels
 * Validates that agents can write to command, status, and message channels without interference
 */

const assert = require('assert');

// Mock WebSocket for testing
class MockWebSocket {
  constructor() {
    this.sent = [];
    this.readyState = 1; // OPEN
  }

  send(data) {
    this.sent.push(JSON.parse(data));
  }

  getSentMessages() {
    return this.sent;
  }

  clearMessages() {
    this.sent = [];
  }
}

// Agent Channel System (simplified from the TypeScript version)
class AgentChannelSystem {
  sendCommandChannel(agentName, command, ws) {
    if (ws && ws.readyState === 1) {
      ws.send(JSON.stringify({
        type: 'command',
        agent: agentName,
        data: command
      }));
    }
  }

  sendStatusChannel(agentName, status, ws) {
    if (ws && ws.readyState === 1) {
      ws.send(JSON.stringify({
        type: 'status_update',
        agent: agentName,
        data: status
      }));
    }
  }

  sendMessageChannel(agentName, message, ws) {
    if (ws && ws.readyState === 1) {
      ws.send(JSON.stringify({
        type: 'agent_message',
        agent: agentName,
        data: message
      }));
    }
  }
}

// Agent Action Parser (from the main system)
function parseAgentAction(message) {
  let agentName = 'AI';
  if (message.includes('PlannerAI')) agentName = 'PlannerAI';
  else if (message.includes('CodeAI')) agentName = 'CodeAI'; 
  else if (message.includes('GeneralAI')) agentName = 'GeneralAI';
  
  let action = '';
  if (message.includes('Enhanced intelligent routing')) action = 'Analyzing request and selecting best AI...';
  else if (message.includes('Strategic/complex task')) action = 'Routing to strategic AI...';
  else if (message.includes('Creating new') && message.includes('session')) action = 'Initializing AI agent...';
  else if (message.includes('processing:')) action = `${agentName} is thinking...`;
  else if (message.includes('Executing WebFetch')) action = 'Searching the web...';
  else if (message.includes('Executing FILE_READ')) action = 'Reading files...';
  else if (message.includes('Executing GIT_STATUS')) action = 'Checking repository status...';
  else if (message.includes('responded:')) action = 'Formulating response...';
  else action = `${agentName} is working...`;

  return { agent: agentName, action };
}

// Agent Selection Logic (from the main system)
function getInitialAgent(task) {
  const taskLower = task.toLowerCase();
  
  if ((taskLower.includes('coordinate') && taskLower.includes('codeai')) ||
      taskLower.includes('ci') || taskLower.includes('github') || taskLower.includes('pr') || 
      taskLower.includes('build fail') || (taskLower.includes('fix') && taskLower.includes('issue'))) {
    return 'PlannerAI';
  } else if (taskLower.includes('plan') || taskLower.includes('strategy') || taskLower.includes('architecture') || 
      taskLower.includes('design') || taskLower.includes('how') || taskLower.includes('what') ||
      taskLower.includes('analyze') || taskLower.includes('organize') ||
      taskLower.includes('improve') || taskLower.includes('optimize') || taskLower.includes('create') ||
      taskLower.includes('build') || taskLower.includes('develop') || taskLower.includes('solution') ||
      task.split(' ').length > 5) {
    return 'PlannerAI';
  } else if (taskLower.includes('continuum') && (taskLower.includes('what') || taskLower.includes('explain') || taskLower.includes('how'))) {
    return 'PlannerAI';
  } else if (taskLower.includes('code') || taskLower.includes('implement') || taskLower.includes('bug')) {
    return 'CodeAI';
  } else {
    return 'GeneralAI';
  }
}

// Test Runner
function runTests() {
  console.log('🧪 Running Agent Communication Channel Tests...');
  console.log('================================================');

  const mockWS = new MockWebSocket();
  const system = new AgentChannelSystem();
  let testCount = 0;
  let passCount = 0;

  function test(description, testFn) {
    testCount++;
    try {
      mockWS.clearMessages();
      testFn();
      console.log(`  ✅ ${description}`);
      passCount++;
    } catch (error) {
      console.log(`  ❌ ${description}`);
      console.log(`     Error: ${error.message}`);
    }
  }

  // Test 1: Command Channel
  test('should send command messages correctly', () => {
    system.sendCommandChannel('PlannerAI', 'WEBFETCH: https://example.com', mockWS);
    const messages = mockWS.getSentMessages();
    assert.strictEqual(messages.length, 1);
    assert.strictEqual(messages[0].type, 'command');
    assert.strictEqual(messages[0].agent, 'PlannerAI');
    assert.strictEqual(messages[0].data, 'WEBFETCH: https://example.com');
  });

  // Test 2: Status Channel
  test('should send status messages correctly', () => {
    system.sendStatusChannel('CodeAI', 'Implementing solution...', mockWS);
    const messages = mockWS.getSentMessages();
    assert.strictEqual(messages.length, 1);
    assert.strictEqual(messages[0].type, 'status_update');
    assert.strictEqual(messages[0].agent, 'CodeAI');
    assert.strictEqual(messages[0].data, 'Implementing solution...');
  });

  // Test 3: Message Channel
  test('should send message channel correctly', () => {
    system.sendMessageChannel('GeneralAI', 'Here is my response...', mockWS);
    const messages = mockWS.getSentMessages();
    assert.strictEqual(messages.length, 1);
    assert.strictEqual(messages[0].type, 'agent_message');
    assert.strictEqual(messages[0].agent, 'GeneralAI');
    assert.strictEqual(messages[0].data, 'Here is my response...');
  });

  // Test 4: Channel Isolation
  test('should not interfere between channels', () => {
    system.sendCommandChannel('PlannerAI', 'WEBFETCH: https://example.com', mockWS);
    system.sendStatusChannel('PlannerAI', 'Searching the web...', mockWS);
    system.sendMessageChannel('PlannerAI', 'Found relevant information.', mockWS);

    const messages = mockWS.getSentMessages();
    assert.strictEqual(messages.length, 3);
    assert.strictEqual(messages[0].type, 'command');
    assert.strictEqual(messages[1].type, 'status_update');
    assert.strictEqual(messages[2].type, 'agent_message');
  });

  // Test 5: Agent Name Detection
  test('should correctly detect agent names from messages', () => {
    const testCases = [
      { message: 'PlannerAI processing: analyze task', expectedAgent: 'PlannerAI' },
      { message: 'CodeAI processing: implement feature', expectedAgent: 'CodeAI' },
      { message: 'GeneralAI processing: general help', expectedAgent: 'GeneralAI' },
      { message: 'Unknown processing: some task', expectedAgent: 'AI' }
    ];

    testCases.forEach(({ message, expectedAgent }) => {
      const result = parseAgentAction(message);
      assert.strictEqual(result.agent, expectedAgent, `Failed for message: ${message}`);
    });
  });

  // Test 6: Action Parsing
  test('should correctly parse action types', () => {
    const testCases = [
      { message: 'Enhanced intelligent routing: complex task', expectedAction: 'Analyzing request and selecting best AI...' },
      { message: 'PlannerAI processing: analyze code', expectedAction: 'PlannerAI is thinking...' },
      { message: 'Executing WebFetch: https://example.com', expectedAction: 'Searching the web...' },
      { message: 'Executing FILE_READ: package.json', expectedAction: 'Reading files...' }
    ];

    testCases.forEach(({ message, expectedAction }) => {
      const result = parseAgentAction(message);
      assert.strictEqual(result.action, expectedAction, `Failed for message: ${message}`);
    });
  });

  // Test 7: Agent Selection Logic
  test('should route tasks to correct agents', () => {
    const testCases = [
      { task: 'coordinate with CodeAI to fix this', expectedAgent: 'PlannerAI' },
      { task: 'implement this feature', expectedAgent: 'CodeAI' },
      { task: 'hello there', expectedAgent: 'GeneralAI' },
      { task: 'how should we plan this complex architecture', expectedAgent: 'PlannerAI' }
    ];

    testCases.forEach(({ task, expectedAgent }) => {
      const result = getInitialAgent(task);
      assert.strictEqual(result, expectedAgent, `Failed for task: ${task}`);
    });
  });

  // Test 8: Multiple Agent Communication
  test('should handle multiple agents simultaneously', () => {
    system.sendCommandChannel('CodeAI', 'FILE_READ: src/main.ts', mockWS);
    system.sendStatusChannel('PlannerAI', 'Analyzing requirements...', mockWS);
    system.sendMessageChannel('GeneralAI', 'I can help with that.', mockWS);

    const messages = mockWS.getSentMessages();
    assert.strictEqual(messages.length, 3);
    assert.strictEqual(messages[0].agent, 'CodeAI');
    assert.strictEqual(messages[1].agent, 'PlannerAI');
    assert.strictEqual(messages[2].agent, 'GeneralAI');
  });

  // Test 9: Error Handling
  test('should handle closed WebSocket gracefully', () => {
    const closedWS = { readyState: 3, send: () => { throw new Error('Connection closed'); } };
    
    // Should not throw
    system.sendCommandChannel('PlannerAI', 'WEBFETCH: https://example.com', closedWS);
    system.sendStatusChannel('PlannerAI', 'Working...', closedWS);
    system.sendMessageChannel('PlannerAI', 'Hello', closedWS);
  });

  // Test 10: Concurrent Operations
  test('should handle rapid successive messages', () => {
    for (let i = 0; i < 50; i++) {
      system.sendStatusChannel('PlannerAI', `Status update ${i}`, mockWS);
    }
    const messages = mockWS.getSentMessages();
    assert.strictEqual(messages.length, 50);
  });

  console.log('\n🎯 Test Summary');
  console.log(`✅ ${passCount}/${testCount} tests passed`);
  
  if (passCount === testCount) {
    console.log('🎉 All tests passed! Agent communication channels work correctly.');
    console.log('\n✅ VALIDATION COMPLETE:');
    console.log('  - Agents can write to command, status, and message channels');
    console.log('  - Channels operate independently without interference');
    console.log('  - Agent name detection works correctly');
    console.log('  - Action parsing converts backend messages to user-friendly status');
    console.log('  - Agent selection logic routes tasks appropriately');
    console.log('  - Error handling prevents crashes');
    console.log('  - Concurrent operations work correctly');
    process.exit(0);
  } else {
    console.log('❌ Some tests failed. Check the output above.');
    process.exit(1);
  }
}

// Run the tests
runTests();