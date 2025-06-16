/**
 * Integration Tests for AI Capabilities
 * Tests multi-step queries and complex problem solving
 */

const ContinuumCore = require('../../src/core/continuum-core.cjs');

describe('AI Capabilities Integration Tests', () => {
  let continuum;

  beforeAll(async () => {
    // Mock environment variables for testing
    process.env.ANTHROPIC_API_KEY = 'test-key';
    process.env.OPENAI_API_KEY = 'test-key';
    
    continuum = new ContinuumCore();
    
    // Mock the actual AI calls to avoid real API usage in tests
    continuum.anthropic = {
      messages: {
        create: jest.fn()
      }
    };
    
    continuum.openai = {
      chat: {
        completions: {
          create: jest.fn()
        }
      }
    };
  });

  describe('PlannerAI Multi-Step Queries', () => {
    test('should handle weather query with location research', async () => {
      // Mock AI response with commands
      const mockResponse = {
        content: [{ 
          text: '[CMD:WEBFETCH] https://weather.com/weather/tenday/l/San+Francisco+CA\n[CHAT] Based on the weather data, Friday will be partly cloudy with a high of 72°F and low of 58°F in San Francisco.' 
        }],
        usage: { input_tokens: 100, output_tokens: 50 }
      };
      
      continuum.anthropic.messages.create.mockResolvedValue(mockResponse);
      
      // Mock command processor
      continuum.commandProcessor.processToolCommands = jest.fn().mockResolvedValue([
        {
          tool: 'WEBFETCH',
          result: 'Weather data: Friday - Partly cloudy, High: 72°F, Low: 58°F'
        }
      ]);

      const result = await continuum.sendTask('PlannerAI', 'What is the weather going to be like on Friday in San Francisco?');
      
      expect(result).toContain('Friday');
      expect(result).toContain('72°F');
      expect(continuum.commandProcessor.processToolCommands).toHaveBeenCalled();
    });

    test('should handle complex file analysis query', async () => {
      const mockResponse = {
        content: [{ 
          text: '[CMD:FILE_READ] package.json\n[CMD:EXEC] npm audit\n[CHAT] I analyzed your package.json and found 3 dependencies that need updates for security.' 
        }],
        usage: { input_tokens: 150, output_tokens: 75 }
      };
      
      continuum.anthropic.messages.create.mockResolvedValue(mockResponse);
      
      continuum.commandProcessor.processToolCommands = jest.fn().mockResolvedValue([
        {
          tool: 'FILE_READ',
          result: '{"dependencies": {"express": "^4.17.1"}}'
        },
        {
          tool: 'EXEC',
          result: 'found 3 vulnerabilities'
        }
      ]);

      const result = await continuum.sendTask('PlannerAI', 'Analyze my project dependencies and check for security issues');
      
      expect(result).toContain('3 dependencies');
      expect(result).toContain('security');
      expect(continuum.commandProcessor.processToolCommands).toHaveBeenCalled();
    });

    test('should handle research and analysis workflow', async () => {
      const mockResponse = {
        content: [{ 
          text: '[CMD:WEBFETCH] https://api.github.com/repos/facebook/react/releases/latest\n[CMD:FILE_WRITE] research.md React Latest: v18.2.0 - Performance improvements\n[CHAT] I researched React\'s latest version and documented the findings. React v18.2.0 includes significant performance improvements.' 
        }],
        usage: { input_tokens: 200, output_tokens: 100 }
      };
      
      continuum.anthropic.messages.create.mockResolvedValue(mockResponse);
      
      continuum.commandProcessor.processToolCommands = jest.fn().mockResolvedValue([
        {
          tool: 'WEBFETCH',
          result: 'React v18.2.0 released with performance improvements'
        },
        {
          tool: 'FILE_WRITE',
          result: 'File written successfully'
        }
      ]);

      const result = await continuum.sendTask('PlannerAI', 'Research the latest React version and document key features');
      
      expect(result).toContain('React');
      expect(result).toContain('18.2.0');
      expect(result).toContain('performance');
      expect(continuum.commandProcessor.processToolCommands).toHaveBeenCalled();
    });
  });

  describe('CodeAI Technical Analysis', () => {
    test('should analyze code and provide recommendations', async () => {
      const mockResponse = {
        content: [{ 
          text: '[CMD:FILE_READ] src/app.js\n[CHAT] I analyzed your code and found 2 performance issues: 1) Missing React.memo for expensive components 2) Unnecessary re-renders in useEffect hooks.' 
        }],
        usage: { input_tokens: 120, output_tokens: 60 }
      };
      
      continuum.anthropic.messages.create.mockResolvedValue(mockResponse);
      
      continuum.commandProcessor.processToolCommands = jest.fn().mockResolvedValue([
        {
          tool: 'FILE_READ',
          result: 'function App() { useEffect(() => { fetch("/api") }, [data]) }'
        }
      ]);

      const result = await continuum.sendTask('CodeAI', 'Analyze my React code for performance issues');
      
      expect(result).toContain('performance');
      expect(result).toContain('React.memo');
      expect(result).toContain('useEffect');
    });
  });

  describe('Group Chat Multi-Agent Coordination', () => {
    test('should coordinate between multiple agents', async () => {
      // Mock responses from different agents
      const plannerResponse = {
        content: [{ text: '[CHAT] I\'ll coordinate this task. CodeAI can handle the analysis while I manage the workflow.' }],
        usage: { input_tokens: 50, output_tokens: 25 }
      };
      
      const codeResponse = {
        content: [{ text: '[CHAT] I\'ve analyzed the code structure. The main issues are in the routing layer.' }],
        usage: { input_tokens: 60, output_tokens: 30 }
      };
      
      continuum.anthropic.messages.create
        .mockResolvedValueOnce(plannerResponse)
        .mockResolvedValueOnce(codeResponse);

      const plannerResult = await continuum.sendTask('PlannerAI', 'Help me refactor my app architecture. [GROUP CHAT CONTEXT: You are in a group chat with CodeAI]');
      const codeResult = await continuum.sendTask('CodeAI', 'Help me refactor my app architecture. [GROUP CHAT CONTEXT: You are in a group chat with PlannerAI]');
      
      expect(plannerResult).toContain('coordinate');
      expect(codeResult).toContain('analyzed');
      expect(codeResult).toContain('routing');
    });
  });

  describe('Protocol Sheriff Validation', () => {
    test('should validate AI responses and correct issues', async () => {
      // Mock Protocol Sheriff
      continuum.protocolSheriff = {
        validateResponse: jest.fn().mockResolvedValue({
          isValid: false,
          violations: ['Mentioning irrelevant topics'],
          correctedResponse: 'Here is a focused response about your specific question.'
        })
      };

      const mockResponse = {
        content: [{ text: 'Let me tell you about my weekend and also here is some random info...' }],
        usage: { input_tokens: 50, output_tokens: 25 }
      };
      
      continuum.anthropic.messages.create.mockResolvedValue(mockResponse);
      continuum.commandProcessor.processToolCommands = jest.fn().mockResolvedValue([]);

      const result = await continuum.sendTask('GeneralAI', 'What is 2+2?');
      
      expect(continuum.protocolSheriff.validateResponse).toHaveBeenCalled();
      // Should return the corrected response, not the original
      expect(result).toContain('focused response');
    });

    test('should pass through valid responses unchanged', async () => {
      continuum.protocolSheriff = {
        validateResponse: jest.fn().mockResolvedValue({
          isValid: true,
          violations: [],
          correctedResponse: null
        })
      };

      const mockResponse = {
        content: [{ text: 'The answer is 4.' }],
        usage: { input_tokens: 30, output_tokens: 15 }
      };
      
      continuum.anthropic.messages.create.mockResolvedValue(mockResponse);
      continuum.commandProcessor.processToolCommands = jest.fn().mockResolvedValue([]);

      const result = await continuum.sendTask('GeneralAI', 'What is 2+2?');
      
      expect(continuum.protocolSheriff.validateResponse).toHaveBeenCalled();
      expect(result).toBe('The answer is 4.');
    });
  });

  describe('Complex Multi-Step Scenarios', () => {
    test('should handle end-to-end project analysis workflow', async () => {
      const responses = [
        {
          content: [{ text: '[CMD:FILE_READ] package.json\n[CMD:EXEC] npm outdated\n[STATUS] Analyzing project dependencies...' }],
          usage: { input_tokens: 100, output_tokens: 50 }
        },
        {
          content: [{ text: '[CHAT] Based on my analysis, your project has 5 outdated dependencies and 2 security vulnerabilities. I recommend updating React and fixing the lodash vulnerability.' }],
          usage: { input_tokens: 150, output_tokens: 75 }
        }
      ];
      
      continuum.anthropic.messages.create
        .mockResolvedValueOnce(responses[0])
        .mockResolvedValueOnce(responses[1]);
      
      continuum.commandProcessor.processToolCommands = jest.fn().mockResolvedValue([
        { tool: 'FILE_READ', result: '{"dependencies": {"react": "16.8.0", "lodash": "4.17.4"}}' },
        { tool: 'EXEC', result: 'Package   Current   Wanted   Latest\nreact     16.8.0    16.14.0  18.2.0\nlodash    4.17.4    4.17.21  4.17.21' }
      ]);

      const result = await continuum.sendTask('PlannerAI', 'Please analyze my project and recommend updates');
      
      expect(result).toContain('5 outdated dependencies');
      expect(result).toContain('React');
      expect(result).toContain('lodash');
      expect(result).toContain('security');
    });

    test('should handle research, analysis, and documentation workflow', async () => {
      const responses = [
        {
          content: [{ text: '[CMD:WEBFETCH] https://nodejs.org/api/fs.html\n[STATUS] Researching Node.js file system best practices...' }],
          usage: { input_tokens: 80, output_tokens: 40 }
        },
        {
          content: [{ text: '[CMD:FILE_WRITE] fs-best-practices.md # Node.js File System Best Practices\n\n## Key Points\n- Use async methods\n- Handle errors properly\n[CHAT] I\'ve researched Node.js file system best practices and created a comprehensive guide.' }],
          usage: { input_tokens: 200, output_tokens: 100 }
        }
      ];
      
      continuum.anthropic.messages.create
        .mockResolvedValueOnce(responses[0])
        .mockResolvedValueOnce(responses[1]);
      
      continuum.commandProcessor.processToolCommands = jest.fn()
        .mockResolvedValueOnce([{ tool: 'WEBFETCH', result: 'Node.js fs module documentation: Use async methods for better performance...' }])
        .mockResolvedValueOnce([{ tool: 'FILE_WRITE', result: 'File written successfully' }]);

      const result = await continuum.sendTask('PlannerAI', 'Research Node.js file system best practices and create documentation');
      
      expect(result).toContain('Node.js');
      expect(result).toContain('best practices');
      expect(result).toContain('comprehensive guide');
      expect(continuum.commandProcessor.processToolCommands).toHaveBeenCalledTimes(2);
    });
  });

  describe('Error Handling and Resilience', () => {
    test('should handle API failures gracefully', async () => {
      continuum.anthropic.messages.create.mockRejectedValue(new Error('API Rate Limit'));

      await expect(continuum.sendTask('PlannerAI', 'Test query')).rejects.toThrow('API Rate Limit');
    });

    test('should handle command execution failures', async () => {
      const mockResponse = {
        content: [{ text: '[CMD:EXEC] invalid-command\n[CHAT] Attempting to run command...' }],
        usage: { input_tokens: 50, output_tokens: 25 }
      };
      
      continuum.anthropic.messages.create.mockResolvedValue(mockResponse);
      continuum.commandProcessor.processToolCommands = jest.fn().mockResolvedValue([
        { tool: 'EXEC', result: 'Error: Command not found', error: true }
      ]);

      const result = await continuum.sendTask('PlannerAI', 'Run some command');
      
      // Should handle the error gracefully and still return a response
      expect(result).toBeDefined();
      expect(continuum.commandProcessor.processToolCommands).toHaveBeenCalled();
    });
  });

  afterEach(() => {
    jest.clearAllMocks();
  });
});