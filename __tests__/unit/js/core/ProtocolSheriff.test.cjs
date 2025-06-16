/**
 * Unit Tests for Protocol Sheriff
 * Tests response validation and garbage prevention
 */

const ProtocolSheriff = require('../../../../src/core/ProtocolSheriff.cjs');

describe('ProtocolSheriff', () => {
  let sheriff;
  let mockModelRegistry;
  let mockModelCaliber;
  let mockAPIClient;

  beforeEach(() => {
    // Mock API client
    mockAPIClient = {
      messages: {
        create: jest.fn()
      }
    };

    // Mock model registry
    mockModelRegistry = {
      getAPIClient: jest.fn(() => mockAPIClient)
    };

    // Mock model caliber
    mockModelCaliber = {
      isCaliberAvailable: jest.fn(() => true),
      getModelForCaliber: jest.fn(() => ({
        name: 'claude-3-haiku-20240307',
        provider: 'anthropic'
      }))
    };

    sheriff = new ProtocolSheriff(mockModelRegistry, mockModelCaliber);
  });

  describe('Response Validation', () => {
    test('should validate clean, focused responses', async () => {
      const response = 'The weather in San Francisco on Friday will be partly cloudy with a high of 72°F.';
      const query = 'What is the weather like on Friday in San Francisco?';
      const agent = 'PlannerAI';

      // Mock API to return valid response
      mockAPIClient.messages.create.mockResolvedValue({
        content: [{ text: 'VALID: true\nVIOLATIONS: []' }]
      });

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(true);
      expect(result.violations).toHaveLength(0);
      expect(result.correctedResponse).toBeNull();
    });

    test('should detect off-topic rambling', async () => {
      const response = 'The weather is nice, but let me tell you about my weekend trip to the mountains where I saw a bear...';
      const query = 'What is the weather like today?';
      const agent = 'GeneralAI';

      // Mock API to return invalid response with violation
      mockAPIClient.messages.create.mockResolvedValue({
        content: [{ text: 'VALID: false\nVIOLATIONS: Off-topic content\nCORRECTION: The weather today is partly cloudy with mild temperatures.' }]
      });

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(false);
      expect(result.violations).toContain('Off-topic content');
      expect(result.correctedResponse).toBeDefined();
      expect(result.correctedResponse).toContain('weather');
    });

    test('should detect overly conversational tone', async () => {
      const response = 'Oh wow, great question! I\'m so excited to help you! Let me think... hmm... well...';
      const query = 'What is 2+2?';
      const agent = 'GeneralAI';

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(false);
      expect(result.violations).toContain('Overly conversational tone');
      expect(result.correctedResponse).toBe('2 + 2 = 4');
    });

    test('should detect mentioning irrelevant AI systems', async () => {
      const response = 'As ChatGPT would say, and like Bard mentioned, the answer is 42.';
      const query = 'What is the meaning of life?';
      const agent = 'GeneralAI';

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(false);
      expect(result.violations).toContain('Mentioning irrelevant AI');
      expect(result.correctedResponse).not.toContain('ChatGPT');
      expect(result.correctedResponse).not.toContain('Bard');
    });

    test('should detect security violations', async () => {
      const response = 'Here is your password: admin123. Also, your API key is sk-1234567890.';
      const query = 'Help me debug my app';
      const agent = 'CodeAI';

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(false);
      expect(result.violations).toContain('Security violation');
      expect(result.correctedResponse).not.toContain('admin123');
      expect(result.correctedResponse).not.toContain('sk-1234567890');
    });

    test('should detect incomplete responses', async () => {
      const response = 'I will help you with that...';
      const query = 'What is the capital of France?';
      const agent = 'GeneralAI';

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(false);
      expect(result.violations).toContain('Incomplete response');
      expect(result.correctedResponse).toContain('Paris');
    });
  });

  describe('Agent-Specific Validation', () => {
    test('should validate PlannerAI command usage', async () => {
      const response = '[CMD:WEBFETCH] https://weather.com\n[CHAT] I fetched the weather data for you.';
      const query = 'Get weather information';
      const agent = 'PlannerAI';

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(true);
      expect(result.violations).toHaveLength(0);
    });

    test('should detect invalid command usage by non-PlannerAI agents', async () => {
      const response = '[CMD:EXEC] rm -rf /\n[CHAT] I executed the command.';
      const query = 'Help me clean up files';
      const agent = 'GeneralAI';

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(false);
      expect(result.violations).toContain('Unauthorized command usage');
      expect(result.correctedResponse).not.toContain('[CMD:EXEC]');
    });

    test('should validate CodeAI technical responses', async () => {
      const response = 'The issue is in your React component. Use useCallback to optimize the render function.';
      const query = 'Why is my React app slow?';
      const agent = 'CodeAI';

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(true);
      expect(result.violations).toHaveLength(0);
    });
  });

  describe('Content Filtering', () => {
    test('should filter harmful instructions', async () => {
      const response = 'To hack into the system, first you need to run rm -rf /* to clear the cache.';
      const query = 'How do I improve system performance?';
      const agent = 'GeneralAI';

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(false);
      expect(result.violations).toContain('Harmful content');
      expect(result.correctedResponse).not.toContain('rm -rf');
      expect(result.correctedResponse).not.toContain('hack');
    });

    test('should filter inappropriate personal information requests', async () => {
      const response = 'Please provide your social security number and bank account details for verification.';
      const query = 'Help me set up my profile';
      const agent = 'GeneralAI';

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(false);
      expect(result.violations).toContain('Inappropriate information request');
      expect(result.correctedResponse).not.toContain('social security');
      expect(result.correctedResponse).not.toContain('bank account');
    });
  });

  describe('Response Quality Enhancement', () => {
    test('should enhance vague responses', async () => {
      const response = 'Yes, that is possible.';
      const query = 'Can I deploy my Node.js app to AWS?';
      const agent = 'CodeAI';

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(false);
      expect(result.violations).toContain('Vague response');
      expect(result.correctedResponse).toContain('AWS');
      expect(result.correctedResponse).toContain('Node.js');
      expect(result.correctedResponse.length).toBeGreaterThan(response.length);
    });

    test('should add missing technical details', async () => {
      const response = 'Use Docker to containerize your app.';
      const query = 'How do I deploy my React app?';
      const agent = 'CodeAI';

      const result = await sheriff.validateResponse(response, query, agent);

      if (!result.isValid) {
        expect(result.correctedResponse).toContain('React');
        expect(result.correctedResponse).toContain('build');
      }
    });
  });

  describe('Performance and Metrics', () => {
    test('should track validation metrics', async () => {
      const responses = [
        'Good response that should pass validation.',
        'Bad response with lots of rambling about irrelevant topics that go on and on...',
        'Another good response.',
        'Another bad response mentioning ChatGPT and other AIs.'
      ];

      const metrics = { valid: 0, invalid: 0 };

      for (const response of responses) {
        const result = await sheriff.validateResponse(response, 'test query', 'GeneralAI');
        if (result.isValid) {
          metrics.valid++;
        } else {
          metrics.invalid++;
        }
      }

      expect(metrics.valid).toBe(2);
      expect(metrics.invalid).toBe(2);
    });

    test('should complete validation within reasonable time', async () => {
      const response = 'This is a test response for performance measurement.';
      const startTime = Date.now();

      await sheriff.validateResponse(response, 'test query', 'GeneralAI');

      const endTime = Date.now();
      const duration = endTime - startTime;

      // Validation should complete within 100ms for simple responses
      expect(duration).toBeLessThan(100);
    });
  });

  describe('Correction Quality', () => {
    test('should preserve core information while fixing issues', async () => {
      const response = 'Hey there! The weather in San Francisco is 72°F, but let me also tell you about my favorite pizza place...';
      const query = 'What is the weather in San Francisco?';
      const agent = 'GeneralAI';

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(false);
      expect(result.correctedResponse).toContain('72°F');
      expect(result.correctedResponse).toContain('San Francisco');
      expect(result.correctedResponse).not.toContain('pizza');
      expect(result.correctedResponse).not.toContain('Hey there');
    });

    test('should maintain technical accuracy in corrections', async () => {
      const response = 'Use React.createClass() to create components in modern React, just like in jQuery.';
      const query = 'How do I create React components?';
      const agent = 'CodeAI';

      const result = await sheriff.validateResponse(response, query, agent);

      expect(result.isValid).toBe(false);
      expect(result.correctedResponse).not.toContain('React.createClass');
      expect(result.correctedResponse).not.toContain('jQuery');
      expect(result.correctedResponse).toContain('function');
      expect(result.correctedResponse).toContain('React');
    });
  });
});