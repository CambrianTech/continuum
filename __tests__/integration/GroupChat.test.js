/**
 * Integration Tests for Group Chat Functionality
 * Tests multi-agent conversations and coordination
 */

const WebSocketServer = require('../../src/integrations/WebSocketServer.cjs');
const ContinuumCore = require('../../src/core/continuum-core.cjs');
const WebSocket = require('ws');

describe('Group Chat Integration Tests', () => {
  let continuum;
  let wsServer;
  let mockHttpServer;
  let testWs;

  beforeAll(async () => {
    // Mock environment
    process.env.ANTHROPIC_API_KEY = 'test-key';
    process.env.OPENAI_API_KEY = 'test-key';
    
    continuum = new ContinuumCore();
    
    // Mock HTTP server
    mockHttpServer = {
      on: jest.fn(),
      listen: jest.fn()
    };
    
    wsServer = new WebSocketServer(continuum, mockHttpServer);
    
    // Mock AI responses
    continuum.anthropic = {
      messages: { create: jest.fn() }
    };
    
    continuum.protocolSheriff = {
      validateResponse: jest.fn().mockResolvedValue({
        isValid: true,
        violations: [],
        correctedResponse: null
      })
    };
  });

  beforeEach(() => {
    // Create mock WebSocket connection
    testWs = {
      send: jest.fn(),
      on: jest.fn(),
      readyState: 1 // OPEN
    };
    
    jest.clearAllMocks();
  });

  describe('Single Agent Direct Communication', () => {
    test('should route message to specific agent', async () => {
      const mockResponse = {
        content: [{ text: 'I am PlannerAI and I can help you with strategic planning.' }],
        usage: { input_tokens: 50, output_tokens: 25 }
      };
      
      continuum.anthropic.messages.create.mockResolvedValue(mockResponse);
      continuum.sendTask = jest.fn().mockResolvedValue(mockResponse.content[0].text);

      const message = {
        type: 'direct_message',
        content: 'Hello PlannerAI, can you help me plan a project?',
        agent: 'PlannerAI',
        room: 'general'
      };

      await wsServer.handleMessage(testWs, JSON.stringify(message));

      expect(continuum.sendTask).toHaveBeenCalledWith('PlannerAI', message.content);
      expect(continuum.protocolSheriff.validateResponse).toHaveBeenCalled();
      expect(testWs.send).toHaveBeenCalledWith(
        expect.stringContaining('"agent":"PlannerAI"')
      );
    });

    test('should handle agent-specific errors', async () => {
      continuum.sendTask = jest.fn().mockRejectedValue(new Error('Agent unavailable'));

      const message = {
        type: 'direct_message',
        content: 'Test message',
        agent: 'CodeAI',
        room: 'general'
      };

      await wsServer.handleMessage(testWs, JSON.stringify(message));

      expect(testWs.send).toHaveBeenCalledWith(
        expect.stringContaining('"message":"Sorry, CodeAI encountered an error: Agent unavailable"')
      );
    });
  });

  describe('Group Chat Multi-Agent Communication', () => {
    test('should send message to multiple agents', async () => {
      const agents = ['PlannerAI', 'CodeAI', 'GeneralAI'];
      const responses = [
        'PlannerAI: I can coordinate the overall strategy.',
        'CodeAI: I will handle the technical implementation.',
        'GeneralAI: I can assist with general questions.'
      ];

      continuum.sendTask = jest.fn()
        .mockResolvedValueOnce(responses[0])
        .mockResolvedValueOnce(responses[1])
        .mockResolvedValueOnce(responses[2]);

      const message = {
        type: 'group_message',
        content: 'Help me build a web application',
        agents: agents,
        room: 'general'
      };

      await wsServer.handleMessage(testWs, JSON.stringify(message));

      expect(continuum.sendTask).toHaveBeenCalledTimes(3);
      
      // Check that each agent received the message with group context
      for (let i = 0; i < agents.length; i++) {
        expect(continuum.sendTask).toHaveBeenNthCalledWith(
          i + 1,
          agents[i],
          expect.stringContaining('GROUP CHAT CONTEXT')
        );
      }

      // Check that all responses were sent
      expect(testWs.send).toHaveBeenCalledTimes(3);
      
      // Verify each agent's response
      const sentMessages = testWs.send.mock.calls.map(call => JSON.parse(call[0]));
      expect(sentMessages.map(m => m.agent)).toEqual(agents);
      expect(sentMessages.every(m => m.group_chat === true)).toBe(true);
    });

    test('should handle mixed success and failure in group chat', async () => {
      const agents = ['PlannerAI', 'CodeAI'];
      
      continuum.sendTask = jest.fn()
        .mockResolvedValueOnce('PlannerAI response')
        .mockRejectedValueOnce(new Error('CodeAI failed'));

      const message = {
        type: 'group_message',
        content: 'Test group message',
        agents: agents,
        room: 'general'
      };

      await wsServer.handleMessage(testWs, JSON.stringify(message));

      expect(testWs.send).toHaveBeenCalledTimes(2);
      
      const sentMessages = testWs.send.mock.calls.map(call => JSON.parse(call[0]));
      
      // First response should be successful
      expect(sentMessages[0].message).toBe('PlannerAI response');
      expect(sentMessages[0].sheriff_status).toBe('VALID');
      
      // Second response should contain error
      expect(sentMessages[1].message).toContain('Error: CodeAI failed');
      expect(sentMessages[1].sheriff_status).toBe('ERROR');
    });

    test('should provide group context to each agent', async () => {
      const agents = ['PlannerAI', 'CodeAI', 'GeneralAI'];
      
      continuum.sendTask = jest.fn().mockResolvedValue('Mock response');

      const message = {
        type: 'group_message',
        content: 'Original user message',
        agents: agents,
        room: 'general'
      };

      await wsServer.handleMessage(testWs, JSON.stringify(message));

      // Check that PlannerAI knows about CodeAI and GeneralAI
      expect(continuum.sendTask).toHaveBeenNthCalledWith(
        1,
        'PlannerAI',
        expect.stringContaining('You are in a group chat with CodeAI, GeneralAI')
      );

      // Check that CodeAI knows about PlannerAI and GeneralAI
      expect(continuum.sendTask).toHaveBeenNthCalledWith(
        2,
        'CodeAI',
        expect.stringContaining('You are in a group chat with PlannerAI, GeneralAI')
      );

      // Check that GeneralAI knows about PlannerAI and CodeAI
      expect(continuum.sendTask).toHaveBeenNthCalledWith(
        3,
        'GeneralAI',
        expect.stringContaining('You are in a group chat with PlannerAI, CodeAI')
      );
    });
  });

  describe('Protocol Sheriff in Group Chat', () => {
    test('should validate all group chat responses', async () => {
      const agents = ['PlannerAI', 'CodeAI'];
      
      continuum.sendTask = jest.fn()
        .mockResolvedValueOnce('Good response from PlannerAI')
        .mockResolvedValueOnce('Rambling response from CodeAI that goes off topic...');

      continuum.protocolSheriff.validateResponse = jest.fn()
        .mockResolvedValueOnce({
          isValid: true,
          violations: [],
          correctedResponse: null
        })
        .mockResolvedValueOnce({
          isValid: false,
          violations: ['Off-topic content'],
          correctedResponse: 'Focused response from CodeAI'
        });

      const message = {
        type: 'group_message',
        content: 'Help me with my project',
        agents: agents,
        room: 'general'
      };

      await wsServer.handleMessage(testWs, JSON.stringify(message));

      expect(continuum.protocolSheriff.validateResponse).toHaveBeenCalledTimes(2);
      
      const sentMessages = testWs.send.mock.calls.map(call => JSON.parse(call[0]));
      
      // First response should be unchanged
      expect(sentMessages[0].message).toBe('Good response from PlannerAI');
      expect(sentMessages[0].sheriff_status).toBe('VALID');
      
      // Second response should be corrected
      expect(sentMessages[1].message).toBe('Focused response from CodeAI');
      expect(sentMessages[1].sheriff_status).toBe('CORRECTED');
    });
  });

  describe('Real-World Group Chat Scenarios', () => {
    test('should handle collaborative problem solving', async () => {
      const mockResponses = [
        'PlannerAI: I suggest we break this into three phases: planning, development, and testing.',
        'CodeAI: For the development phase, I recommend using React with TypeScript for better type safety.',
        'GeneralAI: I can help coordinate between the teams and manage documentation.'
      ];

      continuum.sendTask = jest.fn()
        .mockResolvedValueOnce(mockResponses[0])
        .mockResolvedValueOnce(mockResponses[1])
        .mockResolvedValueOnce(mockResponses[2]);

      const message = {
        type: 'group_message',
        content: 'We need to build a new customer dashboard. How should we approach this?',
        agents: ['PlannerAI', 'CodeAI', 'GeneralAI'],
        room: 'general'
      };

      await wsServer.handleMessage(testWs, JSON.stringify(message));

      const sentMessages = testWs.send.mock.calls.map(call => JSON.parse(call[0]));
      
      expect(sentMessages[0].message).toContain('three phases');
      expect(sentMessages[1].message).toContain('React with TypeScript');
      expect(sentMessages[2].message).toContain('coordinate between');
      
      // All should be marked as group chat
      expect(sentMessages.every(m => m.group_chat === true)).toBe(true);
    });

    test('should handle specialized consultations', async () => {
      continuum.sendTask = jest.fn()
        .mockResolvedValueOnce('PlannerAI: I can create a project timeline and resource allocation plan.')
        .mockResolvedValueOnce('CodeAI: I recommend using microservices architecture with Docker containers.');

      const message = {
        type: 'group_message',
        content: 'I need to scale my application to handle 1 million users. What architecture should I use?',
        agents: ['PlannerAI', 'CodeAI'],
        room: 'general'
      };

      await wsServer.handleMessage(testWs, JSON.stringify(message));

      expect(continuum.sendTask).toHaveBeenCalledTimes(2);
      
      const sentMessages = testWs.send.mock.calls.map(call => JSON.parse(call[0]));
      
      expect(sentMessages[0].message).toContain('project timeline');
      expect(sentMessages[1].message).toContain('microservices');
    });
  });

  describe('Error Handling and Edge Cases', () => {
    test('should handle empty agent list', async () => {
      const message = {
        type: 'group_message',
        content: 'Test message',
        agents: [],
        room: 'general'
      };

      await wsServer.handleMessage(testWs, JSON.stringify(message));

      expect(continuum.sendTask).not.toHaveBeenCalled();
      expect(testWs.send).not.toHaveBeenCalled();
    });

    test('should handle invalid agent names', async () => {
      continuum.sendTask = jest.fn()
        .mockResolvedValueOnce('Valid response')
        .mockRejectedValueOnce(new Error('Agent not found: InvalidAI'));

      const message = {
        type: 'group_message',
        content: 'Test message',
        agents: ['PlannerAI', 'InvalidAI'],
        room: 'general'
      };

      await wsServer.handleMessage(testWs, JSON.stringify(message));

      expect(testWs.send).toHaveBeenCalledTimes(2);
      
      const sentMessages = testWs.send.mock.calls.map(call => JSON.parse(call[0]));
      
      expect(sentMessages[0].message).toBe('Valid response');
      expect(sentMessages[1].message).toContain('Agent not found');
    });

    test('should handle concurrent group chats', async () => {
      const testWs2 = {
        send: jest.fn(),
        on: jest.fn(),
        readyState: 1
      };

      continuum.sendTask = jest.fn().mockResolvedValue('Response');

      // First group chat
      const message1 = {
        type: 'group_message',
        content: 'First chat message',
        agents: ['PlannerAI'],
        room: 'general'
      };

      // Second group chat
      const message2 = {
        type: 'group_message',
        content: 'Second chat message',
        agents: ['CodeAI'],
        room: 'general'
      };

      await Promise.all([
        wsServer.handleMessage(testWs, JSON.stringify(message1)),
        wsServer.handleMessage(testWs2, JSON.stringify(message2))
      ]);

      expect(continuum.sendTask).toHaveBeenCalledTimes(2);
      expect(testWs.send).toHaveBeenCalledTimes(1);
      expect(testWs2.send).toHaveBeenCalledTimes(1);
    });
  });

  describe('Performance and Scalability', () => {
    test('should handle large group chats efficiently', async () => {
      const manyAgents = ['PlannerAI', 'CodeAI', 'GeneralAI', 'ProtocolSheriff'];
      
      continuum.sendTask = jest.fn().mockResolvedValue('Quick response');

      const startTime = Date.now();

      const message = {
        type: 'group_message',
        content: 'Performance test message',
        agents: manyAgents,
        room: 'general'
      };

      await wsServer.handleMessage(testWs, JSON.stringify(message));

      const endTime = Date.now();
      const duration = endTime - startTime;

      expect(continuum.sendTask).toHaveBeenCalledTimes(manyAgents.length);
      expect(testWs.send).toHaveBeenCalledTimes(manyAgents.length);
      
      // Should complete within reasonable time
      expect(duration).toBeLessThan(1000); // 1 second
    });

    test('should maintain message order in group responses', async () => {
      const agents = ['Agent1', 'Agent2', 'Agent3'];
      
      continuum.sendTask = jest.fn()
        .mockImplementation((agent) => 
          new Promise(resolve => setTimeout(() => resolve(`Response from ${agent}`), Math.random() * 100))
        );

      const message = {
        type: 'group_message',
        content: 'Order test',
        agents: agents,
        room: 'general'
      };

      await wsServer.handleMessage(testWs, JSON.stringify(message));

      const sentMessages = testWs.send.mock.calls.map(call => JSON.parse(call[0]));
      const responseAgents = sentMessages.map(m => m.agent);
      
      // Responses should be in the order agents were requested
      expect(responseAgents).toEqual(agents);
    });
  });
});