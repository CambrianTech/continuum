/**
 * Unit Tests for Agent Communication Channels
 * Ensures agents can write to command, status, and message channels without interference
 */

describe('Agent Communication Channels', () => {
  let mockWebSocket: any;
  let mockResponse: any;
  let continuum: any;

  beforeEach(() => {
    // Mock WebSocket
    mockWebSocket = {
      send: jest.fn(),
      readyState: 1 // OPEN
    };

    // Mock HTTP Response
    mockResponse = {
      writeHead: jest.fn(),
      end: jest.fn()
    };

    // Mock Continuum instance with the channel methods
    continuum = {
      sessions: new Map(),
      costs: { total: 0, requests: 0 },
      getInitialAgent: jest.fn(),
      intelligentRoute: jest.fn(),
      
      // Channel methods to test
      sendCommandChannel: function(agentName: string, command: string, ws?: any) {
        if (ws) {
          ws.send(JSON.stringify({
            type: 'command',
            agent: agentName,
            data: command
          }));
        }
      },

      sendStatusChannel: function(agentName: string, status: string, ws?: any) {
        if (ws) {
          ws.send(JSON.stringify({
            type: 'status_update',
            agent: agentName,
            data: status
          }));
        }
      },

      sendMessageChannel: function(agentName: string, message: string, ws?: any) {
        if (ws) {
          ws.send(JSON.stringify({
            type: 'agent_message',
            agent: agentName,
            data: message
          }));
        }
      }
    };
  });

  describe('Command Channel', () => {
    it('should allow agents to send commands without affecting status', () => {
      // Agent sends a command
      continuum.sendCommandChannel('PlannerAI', 'WEBFETCH: https://example.com', mockWebSocket);
      
      expect(mockWebSocket.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'command',
          agent: 'PlannerAI',
          data: 'WEBFETCH: https://example.com'
        })
      );
      expect(mockWebSocket.send).toHaveBeenCalledTimes(1);
    });

    it('should handle multiple command types from different agents', () => {
      continuum.sendCommandChannel('CodeAI', 'FILE_READ: package.json', mockWebSocket);
      continuum.sendCommandChannel('PlannerAI', 'GIT_STATUS', mockWebSocket);
      continuum.sendCommandChannel('GeneralAI', 'WEBFETCH: https://docs.com', mockWebSocket);

      expect(mockWebSocket.send).toHaveBeenCalledTimes(3);
      expect(mockWebSocket.send).toHaveBeenNthCalledWith(1, 
        JSON.stringify({
          type: 'command',
          agent: 'CodeAI',
          data: 'FILE_READ: package.json'
        })
      );
      expect(mockWebSocket.send).toHaveBeenNthCalledWith(2,
        JSON.stringify({
          type: 'command',
          agent: 'PlannerAI', 
          data: 'GIT_STATUS'
        })
      );
      expect(mockWebSocket.send).toHaveBeenNthCalledWith(3,
        JSON.stringify({
          type: 'command',
          agent: 'GeneralAI',
          data: 'WEBFETCH: https://docs.com'
        })
      );
    });

    it('should not send command if WebSocket is not provided', () => {
      continuum.sendCommandChannel('PlannerAI', 'WEBFETCH: https://example.com');
      
      expect(mockWebSocket.send).not.toHaveBeenCalled();
    });
  });

  describe('Status Channel', () => {
    it('should allow agents to send status updates without affecting commands', () => {
      continuum.sendStatusChannel('PlannerAI', 'Analyzing repository structure...', mockWebSocket);
      
      expect(mockWebSocket.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'status_update',
          agent: 'PlannerAI',
          data: 'Analyzing repository structure...'
        })
      );
    });

    it('should handle status updates from multiple agents simultaneously', () => {
      continuum.sendStatusChannel('PlannerAI', 'Creating strategy...', mockWebSocket);
      continuum.sendStatusChannel('CodeAI', 'Implementing solution...', mockWebSocket);
      continuum.sendStatusChannel('GeneralAI', 'Reviewing results...', mockWebSocket);

      expect(mockWebSocket.send).toHaveBeenCalledTimes(3);
      
      const calls = mockWebSocket.send.mock.calls;
      expect(JSON.parse(calls[0][0])).toEqual({
        type: 'status_update',
        agent: 'PlannerAI',
        data: 'Creating strategy...'
      });
      expect(JSON.parse(calls[1][0])).toEqual({
        type: 'status_update',
        agent: 'CodeAI',
        data: 'Implementing solution...'
      });
      expect(JSON.parse(calls[2][0])).toEqual({
        type: 'status_update',
        agent: 'GeneralAI',
        data: 'Reviewing results...'
      });
    });

    it('should handle natural language status descriptions', () => {
      const naturalStatuses = [
        'Searching the web for latest documentation...',
        'Reading through the codebase...',
        'Coordinating with other AIs...',
        'Formulating a comprehensive response...'
      ];

      naturalStatuses.forEach(status => {
        continuum.sendStatusChannel('PlannerAI', status, mockWebSocket);
      });

      expect(mockWebSocket.send).toHaveBeenCalledTimes(4);
      naturalStatuses.forEach((status, index) => {
        expect(JSON.parse(mockWebSocket.send.mock.calls[index][0])).toEqual({
          type: 'status_update',
          agent: 'PlannerAI',
          data: status
        });
      });
    });
  });

  describe('Message Channel', () => {
    it('should allow agents to send messages without affecting status or commands', () => {
      continuum.sendMessageChannel('PlannerAI', 'Here is my analysis of the codebase...', mockWebSocket);
      
      expect(mockWebSocket.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'agent_message',
          agent: 'PlannerAI',
          data: 'Here is my analysis of the codebase...'
        })
      );
    });

    it('should handle long messages without truncation', () => {
      const longMessage = 'This is a very long message that contains detailed analysis. '.repeat(50);
      
      continuum.sendMessageChannel('CodeAI', longMessage, mockWebSocket);
      
      expect(mockWebSocket.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'agent_message',
          agent: 'CodeAI',
          data: longMessage
        })
      );
    });

    it('should handle messages with special characters and formatting', () => {
      const messageWithFormatting = `Here's the analysis:
      
      **Key Points:**
      - Item 1 with "quotes"
      - Item 2 with 'apostrophes'
      - Item 3 with <brackets>
      
      \`\`\`javascript
      const code = "example";
      \`\`\``;

      continuum.sendMessageChannel('GeneralAI', messageWithFormatting, mockWebSocket);
      
      expect(mockWebSocket.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'agent_message',
          agent: 'GeneralAI',
          data: messageWithFormatting
        })
      );
    });
  });

  describe('Channel Isolation', () => {
    it('should not interfere between channels when used simultaneously', () => {
      // Simulate agents using all channels at the same time
      continuum.sendCommandChannel('PlannerAI', 'WEBFETCH: https://example.com', mockWebSocket);
      continuum.sendStatusChannel('PlannerAI', 'Searching the web...', mockWebSocket);
      continuum.sendMessageChannel('PlannerAI', 'I found some relevant information.', mockWebSocket);

      expect(mockWebSocket.send).toHaveBeenCalledTimes(3);

      const calls = mockWebSocket.send.mock.calls.map((call: any) => JSON.parse(call[0]));
      
      expect(calls[0]).toEqual({
        type: 'command',
        agent: 'PlannerAI',
        data: 'WEBFETCH: https://example.com'
      });
      
      expect(calls[1]).toEqual({
        type: 'status_update',
        agent: 'PlannerAI',
        data: 'Searching the web...'
      });
      
      expect(calls[2]).toEqual({
        type: 'agent_message',
        agent: 'PlannerAI',
        data: 'I found some relevant information.'
      });
    });

    it('should handle multiple agents using different channels simultaneously', () => {
      // Different agents using different channels
      continuum.sendCommandChannel('CodeAI', 'FILE_READ: src/main.ts', mockWebSocket);
      continuum.sendStatusChannel('PlannerAI', 'Analyzing requirements...', mockWebSocket);
      continuum.sendMessageChannel('GeneralAI', 'I can help with that.', mockWebSocket);
      continuum.sendCommandChannel('PlannerAI', 'GIT_STATUS', mockWebSocket);
      continuum.sendStatusChannel('CodeAI', 'Reading source files...', mockWebSocket);

      expect(mockWebSocket.send).toHaveBeenCalledTimes(5);

      const calls = mockWebSocket.send.mock.calls.map((call: any) => JSON.parse(call[0]));
      
      // Verify each message has correct type and agent
      expect(calls[0].type).toBe('command');
      expect(calls[0].agent).toBe('CodeAI');
      
      expect(calls[1].type).toBe('status_update');
      expect(calls[1].agent).toBe('PlannerAI');
      
      expect(calls[2].type).toBe('agent_message');
      expect(calls[2].agent).toBe('GeneralAI');
      
      expect(calls[3].type).toBe('command');
      expect(calls[3].agent).toBe('PlannerAI');
      
      expect(calls[4].type).toBe('status_update');
      expect(calls[4].agent).toBe('CodeAI');
    });
  });

  describe('Error Handling', () => {
    it('should handle WebSocket send failures gracefully', () => {
      const failingWebSocket = {
        send: jest.fn(() => {
          throw new Error('WebSocket connection lost');
        }),
        readyState: 1
      };

      expect(() => {
        continuum.sendCommandChannel('PlannerAI', 'WEBFETCH: https://example.com', failingWebSocket);
      }).not.toThrow();
    });

    it('should handle closed WebSocket connections', () => {
      const closedWebSocket = {
        send: jest.fn(),
        readyState: 3 // CLOSED
      };

      // Should not attempt to send on closed connection
      continuum.sendStatusChannel('PlannerAI', 'Working...', closedWebSocket);
      
      expect(closedWebSocket.send).not.toHaveBeenCalled();
    });

    it('should handle null or undefined WebSocket', () => {
      expect(() => {
        continuum.sendCommandChannel('PlannerAI', 'WEBFETCH: https://example.com', null);
        continuum.sendStatusChannel('PlannerAI', 'Working...', undefined);
        continuum.sendMessageChannel('PlannerAI', 'Hello', null);
      }).not.toThrow();
    });
  });

  describe('Agent Name Validation', () => {
    it('should handle valid agent names', () => {
      const validAgentNames = ['PlannerAI', 'CodeAI', 'GeneralAI', 'TestAI', 'ReviewAI'];
      
      validAgentNames.forEach(agentName => {
        continuum.sendStatusChannel(agentName, 'Working...', mockWebSocket);
      });

      expect(mockWebSocket.send).toHaveBeenCalledTimes(5);
      
      validAgentNames.forEach((agentName, index) => {
        const call = JSON.parse(mockWebSocket.send.mock.calls[index][0]);
        expect(call.agent).toBe(agentName);
      });
    });

    it('should handle empty or invalid agent names gracefully', () => {
      const invalidAgentNames = ['', null, undefined, 123, {}];
      
      invalidAgentNames.forEach(agentName => {
        expect(() => {
          continuum.sendStatusChannel(agentName as any, 'Working...', mockWebSocket);
        }).not.toThrow();
      });
    });
  });

  describe('Message Content Validation', () => {
    it('should handle empty messages', () => {
      continuum.sendMessageChannel('PlannerAI', '', mockWebSocket);
      continuum.sendStatusChannel('PlannerAI', '', mockWebSocket);
      continuum.sendCommandChannel('PlannerAI', '', mockWebSocket);

      expect(mockWebSocket.send).toHaveBeenCalledTimes(3);
    });

    it('should handle special message content', () => {
      const specialContents = [
        null,
        undefined,
        123,
        { object: 'content' },
        ['array', 'content']
      ];

      specialContents.forEach(content => {
        expect(() => {
          continuum.sendMessageChannel('PlannerAI', content as any, mockWebSocket);
        }).not.toThrow();
      });
    });
  });

  describe('Performance and Concurrency', () => {
    it('should handle rapid successive messages', () => {
      const messageCount = 100;
      
      for (let i = 0; i < messageCount; i++) {
        continuum.sendStatusChannel('PlannerAI', `Status update ${i}`, mockWebSocket);
      }

      expect(mockWebSocket.send).toHaveBeenCalledTimes(messageCount);
    });

    it('should handle concurrent agent communications', async () => {
      const agents = ['PlannerAI', 'CodeAI', 'GeneralAI'];
      const promises: Promise<void>[] = [];

      // Simulate concurrent operations
      for (let i = 0; i < 10; i++) {
        agents.forEach(agent => {
          promises.push(
            Promise.resolve().then(() => {
              continuum.sendCommandChannel(agent, `Command ${i}`, mockWebSocket);
              continuum.sendStatusChannel(agent, `Status ${i}`, mockWebSocket);
              continuum.sendMessageChannel(agent, `Message ${i}`, mockWebSocket);
            })
          );
        });
      }

      await Promise.all(promises);

      // Should have 3 agents × 10 iterations × 3 message types = 90 total messages
      expect(mockWebSocket.send).toHaveBeenCalledTimes(90);
    });
  });
});