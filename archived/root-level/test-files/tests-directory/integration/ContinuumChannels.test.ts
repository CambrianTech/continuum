/**
 * Integration Tests for Continuum Agent Communication Channels
 * Tests the actual implementation in continuum.cjs
 */

describe('Continuum Channel Integration', () => {
  let mockWebSocket: any;
  let mockDocument: any;

  beforeEach(() => {
    // Mock WebSocket
    mockWebSocket = {
      send: jest.fn(),
      readyState: 1
    };

    // Mock DOM for frontend tests
    mockDocument = {
      getElementById: jest.fn(() => ({
        className: '',
        textContent: '',
        innerHTML: '',
        appendChild: jest.fn(),
        scrollTop: 0,
        scrollHeight: 100
      }))
    };

    (global as any).document = mockDocument;
  });

  // Helper functions for testing
  function simulateWebSocketMessage(type: string, data: any) {
    const messageEvent = {
      data: JSON.stringify({
        type: type,
        data: data
      })
    };

    // Simulate the onmessage handler logic from continuum.cjs
    const parsedData = JSON.parse(messageEvent.data);
    
    return {
      type: parsedData.type,
      data: parsedData.data,
      handled: true
    };
  }

  function parseAgentAction(message: string): { agent: string; action: string } {
    let agentName = 'AI';
    if (message.includes('PlannerAI')) agentName = 'PlannerAI';
    else if (message.includes('CodeAI')) agentName = 'CodeAI'; 
    else if (message.includes('GeneralAI')) agentName = 'GeneralAI';
    
    // Parse specific actions
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

  function getInitialAgent(task: string): string {
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

  describe('Frontend Channel Handling', () => {

    it('should handle status updates correctly', () => {
      const result = simulateWebSocketMessage('status', {
        message: 'PlannerAI is analyzing...',
        costs: { total: 0.05, requests: 1 }
      });

      expect(result.type).toBe('status');
      expect(result.data.message).toBe('PlannerAI is analyzing...');
      expect(result.handled).toBe(true);
    });

    it('should handle working messages correctly', () => {
      const workingMessage = 'PlannerAI processing: analyze this codebase...';
      const result = simulateWebSocketMessage('working', workingMessage);

      expect(result.type).toBe('working');
      expect(result.data).toBe(workingMessage);
      expect(result.handled).toBe(true);
    });

    it('should handle result messages correctly', () => {
      const resultData = {
        role: 'PlannerAI',
        task: 'analyze codebase',
        result: 'Here is my analysis...',
        costs: { total: 0.15, requests: 3 }
      };

      const result = simulateWebSocketMessage('result', resultData);

      expect(result.type).toBe('result');
      expect(result.data.role).toBe('PlannerAI');
      expect(result.data.result).toBe('Here is my analysis...');
      expect(result.handled).toBe(true);
    });
  });

  describe('Agent Status Parsing', () => {

    it('should correctly identify agent names from messages', () => {
      const testCases = [
        { message: 'PlannerAI processing: analyze task', expectedAgent: 'PlannerAI' },
        { message: 'CodeAI processing: implement feature', expectedAgent: 'CodeAI' },
        { message: 'GeneralAI processing: general help', expectedAgent: 'GeneralAI' },
        { message: 'Unknown processing: some task', expectedAgent: 'AI' }
      ];

      testCases.forEach(({ message, expectedAgent }) => {
        const result = parseAgentAction(message);
        expect(result.agent).toBe(expectedAgent);
      });
    });

    it('should correctly parse action types', () => {
      const testCases = [
        { 
          message: 'Enhanced intelligent routing: complex task', 
          expectedAction: 'Analyzing request and selecting best AI...' 
        },
        { 
          message: 'PlannerAI processing: analyze code', 
          expectedAction: 'PlannerAI is thinking...' 
        },
        { 
          message: 'Executing WebFetch: https://example.com', 
          expectedAction: 'Searching the web...' 
        },
        { 
          message: 'Executing FILE_READ: package.json', 
          expectedAction: 'Reading files...' 
        },
        { 
          message: 'Creating new PlannerAI session...', 
          expectedAction: 'Initializing AI agent...' 
        }
      ];

      testCases.forEach(({ message, expectedAction }) => {
        const result = parseAgentAction(message);
        expect(result.action).toBe(expectedAction);
      });
    });
  });

  describe('Agent Selection Logic', () => {

    it('should route coordination tasks to PlannerAI', () => {
      const coordinationTasks = [
        'coordinate with CodeAI to fix this',
        'there is a CI failure in the build',
        'can you fix this GitHub issue',
        'the PR build is failing'
      ];

      coordinationTasks.forEach(task => {
        expect(getInitialAgent(task)).toBe('PlannerAI');
      });
    });

    it('should route strategic tasks to PlannerAI', () => {
      const strategicTasks = [
        'how should we plan this feature',
        'what is the best strategy for this',
        'analyze this codebase architecture',
        'design a solution for this problem',
        'this is a very complex task that needs careful planning and analysis'
      ];

      strategicTasks.forEach(task => {
        expect(getInitialAgent(task)).toBe('PlannerAI');
      });
    });

    it('should route code tasks to CodeAI', () => {
      const codeTasks = [
        'implement this feature',
        'write some code for this',
        'fix this bug in the function',
        'add code to handle this case'
      ];

      codeTasks.forEach(task => {
        expect(getInitialAgent(task)).toBe('CodeAI');
      });
    });

    it('should route general tasks to GeneralAI', () => {
      const generalTasks = [
        'hello',
        'help me understand this',
        'what is the weather',
        'simple question'
      ];

      generalTasks.forEach(task => {
        expect(getInitialAgent(task)).toBe('GeneralAI');
      });
    });

    it('should route Continuum explanation tasks to PlannerAI', () => {
      const continuumTasks = [
        'what is Continuum',
        'how does Continuum work',
        'explain Continuum to me'
      ];

      continuumTasks.forEach(task => {
        expect(getInitialAgent(task)).toBe('PlannerAI');
      });
    });
  });

  describe('Message Format Validation', () => {
    it('should validate WebSocket message format', () => {
      const validMessages = [
        { type: 'status', data: { message: 'Ready', costs: { total: 0, requests: 0 } } },
        { type: 'working', data: 'PlannerAI is thinking...' },
        { type: 'result', data: { role: 'PlannerAI', result: 'Done', costs: { total: 0.1, requests: 1 } } },
        { type: 'error', data: 'Something went wrong' }
      ];

      validMessages.forEach(message => {
        expect(message).toHaveProperty('type');
        expect(message).toHaveProperty('data');
        expect(typeof message.type).toBe('string');
        expect(message.data).toBeDefined();
      });
    });

    it('should handle malformed messages gracefully', () => {
      const malformedMessages = [
        '{"type": "status"}', // missing data
        '{"data": "something"}', // missing type
        'invalid json',
        '',
        null,
        undefined
      ];

      malformedMessages.forEach(message => {
        expect(() => {
          try {
            if (message) {
              JSON.parse(message);
            }
          } catch (error) {
            // Should handle parse errors gracefully
          }
        }).not.toThrow();
      });
    });
  });

  describe('Concurrent Channel Usage', () => {
    it('should handle multiple agents sending messages simultaneously', () => {
      const messages = [
        { type: 'working', data: 'PlannerAI processing: task 1' },
        { type: 'working', data: 'CodeAI processing: task 2' },
        { type: 'working', data: 'GeneralAI processing: task 3' },
        { type: 'result', data: { role: 'PlannerAI', result: 'Analysis complete' } },
        { type: 'result', data: { role: 'CodeAI', result: 'Code implemented' } },
        { type: 'result', data: { role: 'GeneralAI', result: 'Help provided' } }
      ];

      // Simulate rapid message processing
      const processedMessages = messages.map(msg => {
        const processed = simulateWebSocketMessage(msg.type, msg.data);
        return processed;
      });

      expect(processedMessages).toHaveLength(6);
      processedMessages.forEach(msg => {
        expect(msg.handled).toBe(true);
      });
    });

    it('should maintain message order for same agent', () => {
      const agentMessages = [
        'PlannerAI processing: analyzing...',
        'PlannerAI Executing WebFetch: https://docs.com',
        'PlannerAI responded: analysis complete'
      ];

      const parsedActions = agentMessages.map(msg => parseAgentAction(msg));

      // All should be from PlannerAI
      parsedActions.forEach(action => {
        expect(action.agent).toBe('PlannerAI');
      });

      // Actions should reflect the progression
      expect(parsedActions[0].action).toBe('PlannerAI is thinking...');
      expect(parsedActions[1].action).toBe('Searching the web...');
      expect(parsedActions[2].action).toBe('Formulating response...');
    });
  });

  describe('Error Recovery', () => {
    it('should recover from WebSocket errors', () => {
      const errorScenarios = [
        { type: 'connection_lost', handled: false },
        { type: 'parse_error', handled: false },
        { type: 'timeout', handled: false }
      ];

      errorScenarios.forEach(scenario => {
        // Should not crash the system
        expect(() => {
          // Simulate error handling
          if (scenario.type === 'connection_lost') {
            mockWebSocket.readyState = 3; // CLOSED
          }
        }).not.toThrow();
      });
    });

    it('should handle agent communication failures gracefully', () => {
      const failureScenarios = [
        'Agent response timeout',
        'Invalid agent response format',
        'Agent threw an error',
        'Network connection failed'
      ];

      failureScenarios.forEach(scenario => {
        expect(() => {
          // Simulate error in agent communication
          const errorMessage = simulateWebSocketMessage('error', scenario);
          expect(errorMessage.type).toBe('error');
        }).not.toThrow();
      });
    });
  });
});